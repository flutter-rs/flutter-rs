//! Rust embedder for the Flutter engine.
//!
//! Build an engine with [`builder::FlutterEngineBuilder`], then drive it from the
//! platform thread with [`FlutterEngine::run`] and
//! [`FlutterEngine::execute_platform_tasks`]. Most methods on [`FlutterEngine`]
//! must be called on the platform thread — the thread that built the engine —
//! and panic otherwise.
//!
//! The engine calls back into this crate on several of its own threads (raster,
//! IO, platform). Those callbacks live in `flutter_callbacks` and are
//! panic-guarded: a panic that reached the `extern "C"` boundary would abort the
//! process instead of unwinding.

// The unsafe side of this crate is only auditable if every block says why it is
// sound, and a published embedder wants its public API documented.
#![warn(
    clippy::undocumented_unsafe_blocks,
    clippy::missing_safety_doc,
    missing_debug_implementations,
    missing_docs
)]

#[macro_use]
mod macros;

pub mod builder;
pub mod channel;
pub mod codec;
pub mod error;
pub mod ffi;
mod flutter_callbacks;
pub mod plugins;
pub mod tasks;
pub mod view;

pub mod texture_registry;

use crate::builder::FlutterEngineBuilder;
use crate::channel::{Channel, ChannelRegistry};
use crate::ffi::{
    FlutterPointerDeviceKind, FlutterPointerPhase, FlutterPointerSignalKind, PanZoom,
    PointerButtons,
};
use crate::view::ViewFocusChangeHandler;
pub use crate::view::{
    FlutterCompositorHandler, FlutterViewId, IMPLICIT_VIEW_ID, ViewConstraints, ViewError,
    WindowMetrics,
};

use crate::channel::platform_message::{
    PlatformMessage, PlatformMessageResponseHandle, ReplyScope, drain_pending_replies,
};
use crate::tasks::TaskRunner;
use crate::texture_registry::{Texture, TextureRegistry};
use crossbeam_channel::{Receiver, Sender, unbounded};
use flutter_engine_sys::{FlutterEngineResult, FlutterTask};
use log::trace;
use parking_lot::RwLock;
use std::ffi::{CString, NulError};
use std::os::raw::{c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{self, AtomicBool, AtomicPtr};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};
use std::{mem, ptr};

/// Re-exports the `json_value!` macro needs to resolve at a foreign call site.
///
/// Not part of the public API; it exists only so the macro does not require the
/// calling crate to depend on `serde_json` and `log` itself.
#[doc(hidden)]
pub mod __private {
    pub use log;
    pub use serde_json;
}

pub(crate) type MainThreadEngineFn = Box<dyn FnOnce(&FlutterEngine) + Send>;
pub(crate) type MainThreadRenderThreadFn = Box<dyn FnOnce(&FlutterEngine) + Send>;

pub(crate) enum MainThreadCallback {
    Engine(MainThreadEngineFn),
    RenderThread(MainThreadRenderThreadFn),
}

struct FlutterEngineInner {
    opengl_handler: Box<dyn FlutterOpenGLHandler + Send + Sync>,
    compositor_handler: Option<Box<dyn FlutterCompositorHandler + Send + Sync>>,
    view_focus_change_handler: Option<ViewFocusChangeHandler>,
    /// The value exposed to Dart as `PlatformDispatcher.engineId`.
    engine_id: i64,
    /// The engine handle. Written once by `FlutterEngineInitialize` (through a
    /// `*mut` the C side owns for the duration of that call) and read from every
    /// thread afterwards, hence the atomic rather than a plain field.
    engine_ptr: AtomicPtr<flutter_engine_sys::_FlutterEngine>,
    /// Set by [`FlutterEngine::shutdown`]; makes shutdown idempotent and
    /// post-shutdown use observable instead of undefined.
    shut_down: AtomicBool,
    channel_registry: RwLock<ChannelRegistry>,
    /// This engine's partition of the process-wide table of outstanding
    /// platform message reply callbacks, which `shutdown` drains.
    reply_scope: ReplyScope,
    platform_runner: TaskRunner,
    platform_receiver: Receiver<MainThreadCallback>,
    platform_sender: Sender<MainThreadCallback>,
    texture_registry: TextureRegistry,
    /// The renderer configuration handed to `FlutterEngineInitialize`.
    ///
    /// The engine keeps reading it after initialisation (the platform view and
    /// its Metal surface are created when the shell starts in `run`), so it is
    /// heap-allocated and owned here. It is freed once the shell has been shut
    /// down; the leaked-engine path intentionally leaves it alone.
    renderer_config: AtomicPtr<flutter_engine_sys::FlutterRendererConfig>,
    assets: PathBuf,
    arguments: Vec<String>,
    /// `FlutterProjectArgs` holds raw pointers into these three fields. The
    /// engine copies them into its own `Settings` during
    /// `FlutterEngineInitialize`, but keeping them owned here means they are
    /// guaranteed to outlive the engine without leaking.
    argv: Vec<CString>,
    assets_path: CString,
    icu_data_path: Option<CString>,
}

impl Drop for FlutterEngineInner {
    fn drop(&mut self) {
        if !self.shut_down.load(atomic::Ordering::Acquire)
            && !self.engine_ptr.load(atomic::Ordering::Acquire).is_null()
        {
            log::warn!(
                "the last FlutterEngine handle was dropped without calling shutdown(); the engine and its threads are leaked"
            );
        }
        // `shutdown` already drained these, so this is a no-op on the normal
        // path. It is the backstop for the path above: without a shutdown
        // nothing else would ever reclaim an outstanding reply callback, since
        // the table outlives every engine.
        drain_pending_replies(self.reply_scope);
        if self.shut_down.load(atomic::Ordering::Acquire) {
            let config = self
                .renderer_config
                .swap(ptr::null_mut(), atomic::Ordering::AcqRel);
            if !config.is_null() {
                // SAFETY: the shell was shut down, so no engine thread can read
                // the configuration anymore.
                unsafe { drop(Box::from_raw(config)) };
            }
        }
    }
}

/// A weak handle to an engine, as handed to channels and plugins.
///
/// Upgrade it with [`FlutterEngineWeakRef::upgrade`] before use; it returns
/// `None` once every [`FlutterEngine`] handle has been dropped.
pub struct FlutterEngineWeakRef {
    inner: Weak<FlutterEngineInner>,
}

// SAFETY: `FlutterEngineInner` is only `!Send`/`!Sync` because of
// `channel_registry: RwLock<ChannelRegistry>`, whose `Arc<dyn Channel>` values
// carry no thread-safety bound. Every path that touches a channel — the
// platform message callback, `send_platform_message`, `with_channel` — runs on
// the platform thread (the `send_*` entry points assert it), so the channels are
// never actually shared across threads. A `Weak` hands out no access of its own;
// `upgrade` yields a `FlutterEngine`, which carries the same invariant below.
//
// Making `pub trait Channel: Send + Sync` would let all four of these impls be
// deleted, but it needs `RefCell` -> `Mutex` in both channel types and a `Send`
// bound on `MethodCallHandler`/`MessageHandler`, which is a breaking change for
// every plugin.
unsafe impl Send for FlutterEngineWeakRef {}

// SAFETY: see the `Send` impl above.
unsafe impl Sync for FlutterEngineWeakRef {}

impl FlutterEngineWeakRef {
    /// Upgrade to a strong handle, or `None` if the engine has been dropped.
    #[must_use]
    pub fn upgrade(&self) -> Option<FlutterEngine> {
        self.inner.upgrade().map(|arc| FlutterEngine { inner: arc })
    }

    /// `true` if the engine is still alive and has not been shut down.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.upgrade().is_some_and(|engine| !engine.is_shut_down())
    }

    /// `true` if both handles point at the same engine.
    #[must_use]
    pub fn ptr_equal(&self, other: &Self) -> bool {
        self.inner.ptr_eq(&other.inner)
    }
}

impl Default for FlutterEngineWeakRef {
    fn default() -> Self {
        Self { inner: Weak::new() }
    }
}

impl Clone for FlutterEngineWeakRef {
    fn clone(&self) -> Self {
        Self {
            inner: Weak::clone(&self.inner),
        }
    }
}

impl core::fmt::Debug for FlutterEngineWeakRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FlutterEngineWeakRef")
            .field("is_valid", &self.is_valid())
            .finish_non_exhaustive()
    }
}

/// A running Flutter engine.
///
/// Cheap to clone — every clone shares one engine. The engine is shut down by
/// [`FlutterEngine::shutdown`], not by dropping the handles.
#[derive(Clone)]
pub struct FlutterEngine {
    inner: Arc<FlutterEngineInner>,
}

// SAFETY: see the `Send` impl for `FlutterEngineWeakRef` above: the only
// `!Send`/`!Sync` part of `FlutterEngineInner` is the channel registry, which is
// only ever touched on the platform thread. Everything the engine's other
// threads reach through this handle — `engine_ptr`, `opengl_handler`,
// `platform_runner`, `texture_registry` — is `Send + Sync` in its own right.
unsafe impl Send for FlutterEngine {}

// SAFETY: see the `Send` impl above.
unsafe impl Sync for FlutterEngine {}

impl core::fmt::Debug for FlutterEngine {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FlutterEngine")
            .field("engine_ptr", &self.engine_ptr())
            .field("shut_down", &self.is_shut_down())
            .finish_non_exhaustive()
    }
}

/// The OpenGL context the engine renders through.
///
/// The engine calls these from its raster and IO threads, so an implementation
/// must be `Send + Sync` and must not assume a single thread. A panic inside any
/// of them is caught and logged, and the call reports failure to the engine.
pub trait FlutterOpenGLHandler {
    /// Present the current back buffer.
    fn swap_buffers(&self) -> bool;

    /// Make the render context current on the calling thread.
    fn make_current(&self) -> bool;

    /// Clear the current context on the calling thread.
    fn clear_current(&self) -> bool;

    /// The framebuffer object the engine should render into.
    fn fbo_callback(&self) -> u32;

    /// Make the resource context current on the calling thread.
    fn make_resource_current(&self) -> bool;

    /// Resolve an OpenGL procedure by name.
    ///
    /// `proc` is a NUL-terminated C string owned by the engine; the returned
    /// pointer must be null when the symbol is unknown.
    fn gl_proc_resolver(&self, proc: *const c_char) -> *mut c_void;
}

/// Log an embedder call that did not return `kSuccess`.
///
/// `FlutterEngineResult` is the embedder API's only error channel, so dropping
/// one means silently ignoring an engine failure.
pub(crate) fn log_engine_result(result: FlutterEngineResult, what: &str) {
    if result != FlutterEngineResult::kSuccess {
        log::error!("{what} failed: {result:?}");
    }
}

/// Assign a unique non-zero `engine_id` to engines that did not choose one.
fn next_engine_id() -> i64 {
    static NEXT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);
    NEXT.fetch_add(1, atomic::Ordering::Relaxed)
}

/// Completion state shared with an `AddView`/`RemoveView` callback.
///
/// The callback may run inline (when the UI and platform runners are merged) or
/// later on an engine-managed thread, so the state is reference-counted: a
/// timeout never frees it while the engine can still invoke the callback.
struct ViewCompletion {
    done: Mutex<Option<bool>>,
    cv: Condvar,
}

impl ViewCompletion {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            done: Mutex::new(None),
            cv: Condvar::new(),
        })
    }

    fn complete(&self, value: bool) {
        let mut done = match self.done.lock() {
            Ok(done) => done,
            Err(poisoned) => poisoned.into_inner(),
        };
        if done.is_none() {
            *done = Some(value);
            self.cv.notify_all();
        }
    }

    fn wait(&self, timeout: Duration) -> Option<bool> {
        let done = match self.done.lock() {
            Ok(done) => done,
            Err(poisoned) => poisoned.into_inner(),
        };
        let (done, _) = match self
            .cv
            .wait_timeout_while(done, timeout, |done| done.is_none())
        {
            Ok(result) => result,
            Err(poisoned) => poisoned.into_inner(),
        };
        *done
    }
}

/// Completion callback for `FlutterEngineAddView`.
///
/// # Safety
///
/// `result` must be the pointer the engine received from `FlutterEngineAddView`;
/// its `user_data` is the leaked `Arc<ViewCompletion>` created by
/// [`FlutterEngine::add_view`].
unsafe extern "C" fn add_view_callback(result: *const flutter_engine_sys::FlutterAddViewResult) {
    if result.is_null() {
        return;
    }
    // SAFETY: the caller guarantees the pointer and its `user_data`; the result
    // is only valid for this callback, so copy the fields out immediately.
    let result = unsafe { &*result };
    if result.user_data.is_null() {
        return;
    }
    // SAFETY: `user_data` came from `Arc::into_raw(completion.clone())` in
    // `add_view`; this consumes the callback's strong reference.
    let completion = unsafe { Arc::from_raw(result.user_data as *const ViewCompletion) };
    completion.complete(result.added);
}

/// Completion callback for `FlutterEngineRemoveView`.
///
/// # Safety
///
/// `result` must be the pointer the engine received from
/// `FlutterEngineRemoveView`; its `user_data` is the leaked
/// `Arc<ViewCompletion>` created by [`FlutterEngine::remove_view`].
unsafe extern "C" fn remove_view_callback(
    result: *const flutter_engine_sys::FlutterRemoveViewResult,
) {
    if result.is_null() {
        return;
    }
    // SAFETY: see `add_view_callback`.
    let result = unsafe { &*result };
    if result.user_data.is_null() {
        return;
    }
    // SAFETY: see `add_view_callback`.
    let completion = unsafe { Arc::from_raw(result.user_data as *const ViewCompletion) };
    completion.complete(result.removed);
}

impl FlutterEngine {
    pub(crate) fn new(builder: FlutterEngineBuilder) -> Result<Self, CreateError> {
        // Convert arguments into flutter compatible
        let mut argv = Vec::with_capacity(builder.args.len() + 2);
        argv.push(CString::new("flutter-rs").expect("literal contains no nul byte"));
        // Legacy custom engine builds linked the ICU data into the engine
        // library under the `gIcudtl` symbol prefix. Official builds ship
        // `icudtl.dat` separately, in which case `icu_data_path` is set below
        // and this argument must be omitted.
        if builder.icu_data.is_none() {
            argv.push(
                CString::new("--icu-symbol-prefix=gIcudtl").expect("literal contains no nul byte"),
            );
        }
        for arg in builder.args.iter() {
            argv.push(CString::new(arg.as_str())?);
        }

        let (main_tx, main_rx) = unbounded();
        let ui_on_platform_thread = builder.ui_on_platform_thread;

        // `FlutterEngineInner` is `!Send + !Sync` only because of its channel
        // registry; the `unsafe impl`s above carry that invariant (channels are
        // only ever touched on the platform thread) and this `Arc` is exactly
        // what they are written for.
        #[allow(clippy::arc_with_non_send_sync)]
        let engine = Self {
            inner: Arc::new(FlutterEngineInner {
                opengl_handler: builder.opengl_handler.ok_or(CreateError::NoOpenGlHandler)?,
                compositor_handler: builder.compositor_handler,
                view_focus_change_handler: builder.view_focus_change_handler,
                engine_id: builder.engine_id.unwrap_or_else(next_engine_id),
                engine_ptr: AtomicPtr::new(ptr::null_mut()),
                shut_down: AtomicBool::new(false),
                channel_registry: RwLock::new(ChannelRegistry::new()),
                reply_scope: ReplyScope::next(),
                platform_runner: TaskRunner::new(
                    builder
                        .platform_handler
                        .ok_or(CreateError::NoPlatformHandler)?,
                ),
                platform_receiver: main_rx,
                platform_sender: main_tx,
                texture_registry: TextureRegistry::new(),
                renderer_config: AtomicPtr::new(ptr::null_mut()),
                assets_path: path_to_cstring(&builder.assets)?,
                icu_data_path: builder
                    .icu_data
                    .as_deref()
                    .map(path_to_cstring)
                    .transpose()?,
                assets: builder.assets,
                arguments: builder.args,
                argv,
            }),
        };

        let inner = &engine.inner;
        inner.channel_registry.write().init(engine.downgrade());
        inner.platform_runner.init(engine.downgrade());

        // Configure renderer. Written as a full struct literal rather than
        // `mem::zeroed()` so that a field added by a future engine header is a
        // compile error here instead of a silently zeroed value.
        let open_gl = flutter_engine_sys::FlutterOpenGLRendererConfig {
            struct_size: mem::size_of::<flutter_engine_sys::FlutterOpenGLRendererConfig>(),
            make_current: Some(flutter_callbacks::make_current),
            clear_current: Some(flutter_callbacks::clear_current),
            present: Some(flutter_callbacks::present),
            fbo_callback: Some(flutter_callbacks::fbo_callback),
            make_resource_current: Some(flutter_callbacks::make_resource_current),
            fbo_reset_after_present: false,
            surface_transformation: None,
            gl_proc_resolver: Some(flutter_callbacks::gl_proc_resolver),
            gl_external_texture_frame_callback: Some(flutter_callbacks::gl_external_texture_frame),
            fbo_with_frame_info_callback: None,
            present_with_info: None,
            populate_existing_damage: None,
        };

        let renderer_config = match builder.renderer {
            crate::builder::RendererKind::Software => {
                // The engine rasterises on the CPU and hands each view's pixels
                // to the compositor as a software backing store. The surface
                // callback is only used without a compositor, but the engine
                // requires it to be present.
                flutter_engine_sys::FlutterRendererConfig {
                    type_: flutter_engine_sys::FlutterRendererType::kSoftware,
                    __bindgen_anon_1: flutter_engine_sys::FlutterRendererConfig__bindgen_ty_1 {
                        software: flutter_engine_sys::FlutterSoftwareRendererConfig {
                            struct_size: mem::size_of::<
                                flutter_engine_sys::FlutterSoftwareRendererConfig,
                            >(),
                            surface_present_callback: Some(
                                flutter_callbacks::software_surface_present,
                            ),
                        },
                    },
                }
            }
            crate::builder::RendererKind::Metal {
                device,
                command_queue,
            } => {
                // With a compositor the engine renders into the Metal textures
                // the compositor creates, so the drawable callbacks are unused;
                // only the device and the queue are needed.
                flutter_engine_sys::FlutterRendererConfig {
                    type_: flutter_engine_sys::FlutterRendererType::kMetal,
                    __bindgen_anon_1: flutter_engine_sys::FlutterRendererConfig__bindgen_ty_1 {
                        metal: flutter_engine_sys::FlutterMetalRendererConfig {
                            struct_size: mem::size_of::<
                                flutter_engine_sys::FlutterMetalRendererConfig,
                            >(),
                            device: device as *const c_void,
                            present_command_queue: command_queue as *const c_void,
                            // The engine validates these even with a
                            // compositor, but never calls them.
                            get_next_drawable_callback: Some(
                                flutter_callbacks::metal_get_next_drawable,
                            ),
                            present_drawable_callback: Some(
                                flutter_callbacks::metal_present_drawable,
                            ),
                            external_texture_frame_callback: None,
                        },
                    },
                }
            }
            crate::builder::RendererKind::OpenGl => flutter_engine_sys::FlutterRendererConfig {
                type_: flutter_engine_sys::FlutterRendererType::kOpenGL,
                __bindgen_anon_1: flutter_engine_sys::FlutterRendererConfig__bindgen_ty_1 {
                    open_gl,
                },
            },
        };

        // The engine also reads this configuration when it creates the shell
        // in `run`, after this function has returned (the Metal platform view
        // surface is constructed there), so it lives in `FlutterEngineInner`
        // rather than on this stack frame.
        let renderer_config = Box::into_raw(Box::new(renderer_config));
        inner
            .renderer_config
            .store(renderer_config, atomic::Ordering::Release);
        // SAFETY: `renderer_config` points into the box owned by `inner`, which
        // outlives the engine handle.
        let renderer_config: &'static flutter_engine_sys::FlutterRendererConfig =
            unsafe { &*renderer_config };

        // Configure engine threads. The task runner gets a *weak* reference as
        // its user data: a strong one would keep the engine alive forever, and
        // the callbacks are written to cope with a failed upgrade.
        let runner_ptr =
            Weak::into_raw(Arc::downgrade(&inner.platform_runner.inner)) as *mut c_void;

        let platform_task_runner = flutter_engine_sys::FlutterTaskRunnerDescription {
            struct_size: mem::size_of::<flutter_engine_sys::FlutterTaskRunnerDescription>(),
            user_data: runner_ptr,
            runs_task_on_current_thread_callback: Some(
                flutter_callbacks::runs_task_on_current_thread,
            ),
            post_task_callback: Some(flutter_callbacks::post_task),
            identifier: 0,
            destruction_callback: None,
        };

        let custom_task_runners = flutter_engine_sys::FlutterCustomTaskRunners {
            struct_size: mem::size_of::<flutter_engine_sys::FlutterCustomTaskRunners>(),
            platform_task_runner: &platform_task_runner,
            render_task_runner: ptr::null(),
            thread_priority_setter: None,
            // Merging the UI runner into the platform runner is what makes
            // synchronous view creation from Dart work: `FlutterEngineAddView`
            // posts to the UI runner and must complete before the embedder
            // returns the view id to Dart.
            ui_task_runner: if ui_on_platform_thread {
                &platform_task_runner
            } else {
                ptr::null()
            },
        };

        // The engine's user data. Every engine callback upgrades this weak
        // reference, so it has to outlive the engine and is deliberately
        // leaked. `runner_ptr` above is the same pattern for the task runner.
        let inner_ptr = Weak::into_raw(Arc::downgrade(inner)) as *mut c_void;

        // The compositor is copied by the engine during initialisation, but its
        // `user_data` (the engine weak reference) must outlive the engine. The
        // callbacks are only installed when a handler was supplied: providing a
        // compositor switches the engine away from the legacy single-view
        // rendering path.
        let compositor =
            inner
                .compositor_handler
                .as_ref()
                .map(|_| flutter_engine_sys::FlutterCompositor {
                    struct_size: mem::size_of::<flutter_engine_sys::FlutterCompositor>(),
                    user_data: inner_ptr,
                    create_backing_store_callback: Some(
                        flutter_callbacks::compositor_create_backing_store,
                    ),
                    collect_backing_store_callback: Some(
                        flutter_callbacks::compositor_collect_backing_store,
                    ),
                    present_layers_callback: None,
                    avoid_backing_store_cache: false,
                    present_view_callback: Some(flutter_callbacks::compositor_present_view),
                });

        // Configure engine.
        // SAFETY: every field of `FlutterProjectArgs` is an integer, bool, raw
        // pointer or `Option<fn>`, for all of which all-zero is a valid value —
        // checked field by field against the vendored `flutter-engine.h`. A
        // literal would be 40 fields wide, so this one keeps `mem::zeroed()`.
        // Re-verify after every Flutter SDK bump: bindgen emits real Rust enums
        // (`EnumVariation::Rust`), so an enum-by-value field added upstream
        // would make this undefined behaviour.
        let mut project_args: flutter_engine_sys::FlutterProjectArgs = unsafe { mem::zeroed() };
        let argv: Vec<*const c_char> = inner.argv.iter().map(|arg| arg.as_ptr()).collect();
        project_args.struct_size = mem::size_of::<flutter_engine_sys::FlutterProjectArgs>();
        project_args.assets_path = inner.assets_path.as_ptr();
        project_args.command_line_argc = argv.len() as i32;
        project_args.command_line_argv = argv.as_ptr();
        project_args.platform_message_callback = Some(flutter_callbacks::platform_message_callback);
        project_args.root_isolate_create_callback =
            Some(flutter_callbacks::root_isolate_create_callback);
        project_args.custom_task_runners = &custom_task_runners;
        project_args.compositor = compositor
            .as_ref()
            .map_or(ptr::null(), |compositor| compositor as *const _);
        project_args.engine_id = inner.engine_id;
        project_args.view_focus_change_request_callback =
            inner.view_focus_change_handler.as_ref().map(|_| {
                flutter_callbacks::view_focus_change_request
                    as unsafe extern "C" fn(
                        *const flutter_engine_sys::FlutterViewFocusChangeRequest,
                        *mut c_void,
                    )
            });
        project_args.shutdown_dart_vm_when_done = true;
        if let Some(icu) = inner.icu_data_path.as_ref() {
            project_args.icu_data_path = icu.as_ptr();
        }

        // Initialise engine.
        let mut engine_ptr: flutter_engine_sys::FlutterEngine = ptr::null_mut();

        // SAFETY: `renderer_config`, `project_args` and everything they point at
        // (the two task-runner descriptions, `inner.argv`, `inner.assets_path`,
        // `inner.icu_data_path`) are alive for the whole call, and `engine_ptr`
        // is a live, writable `FlutterEngine` out-parameter. `inner_ptr` is a
        // leaked `Weak`, so it stays dereferenceable for as long as the engine
        // can call back into us.
        let result = unsafe {
            flutter_engine_sys::FlutterEngineInitialize(
                1,
                renderer_config,
                &project_args,
                inner_ptr,
                &mut engine_ptr,
            )
        };

        // On the error paths `inner_ptr`/`runner_ptr` stay leaked: only the weak
        // counts of an allocation that is about to drop its value, and the
        // engine may already have handed them to its own threads.
        if result != FlutterEngineResult::kSuccess {
            return Err(CreateError::EngineInitialize(result as i32));
        }
        if engine_ptr.is_null() {
            return Err(CreateError::EnginePtrNull);
        }

        inner
            .engine_ptr
            .store(engine_ptr, atomic::Ordering::Release);

        Ok(engine)
    }

    /// The raw engine handle, or null before initialisation completed.
    #[inline]
    #[must_use]
    pub fn engine_ptr(&self) -> flutter_engine_sys::FlutterEngine {
        self.inner.engine_ptr.load(atomic::Ordering::Acquire)
    }

    /// `true` once [`shutdown`](Self::shutdown) has been called.
    ///
    /// Every other engine call is a no-op after that point.
    #[inline]
    #[must_use]
    pub fn is_shut_down(&self) -> bool {
        self.inner.shut_down.load(atomic::Ordering::Acquire)
    }

    /// This engine's partition of the process-wide table of outstanding
    /// platform message reply callbacks.
    pub(crate) fn reply_scope(&self) -> ReplyScope {
        self.inner.reply_scope
    }

    /// `true` when it is safe to hand `what` to the embedder API; logs and
    /// returns `false` once the engine has been shut down.
    fn check_live(&self, what: &str) -> bool {
        if self.is_shut_down() {
            log::error!("{what} called after FlutterEngine::shutdown; ignoring");
            return false;
        }
        true
    }

    /// Register a channel, replacing any channel of the same name.
    pub fn register_channel<C>(&self, channel: C) -> Weak<C>
    where
        C: Channel + 'static,
    {
        self.inner
            .channel_registry
            .write()
            .register_channel(channel)
    }

    /// Remove a channel by name, returning it if it was registered.
    pub fn remove_channel(&self, channel_name: &str) -> Option<Arc<dyn Channel>> {
        self.inner
            .channel_registry
            .write()
            .remove_channel(channel_name)
    }

    /// Run `f` with the channel registered under `channel_name`, if any.
    pub fn with_channel<F>(&self, channel_name: &str, f: F)
    where
        F: FnOnce(&dyn Channel),
    {
        // The channel's `Arc` is cloned out and the registry's read guard
        // dropped before `f` runs: `f` is user code that may register or remove
        // channels, and the `RwLock` is not reentrant.
        let channel = self.inner.channel_registry.read().channel(channel_name);
        if let Some(channel) = channel {
            f(&*channel);
        }
    }

    /// A weak handle to this engine.
    #[must_use]
    pub fn downgrade(&self) -> FlutterEngineWeakRef {
        FlutterEngineWeakRef {
            inner: Arc::downgrade(&self.inner),
        }
    }

    /// The asset bundle directory the engine was started with.
    #[must_use]
    pub fn assets(&self) -> &Path {
        &self.inner.assets
    }

    /// The extra engine arguments the engine was started with.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.inner.arguments
    }

    /// Start the initialised engine.
    ///
    /// # Errors
    ///
    /// Returns [`RunError::NotPlatformThread`] when called from another thread,
    /// or [`RunError::Engine`] carrying the embedder result code when the engine
    /// refuses to start.
    pub fn run(&self) -> Result<(), RunError> {
        if !self.is_platform_thread() {
            return Err(RunError::NotPlatformThread);
        }
        if self.is_shut_down() {
            return Err(RunError::ShutDown);
        }

        // SAFETY: the engine handle is non-null (checked in `new`) and has not
        // been shut down, and `FlutterEngineRunInitialized` must be called on
        // the platform thread, which we just asserted.
        let result = unsafe { flutter_engine_sys::FlutterEngineRunInitialized(self.engine_ptr()) };
        match result {
            FlutterEngineResult::kSuccess => Ok(()),
            code => Err(RunError::Engine(code as i32)),
        }
    }

    pub(crate) fn post_platform_callback(&self, callback: MainThreadCallback) {
        if let Err(err) = self.inner.platform_sender.send(callback) {
            log::error!("failed to queue a platform thread callback: {err}");
            return;
        }
        self.inner.platform_runner.wake();
    }

    /// `true` when the calling thread is the engine's platform thread.
    #[inline]
    #[must_use]
    pub fn is_platform_thread(&self) -> bool {
        self.inner.platform_runner.runs_task_on_current_thread()
    }

    /// Run `f` on the platform thread, immediately if already there.
    pub fn run_on_platform_thread<F>(&self, f: F)
    where
        F: FnOnce(&FlutterEngine) + 'static + Send,
    {
        if self.is_platform_thread() {
            f(self);
        } else {
            self.post_platform_callback(MainThreadCallback::Engine(Box::new(f)));
        }
    }

    /// Queue `f` to run on the engine's render thread.
    pub fn run_on_render_thread<F>(&self, f: F)
    where
        F: FnOnce(&FlutterEngine) + 'static + Send,
    {
        // Queued to the platform thread, which hands it to the engine as a
        // render-thread task from `execute_platform_tasks`. It cannot be run
        // inline even on the platform thread: `f` expects the render thread's
        // GL context to be current.
        self.post_platform_callback(MainThreadCallback::RenderThread(Box::new(f)));
    }

    /// Add a view to a running engine.
    ///
    /// The engine completes the operation asynchronously; this method waits for
    /// its completion callback. The wait is safe when the UI and platform
    /// runners are merged (the callback runs inline) and when they are not (the
    /// callback runs on an engine thread). A timeout does not free the
    /// completion state while the engine can still call back.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError::NotPlatformThread`] off the platform thread,
    /// [`ViewError::ImplicitView`] for id `0`, [`ViewError::InvalidMetrics`] for
    /// degenerate metrics, [`ViewError::Engine`] when the embedder rejects the
    /// request, [`ViewError::NotAdded`] when the engine reports failure, and
    /// [`ViewError::Timeout`] if the callback never arrives.
    pub fn add_view(
        &self,
        view_id: FlutterViewId,
        metrics: WindowMetrics,
    ) -> Result<(), ViewError> {
        if !self.is_platform_thread() {
            return Err(ViewError::NotPlatformThread);
        }
        if view_id == IMPLICIT_VIEW_ID {
            return Err(ViewError::ImplicitView);
        }
        if !self.check_live("add_view") {
            return Err(ViewError::ShutDown);
        }
        let metrics = metrics.to_event(view_id).ok_or(ViewError::InvalidMetrics)?;

        let completion = ViewCompletion::new();
        let user_data = Arc::into_raw(Arc::clone(&completion)) as *mut c_void;
        let info = flutter_engine_sys::FlutterAddViewInfo {
            struct_size: mem::size_of::<flutter_engine_sys::FlutterAddViewInfo>(),
            view_id,
            view_metrics: &metrics,
            user_data,
            add_view_callback: Some(add_view_callback),
        };
        // SAFETY: the engine handle is live; `metrics` and the task-runner
        // descriptions outlive the call, and `user_data` is a leaked strong
        // reference kept alive by the callback (or reclaimed below on an
        // immediate failure).
        let result = unsafe { flutter_engine_sys::FlutterEngineAddView(self.engine_ptr(), &info) };
        if result != FlutterEngineResult::kSuccess {
            // SAFETY: `user_data` was produced by `Arc::into_raw` above and the
            // engine will not invoke the callback after an immediate failure.
            drop(unsafe { Arc::from_raw(user_data as *const ViewCompletion) });
            log_engine_result(result, "FlutterEngineAddView");
            return Err(ViewError::Engine(result as i32));
        }
        match completion.wait(Duration::from_secs(5)) {
            Some(true) => Ok(()),
            Some(false) => Err(ViewError::NotAdded),
            None => {
                log::error!("FlutterEngineAddView({view_id}) did not complete within 5s");
                Err(ViewError::Timeout)
            }
        }
    }

    /// Remove a view from a running engine.
    ///
    /// The engine completes the operation asynchronously. Its callback is posted
    /// to the raster thread after the view's layers are collected, so this can
    /// take a frame. The implicit view (`0`) cannot be removed.
    ///
    /// # Errors
    ///
    /// See [`add_view`](Self::add_view); removal reports
    /// [`ViewError::NotRemoved`] instead of `NotAdded`.
    pub fn remove_view(&self, view_id: FlutterViewId) -> Result<(), ViewError> {
        if !self.is_platform_thread() {
            return Err(ViewError::NotPlatformThread);
        }
        if view_id == IMPLICIT_VIEW_ID {
            return Err(ViewError::ImplicitView);
        }
        if !self.check_live("remove_view") {
            return Err(ViewError::ShutDown);
        }

        let completion = ViewCompletion::new();
        let user_data = Arc::into_raw(Arc::clone(&completion)) as *mut c_void;
        let info = flutter_engine_sys::FlutterRemoveViewInfo {
            struct_size: mem::size_of::<flutter_engine_sys::FlutterRemoveViewInfo>(),
            view_id,
            user_data,
            remove_view_callback: Some(remove_view_callback),
        };
        // SAFETY: as in `add_view`; `info` is only borrowed for this call.
        let result =
            unsafe { flutter_engine_sys::FlutterEngineRemoveView(self.engine_ptr(), &info) };
        if result != FlutterEngineResult::kSuccess {
            // SAFETY: see `add_view`.
            drop(unsafe { Arc::from_raw(user_data as *const ViewCompletion) });
            log_engine_result(result, "FlutterEngineRemoveView");
            return Err(ViewError::Engine(result as i32));
        }
        match completion.wait(Duration::from_secs(5)) {
            Some(true) => Ok(()),
            Some(false) => Err(ViewError::NotRemoved),
            None => {
                log::error!("FlutterEngineRemoveView({view_id}) did not complete within 5s");
                Err(ViewError::Timeout)
            }
        }
    }

    /// Tell the engine which view has native focus.
    ///
    /// The engine routes keyboard events to the focused view, so an embedder
    /// must send this when a window gains or loses focus.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    pub fn send_view_focus_event(&self, view_id: FlutterViewId, focused: bool) {
        assert!(
            self.is_platform_thread(),
            "send_view_focus_event must be called on the platform thread"
        );
        if !self.check_live("send_view_focus_event") {
            return;
        }

        let event = flutter_engine_sys::FlutterViewFocusEvent {
            struct_size: mem::size_of::<flutter_engine_sys::FlutterViewFocusEvent>(),
            view_id,
            state: if focused {
                flutter_engine_sys::FlutterViewFocusState::kFocused
            } else {
                flutter_engine_sys::FlutterViewFocusState::kUnfocused
            },
            direction: flutter_engine_sys::FlutterViewFocusDirection::kUndefined,
        };
        // SAFETY: the engine handle is live and `event` outlives the platform-
        // thread-only call.
        let result = unsafe {
            flutter_engine_sys::FlutterEngineSendViewFocusEvent(self.engine_ptr(), &event)
        };
        log_engine_result(result, "FlutterEngineSendViewFocusEvent");
    }

    /// Tell the engine the window size or device pixel ratio of a view changed.
    ///
    /// Returns `false` when the metrics are degenerate (0×0, non-finite ratio)
    /// or outside their constraints. A 0×0 framebuffer is what GLFW reports for
    /// an iconified window; callers should skip the update instead of sending
    /// it.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    pub fn send_window_metrics_for_view(
        &self,
        view_id: FlutterViewId,
        metrics: WindowMetrics,
    ) -> bool {
        assert!(
            self.is_platform_thread(),
            "send_window_metrics_for_view must be called on the platform thread"
        );
        if !self.check_live("send_window_metrics_for_view") {
            return false;
        }
        let Some(event) = metrics.to_event(view_id) else {
            log::error!("ignoring invalid window metrics for view {view_id}: {metrics:?}");
            return false;
        };
        // SAFETY: the engine handle is live and `event` outlives the call, which
        // the embedder documents as platform-thread only.
        let result = unsafe {
            flutter_engine_sys::FlutterEngineSendWindowMetricsEvent(self.engine_ptr(), &event)
        };
        log_engine_result(result, "FlutterEngineSendWindowMetricsEvent");
        result == FlutterEngineResult::kSuccess
    }

    /// Tell the engine the window size or device pixel ratio changed.
    ///
    /// This is the implicit-view (`0`) compatibility form; use
    /// [`send_window_metrics_for_view`](Self::send_window_metrics_for_view) for
    /// any other view.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    pub fn send_window_metrics_event(&self, width: usize, height: usize, pixel_ratio: f64) {
        self.send_window_metrics_for_view(
            IMPLICIT_VIEW_ID,
            WindowMetrics::tight(width, height, pixel_ratio),
        );
    }

    /// Send a pointer event to the engine.
    ///
    /// `buttons` is the *cumulative* mask of buttons held down at the moment of
    /// the event, not the button that changed: reporting
    /// [`FlutterPointerDeviceKind::Mouse`] obliges the embedder to send
    /// [`PointerButtons::NONE`] while the pointer only hovers, and to clear a
    /// button's flag before sending its [`FlutterPointerPhase::Up`].
    ///
    /// `pan_zoom` belongs to the `PanZoom*` phases and to
    /// [`FlutterPointerSignalKind::Scale`]; pass `None` for an ordinary
    /// pointer event, which sends the neutral [`PanZoom::default`].
    ///
    /// This is the implicit-view (`0`) compatibility form.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    #[allow(clippy::too_many_arguments)]
    pub fn send_pointer_event(
        &self,
        device: i32,
        phase: FlutterPointerPhase,
        position: (f64, f64),
        signal_kind: FlutterPointerSignalKind,
        scroll_delta: (f64, f64),
        device_kind: FlutterPointerDeviceKind,
        buttons: impl Into<PointerButtons>,
        pan_zoom: Option<PanZoom>,
    ) {
        self.send_pointer_event_for_view(
            IMPLICIT_VIEW_ID,
            device,
            phase,
            position,
            signal_kind,
            scroll_delta,
            device_kind,
            buttons,
            pan_zoom,
        );
    }

    /// Send a pointer event to a specific view.
    ///
    /// See [`send_pointer_event`](Self::send_pointer_event) for the semantics of
    /// the arguments; this form is what a multi-window embedder uses so the
    /// event is routed to the window that produced it.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    #[allow(clippy::too_many_arguments)]
    pub fn send_pointer_event_for_view(
        &self,
        view_id: FlutterViewId,
        device: i32,
        phase: FlutterPointerPhase,
        (x, y): (f64, f64),
        signal_kind: FlutterPointerSignalKind,
        (scroll_delta_x, scroll_delta_y): (f64, f64),
        device_kind: FlutterPointerDeviceKind,
        buttons: impl Into<PointerButtons>,
        pan_zoom: Option<PanZoom>,
    ) {
        assert!(
            self.is_platform_thread(),
            "send_pointer_event_for_view must be called on the platform thread"
        );
        if !self.check_live("send_pointer_event_for_view") {
            return;
        }

        // `FlutterPointerEvent::timestamp` is microseconds on the
        // `FlutterEngineGetCurrentTime` (monotonic) clock, not on the wall
        // clock — the engine compares it against its own frame times.
        // SAFETY: `FlutterEngineGetCurrentTime` takes no arguments, touches no
        // engine state and is callable from any thread.
        let timestamp =
            (unsafe { flutter_engine_sys::FlutterEngineGetCurrentTime() } / 1_000) as usize;
        let buttons: PointerButtons = buttons.into();
        let pan_zoom = pan_zoom.unwrap_or_default();
        // `FlutterPointerEvent` embeds bindgen `enum`s whose first discriminant
        // is not 0, so `mem::zeroed()` would be undefined behaviour. Build it as
        // a full literal instead.
        let event = flutter_engine_sys::FlutterPointerEvent {
            struct_size: mem::size_of::<flutter_engine_sys::FlutterPointerEvent>(),
            phase: phase.into(),
            timestamp,
            x,
            y,
            device,
            signal_kind: signal_kind.into(),
            scroll_delta_x,
            scroll_delta_y,
            device_kind: device_kind.into(),
            buttons: buttons.bits(),
            pan_x: pan_zoom.pan.0,
            pan_y: pan_zoom.pan.1,
            scale: pan_zoom.scale,
            rotation: pan_zoom.rotation,
            view_id,
            pressure: 0.0,
            pressure_min: 0.0,
            pressure_max: 0.0,
        };
        // SAFETY: the engine handle is live and `event` outlives the call, which
        // the embedder documents as platform-thread only.
        let result = unsafe {
            flutter_engine_sys::FlutterEngineSendPointerEvent(self.engine_ptr(), &event, 1)
        };
        log_engine_result(result, "FlutterEngineSendPointerEvent");
    }

    /// Send a hardware key event to the engine, feeding `HardwareKeyboard`
    /// and the modern key event system. `physical` is a USB HID usage code and
    /// `logical` a Flutter logical key id (both non-zero); `character` is the
    /// produced text, if any.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    pub fn send_key_event(
        &self,
        kind: crate::ffi::KeyEventType,
        physical: u64,
        logical: u64,
        character: Option<&str>,
    ) {
        assert!(
            self.is_platform_thread(),
            "send_key_event must be called on the platform thread"
        );
        if !self.check_live("send_key_event") {
            return;
        }

        // Keep the CString alive until FlutterEngineSendKeyEvent returns.
        let character = character.and_then(|s| CString::new(s).ok());
        // SAFETY: `FlutterEngineGetCurrentTime` takes no arguments, touches no
        // engine state and is callable from any thread.
        let timestamp =
            unsafe { flutter_engine_sys::FlutterEngineGetCurrentTime() } as f64 / 1000.0;

        // FlutterKeyEvent embeds enums whose first discriminant is non-zero, so
        // it cannot be zero-initialised; build it as a full literal.
        let event = flutter_engine_sys::FlutterKeyEvent {
            struct_size: mem::size_of::<flutter_engine_sys::FlutterKeyEvent>(),
            timestamp,
            type_: kind.into(),
            physical,
            logical,
            character: character.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
            synthesized: false,
            device_type:
                flutter_engine_sys::FlutterKeyEventDeviceType::kFlutterKeyEventDeviceTypeKeyboard,
        };
        // SAFETY: the engine handle is live, and `event` and the `CString` its
        // `character` field points at both outlive the call. A null response
        // callback with null user data is how the embedder API spells
        // "fire and forget".
        let result = unsafe {
            flutter_engine_sys::FlutterEngineSendKeyEvent(
                self.engine_ptr(),
                &event,
                None,
                ptr::null_mut(),
            )
        };
        log_engine_result(result, "FlutterEngineSendKeyEvent");
    }

    /// Push the platform's preferred locales to the framework, most preferred
    /// first.
    ///
    /// Each entry is `(language_code, country_code, script_code,
    /// variant_code)`; empty strings are reported to the framework as absent.
    /// This is the only API that feeds `PlatformDispatcher.locales`, so an
    /// embedder must call it (and call it again when the system locale changes)
    /// or every app resolves to `supportedLocales.first`.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    pub fn update_locales(&self, locales: &[(String, String, String, String)]) {
        assert!(
            self.is_platform_thread(),
            "update_locales must be called on the platform thread"
        );
        if !self.check_live("update_locales") {
            return;
        }

        // Keep every CString alive until FlutterEngineUpdateLocales returns.
        fn to_cstring(value: &str) -> Option<CString> {
            if value.is_empty() {
                return None;
            }
            match CString::new(value) {
                Ok(value) => Some(value),
                Err(_) => {
                    log::error!("locale code contains an interior nul byte, ignoring it");
                    None
                }
            }
        }

        let owned: Vec<_> = locales
            .iter()
            .map(|(language, country, script, variant)| {
                (
                    to_cstring(language),
                    to_cstring(country),
                    to_cstring(script),
                    to_cstring(variant),
                )
            })
            .collect();
        let structs: Vec<flutter_engine_sys::FlutterLocale> = owned
            .iter()
            .map(
                |(language, country, script, variant)| flutter_engine_sys::FlutterLocale {
                    struct_size: mem::size_of::<flutter_engine_sys::FlutterLocale>(),
                    language_code: language.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                    country_code: country.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                    script_code: script.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                    variant_code: variant.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                },
            )
            .collect();
        let mut pointers: Vec<*const flutter_engine_sys::FlutterLocale> =
            structs.iter().map(|locale| locale as *const _).collect();

        // SAFETY: the engine handle is live; `pointers`, the `FlutterLocale`
        // structs it points at and the `CString`s those point at all outlive the
        // call, and the engine copies what it needs before returning.
        let result = unsafe {
            flutter_engine_sys::FlutterEngineUpdateLocales(
                self.engine_ptr(),
                pointers.as_mut_ptr(),
                pointers.len(),
            )
        };
        log_engine_result(result, "FlutterEngineUpdateLocales");
    }

    /// Fire-and-forget a raw, pre-encoded platform message on `channel` to Dart.
    ///
    /// This is the low-level escape hatch used by out-of-tree plugins that want
    /// to push events to Dart (e.g. a `MethodChannel` "invoke method" call whose
    /// body was encoded with a [`MethodCodec`](crate::codec::MethodCodec)).
    /// Must be called on the platform thread — wrap it in
    /// [`run_on_platform_thread`](Self::run_on_platform_thread) when emitting
    /// from a background thread. Failures are logged, not returned.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    pub fn post_message(&self, channel: &str, message: &[u8]) {
        self.send_platform_message(PlatformMessage {
            channel: std::borrow::Cow::Borrowed(channel),
            message,
            response_handle: None,
        });
    }

    pub(crate) fn send_platform_message(&self, message: PlatformMessage) {
        trace!("Sending message on channel {}", message.channel);
        assert!(
            self.is_platform_thread(),
            "send_platform_message must be called on the platform thread"
        );
        if !self.check_live("send_platform_message") {
            return;
        }

        // `raw` owns the channel name the engine copies during the call; it also
        // owns the response handle the message carried, so dropping it at the
        // end of this function releases that handle, which is what the embedder
        // API asks for once the message has been sent.
        let mut raw = match message.into_raw() {
            Ok(raw) => raw,
            Err(error) => {
                log::error!("the channel name contains an interior NUL byte: {error}");
                return;
            }
        };
        // SAFETY: the engine handle is live, and the `FlutterPlatformMessage`
        // and the channel name and payload it points at all outlive the call.
        let result = unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessage(self.engine_ptr(), raw.as_ffi())
        };
        if result == FlutterEngineResult::kSuccess {
            // The engine has taken the message and now holds its own reference
            // to the response object, so it — not the handle below — is what
            // ends the reply callback.
            raw.disarm_reply();
        } else {
            log::error!(
                "FlutterEngineSendPlatformMessage({}) failed: {result:?}",
                raw.channel()
            );
            // `raw` stays armed, so dropping it reclaims the reply callback
            // the engine will now never deliver.
        }
    }

    pub(crate) fn send_platform_message_response(
        &self,
        response_handle: PlatformMessageResponseHandle,
        bytes: &[u8],
    ) {
        trace!("Sending message response");
        assert!(
            self.is_platform_thread(),
            "send_platform_message_response must be called on the platform thread"
        );
        if !self.check_live("send_platform_message_response") {
            return;
        }

        // SAFETY: the engine handle is live, `response_handle` is a live handle
        // the engine gave us and has not been responded to yet, and `bytes` is
        // valid for `bytes.len()` for the duration of the call.
        let result = unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessageResponse(
                self.engine_ptr(),
                response_handle.into(),
                bytes.as_ptr(),
                bytes.len(),
            )
        };
        log_engine_result(result, "FlutterEngineSendPlatformMessageResponse");
    }

    /// Shut the engine down. Idempotent: a second call logs and does nothing.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    pub fn shutdown(&self) {
        assert!(
            self.is_platform_thread(),
            "shutdown must be called on the platform thread"
        );

        if self.inner.shut_down.swap(true, atomic::Ordering::AcqRel) {
            log::warn!("FlutterEngine::shutdown called more than once; ignoring");
            return;
        }

        // SAFETY: this is the first and only shutdown, so the engine handle is
        // still valid, and we are on the platform thread.
        let result = unsafe { flutter_engine_sys::FlutterEngineShutdown(self.engine_ptr()) };
        log_engine_result(result, "FlutterEngineShutdown");

        // `FlutterEngineShutdown` joins the engine's threads and destroys the
        // response objects it still holds *without* completing them, so from
        // here on no reply callback can be running, none can arrive, and
        // `PlatformMessageResponseHandle::new` refuses to register more
        // (it checks `is_shut_down`, on this same thread). Whatever is left in
        // the table for this engine is a reply that will never come: reclaim
        // it rather than leak it, which is what used to happen to every
        // message that was in flight at shutdown.
        drain_pending_replies(self.inner.reply_scope);
    }

    /// Run the tasks the engine has queued on the platform thread, plus any
    /// callbacks posted with [`run_on_platform_thread`](Self::run_on_platform_thread).
    ///
    /// Returns when the next queued engine task is due, if any.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    pub fn execute_platform_tasks(&self) -> Option<Instant> {
        assert!(
            self.is_platform_thread(),
            "execute_platform_tasks must be called on the platform thread"
        );
        if self.is_shut_down() {
            return None;
        }

        let next_task = self.inner.platform_runner.execute_tasks();

        let mut render_thread_fns = Vec::new();
        let callbacks: Vec<MainThreadCallback> = self.inner.platform_receiver.try_iter().collect();
        for cb in callbacks {
            match cb {
                MainThreadCallback::Engine(func) => func(self),
                MainThreadCallback::RenderThread(f) => render_thread_fns.push(f),
            }
        }
        if !render_thread_fns.is_empty() {
            let engine_copy = self.clone();
            self.post_render_thread_task(move || {
                for f in render_thread_fns {
                    f(&engine_copy);
                }
            });
        }

        next_task
    }

    pub(crate) fn run_task(&self, task: &FlutterTask) {
        if !self.check_live("run_task") {
            return;
        }

        // SAFETY: the engine handle is live and `task` is a task the engine
        // handed us through `post_task`, borrowed for the duration of the call.
        let result = unsafe {
            flutter_engine_sys::FlutterEngineRunTask(self.engine_ptr(), task as *const FlutterTask)
        };
        log_engine_result(result, "FlutterEngineRunTask");
    }

    fn post_render_thread_task<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if !self.check_live("post_render_thread_task") {
            return;
        }

        let b = Box::new(CallbackBox { cbk: Box::new(f) });
        let ptr = Box::into_raw(b);

        // SAFETY: the engine handle is live and `ptr` is a freshly leaked
        // `Box<CallbackBox>`, which `render_thread_task` turns back into a
        // `Box` exactly once.
        let result = unsafe {
            flutter_engine_sys::FlutterEnginePostRenderThreadTask(
                self.engine_ptr(),
                Some(render_thread_task),
                ptr as *mut c_void,
            )
        };
        if result != FlutterEngineResult::kSuccess {
            log::error!("FlutterEnginePostRenderThreadTask failed: {result:?}");
            // The engine will not call us back, so reclaim the box here.
            // SAFETY: `ptr` came from `Box::into_raw` just above and the engine
            // never saw it, so nothing else can own it.
            drop(unsafe { Box::from_raw(ptr) });
        }

        struct CallbackBox {
            pub cbk: Box<dyn FnOnce() + Send>,
        }

        /// # Safety
        ///
        /// `user_data` must be the `Box<CallbackBox>` pointer leaked above,
        /// delivered back by the engine exactly once.
        unsafe extern "C" fn render_thread_task(user_data: *mut c_void) {
            // SAFETY: the caller guarantees `user_data` is the leaked
            // `Box<CallbackBox>` and that this runs once for it.
            let b = unsafe { Box::from_raw(user_data as *mut CallbackBox) };
            // A panic must not reach the `extern "C"` boundary: it would abort
            // the process rather than unwind.
            if catch_unwind(AssertUnwindSafe(|| (b.cbk)())).is_err() {
                log::error!("panic in a render thread task");
            }
        }
    }

    /// Register a new external texture with the engine.
    #[must_use]
    pub fn create_texture(&self) -> Texture {
        self.inner.texture_registry.create_texture(self.clone())
    }
}

#[cfg(unix)]
fn path_to_cstring(path: &Path) -> Result<CString, NulError> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes())
}

#[cfg(not(unix))]
fn path_to_cstring(path: &Path) -> Result<CString, NulError> {
    CString::new(path.to_string_lossy().to_string())
}

/// Why [`FlutterEngineBuilder::build`](builder::FlutterEngineBuilder::build)
/// could not create an engine.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum CreateError {
    /// No OpenGL handler was set on the builder.
    #[error("no opengl handler set")]
    NoOpenGlHandler,
    /// No platform task runner handler was set on the builder.
    #[error("no platform task runner handler set")]
    NoPlatformHandler,
    /// An argument or path contains a NUL byte and cannot cross the FFI
    /// boundary.
    #[error("path or argument contains an interior nul byte")]
    InteriorNul,
    /// `FlutterEngineInitialize` refused the configuration.
    #[error("FlutterEngineInitialize failed with code {0}")]
    EngineInitialize(i32),
    /// `FlutterEngineInitialize` reported success but produced no handle.
    #[error("engine pointer is null")]
    EnginePtrNull,
}

impl From<NulError> for CreateError {
    fn from(_: NulError) -> Self {
        CreateError::InteriorNul
    }
}

/// Why [`FlutterEngine::run`] could not start the engine.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RunError {
    /// `run` was called from a thread other than the platform thread.
    #[error("run must be called on the platform thread")]
    NotPlatformThread,
    /// The engine has already been shut down.
    #[error("the engine has already been shut down")]
    ShutDown,
    /// `FlutterEngineRunInitialized` failed with this embedder result code.
    #[error("FlutterEngineRunInitialized failed with code {0}")]
    Engine(i32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_error_messages_are_lowercase_and_unterminated() {
        for error in [
            CreateError::NoOpenGlHandler,
            CreateError::NoPlatformHandler,
            CreateError::InteriorNul,
            CreateError::EngineInitialize(2),
            CreateError::EnginePtrNull,
        ] {
            let msg = error.to_string();
            assert!(!msg.ends_with('\n'), "{msg:?} ends in a newline");
            assert!(!msg.ends_with('.'), "{msg:?} ends in a period");
            // Lowercase first word, so the message composes into
            // `"failed to start: {err}"`. A message naming an embedder entry
            // point keeps that symbol's own spelling.
            assert!(
                !msg.starts_with(|c: char| c.is_uppercase()) || msg.starts_with("FlutterEngine"),
                "{msg:?} is capitalised"
            );
        }
    }

    #[test]
    fn interior_nul_is_reported_not_panicked() {
        assert_eq!(
            path_to_cstring(Path::new("as\0sets")).map_err(CreateError::from),
            Err(CreateError::InteriorNul)
        );
        assert!(path_to_cstring(Path::new("/tmp/assets")).is_ok());
    }

    #[test]
    fn run_error_carries_the_engine_code() {
        assert_eq!(
            RunError::Engine(2).to_string(),
            "FlutterEngineRunInitialized failed with code 2"
        );
        assert_eq!(
            RunError::NotPlatformThread.to_string(),
            "run must be called on the platform thread"
        );
    }

    #[test]
    fn a_default_weak_ref_is_not_valid() {
        let weak = FlutterEngineWeakRef::default();
        assert!(weak.upgrade().is_none());
        assert!(!weak.is_valid());
        assert!(weak.ptr_equal(&weak.clone()));
    }

    /// The handles must stay usable from other threads: the engine's raster and
    /// IO threads hold them.
    #[test]
    fn handles_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FlutterEngine>();
        assert_send_sync::<FlutterEngineWeakRef>();
        assert_send_sync::<CreateError>();
        assert_send_sync::<RunError>();
    }
}
