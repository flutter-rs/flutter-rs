//! The raw platform-message plumbing the channel layer is built on.
//!
//! Everything here sits directly on the embedder API's `FlutterPlatformMessage`
//! boundary. Two rules shape the types in this module:
//!
//! 1. The engine hands an incoming message in as a *borrowed* C struct that is
//!    only valid for the duration of its callback, so [`PlatformMessage`]
//!    borrows rather than owns and can only be built through an `unsafe fn`.
//! 2. Every response handle has exactly one correct ending. A handle that came
//!    in with a message must be answered once; a handle this crate created to
//!    receive Dart's reply must be released again. Doing neither leaks memory
//!    inside the engine (flutter-engine.h: "Not calling release on the handle
//!    results in a small memory leak").

use crate::{FlutterEngine, FlutterEngineWeakRef, log_engine_result};
use flutter_engine_sys::{
    FlutterEngineResult, FlutterPlatformMessage, FlutterPlatformMessageResponseHandle,
};
use log::error;
use std::borrow::Cow;
use std::ffi::{CStr, CString, NulError};
use std::marker::PhantomData;
use std::os::raw::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::{mem, ptr};

/// The reply callback as it is stored behind a response handle's user data.
type ResponseType = Box<dyn FnOnce(&[u8]) + Send>;

/// A handle the engine uses to match a reply to a message.
///
/// Two different things wear this type, and they end differently:
///
/// * a handle the engine passed in with an incoming [`PlatformMessage`], which
///   must be answered exactly once — see
///   [`Channel::send_response`](crate::channel::Channel::send_response);
/// * a handle created with [`PlatformMessageResponseHandle::new`] to receive
///   Dart's reply to an outgoing message, which this crate releases again once
///   the message has been sent.
///
/// Dropping an unanswered incoming handle leaks engine memory, so [`Drop`] logs
/// an error when that happens.
#[derive(Debug)]
pub struct PlatformMessageResponseHandle {
    handle: *const FlutterPlatformMessageResponseHandle,
    /// `Some` for a handle this crate created and therefore has to release
    /// again, `None` for one the engine handed in with a message.
    owner: Option<FlutterEngineWeakRef>,
}

// SAFETY: `handle` is an opaque, engine-owned handle. Nothing in this crate
// dereferences it — it is only ever handed back to the embedder API, which
// reference-counts these handles internally — so moving one between threads
// cannot create an unsynchronised access. `FlutterEngineWeakRef` is `Send` for
// the reasons documented at its own `unsafe impl`.
unsafe impl Send for PlatformMessageResponseHandle {}

// SAFETY: see the `Send` impl above; the handle has no interior mutability, so
// a shared reference to one hands out no access at all.
unsafe impl Sync for PlatformMessageResponseHandle {}

impl PlatformMessageResponseHandle {
    /// Create a handle that calls `callback` with Dart's reply to an outgoing
    /// message.
    ///
    /// Returns `None` when the engine refuses to create a handle or has already
    /// been shut down; nothing is registered in that case and `callback` will
    /// never run. The handle is released again as soon as the message it was
    /// attached to has been sent, which is what the embedder API asks for.
    ///
    /// `callback` runs on an engine thread, inside an `extern "C"` callback. A
    /// panic in it is caught and logged rather than left to abort the process.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    #[must_use]
    pub fn new<F>(engine: &FlutterEngine, callback: F) -> Option<Self>
    where
        F: FnOnce(&[u8]) + 'static + Send,
    {
        assert!(
            engine.is_platform_thread(),
            "PlatformMessageResponseHandle::new must be called on the platform thread"
        );
        if engine.is_shut_down() {
            error!("cannot create a response handle after FlutterEngine::shutdown");
            return None;
        }

        let user_data = into_user_data(callback);
        let mut handle: *mut FlutterPlatformMessageResponseHandle = ptr::null_mut();
        // SAFETY: the engine handle is live, `response_handle_callback` has the
        // `FlutterDataCallback` signature and reclaims exactly the
        // `Box<ResponseType>` passed as `user_data`, and `handle` is a live
        // out-parameter for the duration of the call.
        let result = unsafe {
            flutter_engine_sys::FlutterPlatformMessageCreateResponseHandle(
                engine.engine_ptr(),
                Some(response_handle_callback),
                user_data,
                &mut handle,
            )
        };

        if result != FlutterEngineResult::kSuccess || handle.is_null() {
            error!("FlutterPlatformMessageCreateResponseHandle failed: {result:?}");
            // The engine did not register the callback, so take the box back
            // instead of leaking it.
            // SAFETY: `user_data` is the `Box<ResponseType>` leaked just above
            // and ownership never reached the engine.
            drop(unsafe { Box::from_raw(user_data.cast::<ResponseType>()) });
            return None;
        }

        Some(Self {
            handle,
            owner: Some(engine.downgrade()),
        })
    }

    /// Adopt a response handle the engine passed in with an incoming message.
    ///
    /// # Safety
    ///
    /// `handle` must be a live, not-yet-answered response handle produced by
    /// the engine. The caller gives up ownership: the returned value must be
    /// answered exactly once, or the engine leaks the memory behind it.
    pub(crate) unsafe fn from_raw(handle: *const FlutterPlatformMessageResponseHandle) -> Self {
        Self {
            handle,
            owner: None,
        }
    }
}

impl Drop for PlatformMessageResponseHandle {
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }
        let Some(owner) = &self.owner else {
            error!(
                "a message response handle was dropped without sending a response! This WILL leak memory inside the engine."
            );
            return;
        };
        // A handle of our own has to be collected again whether or not Dart
        // ever replied. An engine that is gone or shut down has already freed
        // its side of it.
        let Some(engine) = owner.upgrade().filter(|engine| !engine.is_shut_down()) else {
            return;
        };

        // SAFETY: the engine is alive and not shut down, and `handle` is a
        // handle this crate created with
        // `FlutterPlatformMessageCreateResponseHandle` and has not released
        // yet. The engine keeps its own reference while a message carrying the
        // handle is in flight, so releasing here is exactly the lifecycle the
        // header documents.
        let result = unsafe {
            flutter_engine_sys::FlutterPlatformMessageReleaseResponseHandle(
                engine.engine_ptr(),
                self.handle.cast_mut(),
            )
        };
        log_engine_result(result, "FlutterPlatformMessageReleaseResponseHandle");
    }
}

impl From<PlatformMessageResponseHandle> for *const FlutterPlatformMessageResponseHandle {
    /// Take the raw handle out to answer it, leaving the wrapper empty so its
    /// [`Drop`] neither reports an unanswered message nor releases the handle.
    ///
    /// Only correct for a handle the engine passed in with a message: answering
    /// is what collects such a handle.
    fn from(mut handle: PlatformMessageResponseHandle) -> Self {
        debug_assert!(
            handle.owner.is_none(),
            "a handle this crate created must be released, not answered"
        );
        let ptr = handle.handle;
        handle.handle = ptr::null();
        ptr
    }
}

/// Leak `callback` as the `user_data` baton the engine hands back to
/// [`response_handle_callback`].
///
/// The double box is load-bearing: the baton is a thin `*mut c_void`, so the
/// *fat* `Box<dyn FnOnce(..)>` needs one more indirection to fit through it.
/// Boxing the closure without naming [`ResponseType`] produces a `Box<Box<F>>`
/// — a thin pointer in a thin allocation — which the callback below then reads
/// back as a fat pointer, eight bytes past the end of the allocation. That is a
/// segfault on the first reply, so the annotation stays.
fn into_user_data<F>(callback: F) -> *mut c_void
where
    F: FnOnce(&[u8]) + 'static + Send,
{
    let callback: ResponseType = Box::new(callback);
    Box::into_raw(Box::new(callback)).cast::<c_void>()
}

/// The engine's reply callback for a handle made by
/// [`PlatformMessageResponseHandle::new`].
///
/// # Safety
///
/// `user_data` must be the pointer [`into_user_data`] produced, and the engine
/// must call this at most once per handle, since the call reclaims that box.
/// `data` must point at `size` initialised bytes, or be null when `size` is 0.
unsafe extern "C" fn response_handle_callback(
    data: *const u8,
    size: usize,
    user_data: *mut c_void,
) {
    log::trace!("response_handle_callback");
    // SAFETY: the caller guarantees `user_data` is the `Box<ResponseType>`
    // leaked by `into_user_data` and that this runs at most once for it, so
    // reclaiming it here frees the callback exactly once.
    let callback = unsafe { Box::from_raw(user_data.cast::<ResponseType>()) };
    let message = if data.is_null() {
        // The engine reports an empty reply as a null pointer with size 0, and
        // `slice::from_raw_parts` may not be called with a null pointer even
        // for a zero-length slice.
        &[][..]
    } else {
        // SAFETY: the caller guarantees `data` points at `size` initialised
        // bytes, which the engine keeps alive for the duration of this call.
        unsafe { std::slice::from_raw_parts(data, size) }
    };

    // A panic must not reach the engine's C++ frames: unwinding across the
    // `extern "C"` boundary aborts the process, so a panicking reply callback is
    // caught and logged instead.
    if catch_unwind(AssertUnwindSafe(|| callback(message))).is_err() {
        error!("panic in a platform message reply callback");
    }
}

/// A message on its way to or from Dart.
///
/// Both the channel name and the payload may be borrowed — from the engine's
/// own buffers for an incoming message, from the sender's for an outgoing one.
#[derive(Debug)]
pub struct PlatformMessage<'a, 'b> {
    /// The channel the message is addressed to.
    pub channel: Cow<'a, str>,
    /// The encoded message body.
    pub message: &'b [u8],
    /// The handle a reply has to be sent on, if the sender asked for one.
    pub response_handle: Option<PlatformMessageResponseHandle>,
}

impl<'a> PlatformMessage<'a, 'a> {
    /// Borrow an incoming message out of the engine's `FlutterPlatformMessage`.
    ///
    /// # Safety
    ///
    /// `msg.channel` must point at a NUL-terminated C string and `msg.message`
    /// at `msg.message_size` initialised bytes (or be null when the size is 0).
    /// Both must stay valid for `'a`, which must therefore not outlive the
    /// engine callback that produced `msg`. `msg.response_handle` must be null
    /// or a live, not-yet-answered handle whose ownership passes to the returned
    /// value.
    pub(crate) unsafe fn from_raw(msg: &'a FlutterPlatformMessage) -> Self {
        debug_assert_eq!(msg.struct_size, mem::size_of::<FlutterPlatformMessage>());

        // SAFETY: the caller guarantees `channel` is a NUL-terminated string
        // that stays valid for `'a`.
        let channel = unsafe { CStr::from_ptr(msg.channel) }.to_string_lossy();
        let message = if msg.message.is_null() {
            // The engine passes a null pointer with size 0 for an empty
            // message, which `slice::from_raw_parts` does not accept.
            &[][..]
        } else {
            // SAFETY: the caller guarantees `message` points at `message_size`
            // initialised bytes that stay valid for `'a`.
            unsafe { std::slice::from_raw_parts(msg.message, msg.message_size) }
        };
        let response_handle = if msg.response_handle.is_null() {
            None
        } else {
            // SAFETY: the caller guarantees the handle is live and unanswered,
            // and hands its ownership to us.
            Some(unsafe { PlatformMessageResponseHandle::from_raw(msg.response_handle) })
        };

        Self {
            channel,
            message,
            response_handle,
        }
    }
}

impl<'b> PlatformMessage<'_, 'b> {
    /// Lay the message out the way the engine wants it, keeping everything the
    /// raw struct points at alive alongside it.
    ///
    /// # Errors
    ///
    /// Fails if the channel name contains an interior NUL byte, which no
    /// Flutter channel name may.
    pub(crate) fn into_raw(mut self) -> Result<RawPlatformMessage<'b>, NulError> {
        let channel = CString::new(self.channel.as_bytes())?;
        let response_handle = self.response_handle.take();
        Ok(RawPlatformMessage {
            raw: FlutterPlatformMessage {
                struct_size: mem::size_of::<FlutterPlatformMessage>(),
                // Moving the `CString` into the struct below moves the handle,
                // not the heap buffer, so this pointer stays valid.
                channel: channel.as_ptr(),
                message: self.message.as_ptr(),
                message_size: self.message.len(),
                // Borrowed, not taken: `response_handle` stays owned by the
                // value below so that dropping it releases the engine's handle.
                response_handle: response_handle
                    .as_ref()
                    .map_or(ptr::null(), |handle| handle.handle),
            },
            channel,
            _response_handle: response_handle,
            message: PhantomData,
        })
    }
}

/// An outgoing [`PlatformMessage`] in the engine's own layout, together with
/// everything its raw struct points at.
///
/// The engine copies the channel name into a `std::string` of its own during
/// `FlutterEngineSendPlatformMessage`, so this only has to survive that one
/// call — and it must, because the raw struct borrows from it. Dropping it also
/// releases the response handle this crate created for the message, which the
/// embedder API allows "immediately after a call to
/// `FlutterEngineSendPlatformMessage`".
pub(crate) struct RawPlatformMessage<'b> {
    raw: FlutterPlatformMessage,
    /// Owns the bytes `raw.channel` points at.
    channel: CString,
    /// Owns the handle `raw.response_handle` points at, if any. Never read:
    /// what matters is that dropping it releases the engine's handle.
    _response_handle: Option<PlatformMessageResponseHandle>,
    /// Ties the value to the payload `raw.message` points at.
    message: PhantomData<&'b [u8]>,
}

impl RawPlatformMessage<'_> {
    /// The engine's view of the message.
    pub(crate) fn as_ffi(&self) -> &FlutterPlatformMessage {
        &self.raw
    }

    /// The channel name, for log messages on the send path.
    pub(crate) fn channel(&self) -> Cow<'_, str> {
        self.channel.to_string_lossy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// The engine's baton is a thin `*mut c_void`, so the fat `dyn FnOnce` must
    /// be stored behind a second box. Boxing the closure without naming
    /// `ResponseType` made the outer allocation eight bytes wide and the
    /// callback then read a sixteen-byte fat pointer out of it, which segfaults
    /// (exit 139) on the first reply from Dart.
    #[test]
    fn a_reply_callback_runs_with_the_bytes_the_engine_supplied() {
        let (sender, receiver) = mpsc::channel();
        let user_data = into_user_data(move |data: &[u8]| {
            sender.send(data.to_vec()).expect("receiver is alive");
        });

        let reply = [1_u8, 2, 3];
        // SAFETY: `user_data` is the box `into_user_data` just leaked, this is
        // its only call, and `reply` is three readable bytes.
        unsafe { response_handle_callback(reply.as_ptr(), reply.len(), user_data) };

        assert_eq!(receiver.try_recv().expect("the callback ran"), reply);
    }

    /// An empty reply arrives as a null pointer with size 0, which
    /// `slice::from_raw_parts` is not allowed to see.
    #[test]
    fn an_empty_reply_is_delivered_as_an_empty_slice() {
        let (sender, receiver) = mpsc::channel();
        let user_data = into_user_data(move |data: &[u8]| {
            sender.send(data.len()).expect("receiver is alive");
        });

        // SAFETY: as above; a null pointer with size 0 is what the engine sends
        // for an empty reply.
        unsafe { response_handle_callback(ptr::null(), 0, user_data) };

        assert_eq!(receiver.try_recv().expect("the callback ran"), 0);
    }

    /// The callback runs on an engine thread, so a panic in it would abort the
    /// process rather than unwind. (The panic message on stderr is expected.)
    #[test]
    fn a_panicking_reply_callback_does_not_escape_the_ffi_boundary() {
        let user_data = into_user_data(|_: &[u8]| panic!("a plugin's reply callback panicked"));

        // SAFETY: as above.
        unsafe { response_handle_callback(ptr::null(), 0, user_data) };
    }
}
