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
use log::{debug, error, trace};
use parking_lot::Mutex;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString, NulError};
use std::marker::PhantomData;
use std::os::raw::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{mem, ptr};

/// The reply callback a response handle this crate created stands for.
type ResponseType = Box<dyn FnOnce(&[u8]) + Send>;

/// Identifies the engine a pending reply belongs to.
///
/// A counter rather than the engine pointer: a `shutdown` has to reclaim its
/// own engine's callbacks and leave every other engine's alone, and an engine
/// pointer can be reused by a later engine in the same process.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReplyScope(usize);

impl ReplyScope {
    /// A scope no other engine shares.
    pub(crate) fn next() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(1);
        // `Relaxed` is enough: nothing is ordered against this, the only
        // property needed is that no two calls return the same number, which
        // `fetch_add` guarantees at every ordering.
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// A token identifying one outstanding reply callback.
///
/// This, not a pointer, is what the engine carries as the `user_data` of a
/// response handle this crate created, and it is never dereferenced. That is
/// the whole point: a token the engine hands back after its entry has already
/// been reclaimed simply misses [`PENDING_REPLIES`], where a stale pointer
/// would be a use-after-free.
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ReplyId(usize);

impl ReplyId {
    fn next() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(1);
        // See `ReplyScope::next` on the ordering.
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    /// The baton to hand the engine.
    fn into_user_data(self) -> *mut c_void {
        // An integer, not an address: `without_provenance_mut` says so instead
        // of laundering a pointer through a cast.
        ptr::without_provenance_mut(self.0)
    }

    /// The token behind a baton the engine handed back.
    fn from_user_data(user_data: *mut c_void) -> Self {
        Self(user_data.addr())
    }
}

/// One reply callback the engine has not delivered yet.
struct PendingReply {
    scope: ReplyScope,
    callback: ResponseType,
}

/// Every reply callback that has been handed to an engine and not delivered
/// yet, in the whole process.
///
/// # Why one global table instead of a field on the engine
///
/// [`response_handle_callback`] is given nothing but the `user_data` baton —
/// no engine handle, no channel, no context of any kind — so whatever the
/// baton is has to be resolvable from nowhere. A `Box` pointer is resolvable,
/// which is what this used to be, and that is exactly why it leaked: nothing
/// but the callback itself could ever reclaim it, and the engine destroys its
/// response object *without* calling back when the send fails or when it is
/// shut down with a message in flight. A token into one process-wide table is
/// resolvable too, and can also be reclaimed from the outside.
///
/// # Threading
///
/// * Registration happens in [`PlatformMessageResponseHandle::new`], on the
///   platform thread.
/// * Delivery happens in [`response_handle_callback`], on an engine thread.
/// * Reclaiming happens in the handle's [`Drop`] when the message it was
///   attached to never reached the engine, and in [`drain_pending_replies`]
///   from `FlutterEngine::shutdown`, after `FlutterEngineShutdown` has joined
///   the engine's threads. Both normally run on the platform thread, but
///   neither is required to: a handle is [`Send`], and the engine's own `Drop`
///   drains as a backstop on whichever thread released the last handle.
///
/// The mutex orders those three against each other, and the invariant that
/// makes them safe in any order is that a callback is always *moved out* of
/// the table before it is run or dropped: it therefore ends exactly once, and
/// a token whose entry is gone is a no-op rather than a double free. A
/// callback never runs or drops with the lock held, because it is user code
/// that may send another message — which would register a new reply and
/// deadlock a non-reentrant mutex.
static PENDING_REPLIES: Mutex<BTreeMap<ReplyId, PendingReply>> = Mutex::new(BTreeMap::new());

/// Register `callback` for `scope` and return the token to hand the engine.
fn register_pending_reply(scope: ReplyScope, callback: ResponseType) -> ReplyId {
    let id = ReplyId::next();
    // The guard is a temporary of this statement, so it is released before
    // `displaced` is dropped at the end of the function.
    let displaced = PENDING_REPLIES
        .lock()
        .insert(id, PendingReply { scope, callback });
    if displaced.is_some() {
        // Only reachable if the token counter wrapped all the way round while
        // an entry was still outstanding.
        error!("reply token {id:?} was already in use; the older reply callback was dropped");
    }
    id
}

/// Take the callback registered under `id` out of the table, if it is still
/// there.
fn take_pending_reply(id: ReplyId) -> Option<ResponseType> {
    PENDING_REPLIES
        .lock()
        .remove(&id)
        .map(|pending| pending.callback)
}

/// Reclaim every reply callback still outstanding for `scope`.
///
/// Called from `FlutterEngine::shutdown` once `FlutterEngineShutdown` has
/// returned — at which point the engine's threads are joined, so no callback
/// can still be running and none can arrive later — and again when the last
/// engine handle is dropped without a shutdown, which is the only other way
/// the table could keep entries nobody can reach. Whatever is left is a reply
/// the engine will never deliver, so the callbacks are dropped without being
/// called.
pub(crate) fn drain_pending_replies(scope: ReplyScope) {
    // Collected under the lock and dropped after it is released: dropping a
    // callback runs the destructors of whatever it captured.
    let mut table = PENDING_REPLIES.lock();
    let ids: Vec<ReplyId> = table
        .iter()
        .filter(|(_, pending)| pending.scope == scope)
        .map(|(id, _)| *id)
        .collect();
    let drained: Vec<ResponseType> = ids
        .iter()
        .filter_map(|id| table.remove(id))
        .map(|pending| pending.callback)
        .collect();
    drop(table);

    if !drained.is_empty() {
        debug!(
            "reclaimed {} platform message reply callback(s) the engine never delivered",
            drained.len()
        );
    }
}

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
/// an error when that happens. A handle of this crate's own is safe to drop at
/// any point: dropping one whose message never reached the engine reclaims the
/// reply callback waiting behind it, instead of leaving it outstanding for the
/// life of the engine, which is what used to happen.
#[derive(Debug)]
pub struct PlatformMessageResponseHandle {
    handle: *const FlutterPlatformMessageResponseHandle,
    /// `Some` for a handle this crate created and therefore has to release
    /// again, `None` for one the engine handed in with a message.
    owner: Option<FlutterEngineWeakRef>,
    /// The reply callback this handle is still responsible for reclaiming.
    ///
    /// Set, for a handle this crate created, until the message it was attached
    /// to has reached the engine. From that point the engine owns the response
    /// object and will either complete it — running the callback — or destroy
    /// it during shutdown, where [`drain_pending_replies`] reclaims it; that is
    /// where [`disarm_reply`](Self::disarm_reply) clears this field. A handle
    /// dropped while it is still set means the message never went anywhere, so
    /// nothing will ever deliver that reply and the callback has to be
    /// reclaimed here.
    reply_id: Option<ReplyId>,
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
    /// It is dropped without being called if the reply never comes — because
    /// the send failed, or because the engine was shut down with the message
    /// still in flight — so a caller waiting on it has to cope with never
    /// hearing back.
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
        // Both this check and `FlutterEngine::shutdown` run on the platform
        // thread, so a shutdown cannot land between the two and leave an entry
        // nothing drains.
        if engine.is_shut_down() {
            error!("cannot create a response handle after FlutterEngine::shutdown");
            return None;
        }

        let reply_id = register_pending_reply(engine.reply_scope(), Box::new(callback));
        let mut handle: *mut FlutterPlatformMessageResponseHandle = ptr::null_mut();
        // SAFETY: the engine handle is live, `response_handle_callback` has the
        // `FlutterDataCallback` signature, `user_data` is a plain integer token
        // that is never dereferenced, and `handle` is a live out-parameter for
        // the duration of the call.
        let result = unsafe {
            flutter_engine_sys::FlutterPlatformMessageCreateResponseHandle(
                engine.engine_ptr(),
                Some(response_handle_callback),
                reply_id.into_user_data(),
                &mut handle,
            )
        };

        if result != FlutterEngineResult::kSuccess || handle.is_null() {
            error!("FlutterPlatformMessageCreateResponseHandle failed: {result:?}");
            // The engine did not take the token, so nothing will ever hand it
            // back: reclaim the callback now rather than leave it outstanding
            // until shutdown.
            drop(take_pending_reply(reply_id));
            return None;
        }

        Some(Self {
            handle,
            owner: Some(engine.downgrade()),
            reply_id: Some(reply_id),
        })
    }

    /// Hand responsibility for the pending reply to the engine.
    ///
    /// Called once `FlutterEngineSendPlatformMessage` has accepted the message
    /// this handle was attached to. From then on the engine's own reply, or
    /// [`drain_pending_replies`] at shutdown, is what ends the callback, and
    /// dropping this handle must not cancel it.
    pub(crate) fn disarm_reply(&mut self) {
        self.reply_id = None;
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
            // The engine created this one; there is no callback of ours behind
            // it, and answering it is what collects it.
            reply_id: None,
        }
    }
}

impl Drop for PlatformMessageResponseHandle {
    fn drop(&mut self) {
        let Some(owner) = &self.owner else {
            // A handle the engine passed in with a message. Answering it is
            // what collects it, and a null handle means it was answered.
            if !self.handle.is_null() {
                error!(
                    "a message response handle was dropped without sending a response! This WILL leak memory inside the engine."
                );
            }
            return;
        };

        // A handle of our own has to be collected again whether or not Dart
        // ever replied. An engine that is gone or shut down has already freed
        // its side of it.
        if let Some(engine) = owner.upgrade().filter(|engine| !engine.is_shut_down()) {
            // SAFETY: the engine is alive and not shut down, and `handle` is a
            // handle this crate created with
            // `FlutterPlatformMessageCreateResponseHandle` and has not released
            // yet. The engine keeps its own reference while a message carrying
            // the handle is in flight, so releasing here is exactly the
            // lifecycle the header documents.
            let result = unsafe {
                flutter_engine_sys::FlutterPlatformMessageReleaseResponseHandle(
                    engine.engine_ptr(),
                    self.handle.cast_mut(),
                )
            };
            log_engine_result(result, "FlutterPlatformMessageReleaseResponseHandle");
        }

        // Still armed, so the message this handle was attached to never
        // reached the engine and nothing will ever deliver its reply: reclaim
        // the callback instead of leaving it outstanding for the lifetime of
        // the engine. Done *after* the release above and outside the table's
        // lock, so that a release which did complete the response gets the
        // callback first and this is the no-op.
        if let Some(reply_id) = self.reply_id.take() {
            drop(take_pending_reply(reply_id));
        }
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

/// The engine's reply callback for a handle made by
/// [`PlatformMessageResponseHandle::new`].
///
/// `user_data` is a [`ReplyId`] token, not a pointer, so a baton that is stale
/// — the entry was reclaimed because the send failed or the engine shut down —
/// or repeated is not a safety problem: the lookup misses and the call does
/// nothing. That is what keeps this sound against the engine's own lifecycle.
///
/// # Safety
///
/// `data` must point at `size` initialised bytes, or be null when `size` is 0.
unsafe extern "C" fn response_handle_callback(
    data: *const u8,
    size: usize,
    user_data: *mut c_void,
) {
    trace!("response_handle_callback");
    let reply_id = ReplyId::from_user_data(user_data);
    let Some(callback) = take_pending_reply(reply_id) else {
        trace!("a platform message reply arrived for {reply_id:?}, which is no longer outstanding");
        return;
    };
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
            response_handle,
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
    /// Owns the handle `raw.response_handle` points at, if any; dropping it
    /// releases the engine's handle and, unless
    /// [`disarm_reply`](Self::disarm_reply) was called, cancels the reply
    /// callback that was waiting for a message that never got sent.
    response_handle: Option<PlatformMessageResponseHandle>,
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

    /// Record that the engine has accepted this message, so that dropping it
    /// no longer cancels the pending reply. See
    /// [`PlatformMessageResponseHandle::disarm_reply`].
    pub(crate) fn disarm_reply(&mut self) {
        if let Some(handle) = &mut self.response_handle {
            handle.disarm_reply();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;

    /// Registers a callback the way `PlatformMessageResponseHandle::new` does,
    /// without an engine, and hands back the baton the engine would carry.
    fn register<F>(scope: ReplyScope, callback: F) -> *mut c_void
    where
        F: FnOnce(&[u8]) + 'static + Send,
    {
        register_pending_reply(scope, Box::new(callback)).into_user_data()
    }

    /// Reports whether it was dropped, so a test can tell "reclaimed" from
    /// "leaked" without a live engine.
    struct DropFlag(Arc<AtomicBool>);

    impl DropFlag {
        fn new() -> (Self, Arc<AtomicBool>) {
            let flag = Arc::new(AtomicBool::new(false));
            (Self(Arc::clone(&flag)), flag)
        }
    }

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    #[test]
    fn a_reply_callback_runs_with_the_bytes_the_engine_supplied() {
        let (sender, receiver) = mpsc::channel();
        let user_data = register(ReplyScope::next(), move |data: &[u8]| {
            sender.send(data.to_vec()).expect("receiver is alive");
        });

        let reply = [1_u8, 2, 3];
        // SAFETY: `reply` is three readable bytes; `user_data` is the token
        // just registered, which this call reclaims.
        unsafe { response_handle_callback(reply.as_ptr(), reply.len(), user_data) };

        assert_eq!(receiver.try_recv().expect("the callback ran"), reply);
    }

    /// An empty reply arrives as a null pointer with size 0, which
    /// `slice::from_raw_parts` is not allowed to see.
    #[test]
    fn an_empty_reply_is_delivered_as_an_empty_slice() {
        let (sender, receiver) = mpsc::channel();
        let user_data = register(ReplyScope::next(), move |data: &[u8]| {
            sender.send(data.len()).expect("receiver is alive");
        });

        // SAFETY: a null pointer with size 0 is what the engine sends for an
        // empty reply.
        unsafe { response_handle_callback(ptr::null(), 0, user_data) };

        assert_eq!(receiver.try_recv().expect("the callback ran"), 0);
    }

    /// The callback runs on an engine thread, so a panic in it would abort the
    /// process rather than unwind. (The panic message on stderr is expected.)
    #[test]
    fn a_panicking_reply_callback_does_not_escape_the_ffi_boundary() {
        let user_data = register(ReplyScope::next(), |_: &[u8]| {
            panic!("a plugin's reply callback panicked")
        });

        // SAFETY: as above.
        unsafe { response_handle_callback(ptr::null(), 0, user_data) };
    }

    /// The old design reclaimed a `Box` inside the callback, so a second
    /// delivery for the same handle would have been a double free. A token
    /// cannot be: its entry is already gone.
    #[test]
    fn a_reply_delivered_twice_runs_the_callback_once() {
        let (sender, receiver) = mpsc::channel();
        let user_data = register(ReplyScope::next(), move |_: &[u8]| {
            sender.send(()).expect("receiver is alive");
        });

        for _ in 0..2 {
            // SAFETY: a null pointer with size 0 is an empty reply; the second
            // call carries a token whose entry the first one removed.
            unsafe { response_handle_callback(ptr::null(), 0, user_data) };
        }

        assert!(receiver.try_recv().is_ok(), "the callback ran");
        assert!(
            receiver.try_recv().is_err(),
            "the callback must not run a second time"
        );
    }

    /// Every token the engine could hand back after a drain or a failed send
    /// lands here, including the null baton.
    #[test]
    fn a_stale_token_is_ignored() {
        for user_data in [ptr::null_mut(), ptr::without_provenance_mut(usize::MAX)] {
            // SAFETY: a null pointer with size 0 is an empty reply, and no
            // entry is registered for either token.
            unsafe { response_handle_callback(ptr::null(), 0, user_data) };
        }
    }

    /// What `FlutterEngine::shutdown` relies on: a drain takes this engine's
    /// outstanding callbacks and drops them without calling them, and leaves
    /// every other engine's alone.
    #[test]
    fn draining_a_scope_reclaims_only_its_own_callbacks() {
        let (mine, mine_dropped) = DropFlag::new();
        let (theirs, theirs_dropped) = DropFlag::new();
        let mine_ran = Arc::new(AtomicBool::new(false));

        let my_scope = ReplyScope::next();
        let other_scope = ReplyScope::next();
        let ran = Arc::clone(&mine_ran);
        let _ = register(my_scope, move |_: &[u8]| {
            let _ = &mine;
            ran.store(true, Ordering::Relaxed);
        });
        let their_user_data = register(other_scope, move |_: &[u8]| {
            let _ = &theirs;
        });

        drain_pending_replies(my_scope);

        assert!(
            mine_dropped.load(Ordering::Relaxed),
            "a drained callback must be dropped, not leaked"
        );
        assert!(
            !mine_ran.load(Ordering::Relaxed),
            "a reply that never came must not be reported as one that did"
        );
        assert!(
            !theirs_dropped.load(Ordering::Relaxed),
            "another engine's callbacks must survive this engine's shutdown"
        );

        // And the other engine's reply still works afterwards.
        // SAFETY: a null pointer with size 0 is an empty reply, and this token
        // is still registered.
        unsafe { response_handle_callback(ptr::null(), 0, their_user_data) };
        assert!(theirs_dropped.load(Ordering::Relaxed));
    }

    /// Draining twice is what a `shutdown` followed by the engine's own `Drop`
    /// does.
    #[test]
    fn draining_an_empty_scope_is_a_no_op() {
        let scope = ReplyScope::next();
        drain_pending_replies(scope);
        drain_pending_replies(scope);
    }

    /// `PlatformMessageResponseHandle::new` reclaims the callback itself when
    /// `FlutterPlatformMessageCreateResponseHandle` refuses, which is this
    /// path.
    #[test]
    fn taking_a_pending_reply_hands_the_callback_back_exactly_once() {
        let (flag, dropped) = DropFlag::new();
        let id = register_pending_reply(
            ReplyScope::next(),
            Box::new(move |_: &[u8]| {
                let _ = &flag;
            }),
        );

        assert!(take_pending_reply(id).is_some(), "the entry was there");
        assert!(
            dropped.load(Ordering::Relaxed),
            "the callback is dropped when the caller drops what it took"
        );
        assert!(take_pending_reply(id).is_none(), "and only once");
    }

    /// A handle whose owning engine is gone, so that `Drop` skips the
    /// `FlutterPlatformMessageReleaseResponseHandle` call and never looks at
    /// `handle`. That makes the arming invariant testable without an engine.
    fn orphaned_handle(reply_id: ReplyId) -> PlatformMessageResponseHandle {
        PlatformMessageResponseHandle {
            // Non-null so `Drop` takes the owned-handle path, and never
            // dereferenced because the weak engine ref below cannot upgrade.
            handle: ptr::without_provenance(1),
            owner: Some(FlutterEngineWeakRef::default()),
            reply_id: Some(reply_id),
        }
    }

    /// The leak this all exists for: a handle dropped while still armed means
    /// the message never reached the engine, so nothing else will ever end the
    /// callback.
    #[test]
    fn dropping_an_armed_handle_reclaims_its_reply_callback() {
        let (flag, dropped) = DropFlag::new();
        let reply_id = register_pending_reply(
            ReplyScope::next(),
            Box::new(move |_: &[u8]| {
                let _ = &flag;
            }),
        );

        drop(orphaned_handle(reply_id));

        assert!(
            dropped.load(Ordering::Relaxed),
            "an unsent message's reply callback must be reclaimed"
        );
        assert!(take_pending_reply(reply_id).is_none(), "and only once");
    }

    /// The other half: once the engine has taken the message it owns the
    /// response object, so dropping the handle must *not* cancel the reply.
    #[test]
    fn dropping_a_disarmed_handle_leaves_the_reply_to_the_engine() {
        let (flag, dropped) = DropFlag::new();
        let reply_id = register_pending_reply(
            ReplyScope::next(),
            Box::new(move |_: &[u8]| {
                let _ = &flag;
            }),
        );

        let mut handle = orphaned_handle(reply_id);
        handle.disarm_reply();
        drop(handle);

        assert!(
            !dropped.load(Ordering::Relaxed),
            "a sent message's reply callback must survive the handle"
        );
        assert!(
            take_pending_reply(reply_id).is_some(),
            "it is still outstanding, for the engine or the next drain"
        );
    }

    /// The baton is a `*mut c_void` the engine stores and hands back
    /// unchanged, so the integer has to survive the round trip exactly.
    #[test]
    fn a_token_round_trips_through_the_engines_baton() {
        let id = ReplyId::next();
        assert_eq!(ReplyId::from_user_data(id.into_user_data()), id);
        assert_ne!(id, ReplyId::next(), "tokens must be unique");
    }
}
