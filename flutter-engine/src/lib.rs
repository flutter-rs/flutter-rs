#[macro_use]
mod macros;

pub mod channel;
pub mod codec;
pub mod error;
pub mod ffi;
mod flutter_callbacks;
pub mod plugins;
pub mod tasks;
pub mod texture_registry;
pub mod utils;

use crate::channel::{Channel, ChannelRegistrar};
use crate::ffi::{
    FlutterPointerDeviceKind, FlutterPointerMouseButtons, FlutterPointerPhase,
    FlutterPointerSignalKind, PlatformMessage, PlatformMessageResponseHandle,
};
use crate::plugins::{Plugin, PluginRegistrar};
use crate::tasks::{TaskRunner, TaskRunnerHandler};
use crate::texture_registry::{Texture, TextureRegistry};
use crossbeam_channel::{unbounded, Receiver, Sender};
use flutter_engine_sys::FlutterTask;
use log::trace;
use parking_lot::RwLock;
use std::ffi::CString;
use std::future::Future;
use std::os::raw::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::{mem, ptr};

pub(crate) type MainThreadEngineFn = Box<dyn FnOnce(&FlutterEngine) + Send>;
pub(crate) type MainThreadChannelFn = (String, Box<dyn FnMut(&dyn Channel) + Send>);
pub(crate) type MainThreadRenderThreadFn = Box<dyn FnOnce(&FlutterEngine) + Send>;

pub(crate) enum MainThreadCallback {
    Engine(MainThreadEngineFn),
    Channel(MainThreadChannelFn),
    RenderThread(MainThreadRenderThreadFn),
}

struct FlutterEngineInner {
    handler: Weak<dyn FlutterEngineHandler>,
    engine_ptr: AtomicPtr<flutter_engine_sys::_FlutterEngine>,
    plugins: RwLock<PluginRegistrar>,
    platform_runner: TaskRunner,
    _platform_runner_handler: Arc<PlatformRunnerHandler>,
    platform_receiver: Receiver<MainThreadCallback>,
    platform_sender: Sender<MainThreadCallback>,
    texture_registry: TextureRegistry,
    assets: PathBuf,
}

pub struct FlutterEngineWeakRef {
    inner: Weak<FlutterEngineInner>,
}

unsafe impl Send for FlutterEngineWeakRef {}

unsafe impl Sync for FlutterEngineWeakRef {}

impl FlutterEngineWeakRef {
    pub fn upgrade(&self) -> Option<FlutterEngine> {
        match self.inner.upgrade() {
            None => None,
            Some(arc) => Some(FlutterEngine { inner: arc }),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.upgrade().is_some()
    }

    pub fn ptr_equal(&self, other: Self) -> bool {
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

pub struct FlutterEngine {
    inner: Arc<FlutterEngineInner>,
}

unsafe impl Send for FlutterEngine {}

unsafe impl Sync for FlutterEngine {}

impl Clone for FlutterEngine {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub trait FlutterEngineHandler {
    fn swap_buffers(&self) -> bool;

    fn make_current(&self) -> bool;

    fn clear_current(&self) -> bool;

    fn fbo_callback(&self) -> u32;

    fn make_resource_current(&self) -> bool;

    fn gl_proc_resolver(&self, proc: *const c_char) -> *mut c_void;

    fn wake_platform_thread(&self);

    fn run_in_background(&self, func: Box<dyn Future<Output = ()> + Send + 'static>);
}

struct PlatformRunnerHandler {
    handler: Weak<dyn FlutterEngineHandler>,
}

impl TaskRunnerHandler for PlatformRunnerHandler {
    fn wake(&self) {
        if let Some(handler) = self.handler.upgrade() {
            handler.wake_platform_thread();
        }
    }
}

impl FlutterEngine {
    pub fn new(handler: Weak<dyn FlutterEngineHandler>, assets: PathBuf) -> Self {
        let platform_handler = Arc::new(PlatformRunnerHandler {
            handler: handler.clone(),
        });

        let (main_tx, main_rx) = unbounded();

        let engine = Self {
            inner: Arc::new(FlutterEngineInner {
                handler,
                engine_ptr: AtomicPtr::new(ptr::null_mut()),
                plugins: RwLock::new(PluginRegistrar::new()),
                platform_runner: TaskRunner::new(
                    Arc::downgrade(&platform_handler) as Weak<dyn TaskRunnerHandler>
                ),
                _platform_runner_handler: platform_handler,
                platform_receiver: main_rx,
                platform_sender: main_tx,
                texture_registry: TextureRegistry::new(),
                assets,
            }),
        };

        let inner = &engine.inner;
        inner.plugins.write().init(engine.downgrade());
        inner.platform_runner.init(engine.downgrade());

        engine
    }

    #[inline]
    pub fn engine_ptr(&self) -> flutter_engine_sys::FlutterEngine {
        self.inner.engine_ptr.load(Ordering::Relaxed)
    }

    pub fn add_plugin<P>(&self, plugin: P) -> &Self
    where
        P: Plugin + 'static,
    {
        self.inner.plugins.write().add_plugin(plugin);
        self
    }

    pub fn with_plugin<F, P>(&self, f: F)
    where
        F: FnOnce(&P),
        P: Plugin + 'static,
    {
        self.inner.plugins.read().with_plugin(f)
    }

    pub fn with_plugin_mut<F, P>(&self, f: F)
    where
        F: FnOnce(&mut P),
        P: Plugin + 'static,
    {
        self.inner.plugins.write().with_plugin_mut(f)
    }

    pub fn remove_channel(&self, channel_name: &str) -> Option<Arc<dyn Channel>> {
        self.inner
            .plugins
            .write()
            .channel_registry
            .remove_channel(channel_name)
    }

    pub fn with_channel<F>(&self, channel_name: &str, f: F)
    where
        F: FnOnce(&dyn Channel),
    {
        self.inner
            .plugins
            .read()
            .channel_registry
            .with_channel(channel_name, f)
    }

    pub fn with_channel_registrar<F>(&self, plugin_name: &'static str, f: F)
    where
        F: FnOnce(&mut ChannelRegistrar),
    {
        self.inner
            .plugins
            .write()
            .channel_registry
            .with_channel_registrar(plugin_name, f)
    }

    pub fn downgrade(&self) -> FlutterEngineWeakRef {
        FlutterEngineWeakRef {
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub fn assets(&self) -> &Path {
        &self.inner.assets
    }

    pub fn run(&self, arguments: &[String]) -> Result<(), RunError> {
        if !self.is_platform_thread() {
            return Err(RunError::NotPlatformThread);
        }

        let mut args = Vec::with_capacity(arguments.len() + 2);
        args.push(CString::new("flutter-rs").unwrap().into_raw());
        args.push(
            CString::new("--icu-symbol-prefix=gIcudtl")
                .unwrap()
                .into_raw(),
        );
        for arg in arguments.iter() {
            args.push(CString::new(arg.as_str()).unwrap().into_raw());
        }

        let renderer_config = flutter_engine_sys::FlutterRendererConfig {
            type_: flutter_engine_sys::FlutterRendererType::kOpenGL,
            __bindgen_anon_1: flutter_engine_sys::FlutterRendererConfig__bindgen_ty_1 {
                open_gl: flutter_engine_sys::FlutterOpenGLRendererConfig {
                    struct_size: std::mem::size_of::<flutter_engine_sys::FlutterOpenGLRendererConfig>(
                    ),
                    make_current: Some(flutter_callbacks::make_current),
                    clear_current: Some(flutter_callbacks::clear_current),
                    present: Some(flutter_callbacks::present),
                    fbo_callback: Some(flutter_callbacks::fbo_callback),
                    make_resource_current: Some(flutter_callbacks::make_resource_current),
                    fbo_reset_after_present: false,
                    surface_transformation: None,
                    gl_proc_resolver: Some(flutter_callbacks::gl_proc_resolver),
                    gl_external_texture_frame_callback: Some(
                        flutter_callbacks::gl_external_texture_frame,
                    ),
                },
            },
        };

        // TODO: Should be downgraded to a weak once weak::into_raw lands in stable
        let runner_ptr = {
            let arc = self.inner.platform_runner.clone().inner;
            Arc::into_raw(arc) as *mut std::ffi::c_void
        };

        let platform_task_runner = flutter_engine_sys::FlutterTaskRunnerDescription {
            struct_size: std::mem::size_of::<flutter_engine_sys::FlutterTaskRunnerDescription>(),
            user_data: runner_ptr,
            runs_task_on_current_thread_callback: Some(
                flutter_callbacks::runs_task_on_current_thread,
            ),
            post_task_callback: Some(flutter_callbacks::post_task),
            identifier: 0,
        };
        let custom_task_runners = flutter_engine_sys::FlutterCustomTaskRunners {
            struct_size: std::mem::size_of::<flutter_engine_sys::FlutterCustomTaskRunners>(),
            platform_task_runner: &platform_task_runner
                as *const flutter_engine_sys::FlutterTaskRunnerDescription,
            render_task_runner: &platform_task_runner
                as *const flutter_engine_sys::FlutterTaskRunnerDescription,
        };

        let project_args = flutter_engine_sys::FlutterProjectArgs {
            struct_size: std::mem::size_of::<flutter_engine_sys::FlutterProjectArgs>(),
            assets_path: path_to_cstring(self.assets()).into_raw(),
            main_path__unused__: std::ptr::null(),
            packages_path__unused__: std::ptr::null(),
            icu_data_path: std::ptr::null(),
            command_line_argc: args.len() as i32,
            command_line_argv: args.as_mut_ptr() as _,
            platform_message_callback: Some(flutter_callbacks::platform_message_callback),
            vm_snapshot_data: std::ptr::null(),
            vm_snapshot_data_size: 0,
            vm_snapshot_instructions: std::ptr::null(),
            vm_snapshot_instructions_size: 0,
            isolate_snapshot_data: std::ptr::null(),
            isolate_snapshot_data_size: 0,
            isolate_snapshot_instructions: std::ptr::null(),
            isolate_snapshot_instructions_size: 0,
            root_isolate_create_callback: Some(flutter_callbacks::root_isolate_create_callback),
            update_semantics_node_callback: None,
            update_semantics_custom_action_callback: None,
            persistent_cache_path: std::ptr::null(),
            is_persistent_cache_read_only: false,
            vsync_callback: None,
            custom_dart_entrypoint: std::ptr::null(),
            custom_task_runners: &custom_task_runners
                as *const flutter_engine_sys::FlutterCustomTaskRunners,
            shutdown_dart_vm_when_done: true,
            compositor: std::ptr::null(),
        };

        unsafe {
            // TODO: Should be downgraded to a weak once weak::into_raw lands in stable
            let inner_ptr = Arc::into_raw(self.inner.clone()) as *mut std::ffi::c_void;

            let engine_ptr: flutter_engine_sys::FlutterEngine = std::ptr::null_mut();
            if flutter_engine_sys::FlutterEngineRun(
                1,
                &renderer_config,
                &project_args,
                inner_ptr,
                &engine_ptr as *const flutter_engine_sys::FlutterEngine
                    as *mut flutter_engine_sys::FlutterEngine,
            ) != flutter_engine_sys::FlutterEngineResult::kSuccess
                || engine_ptr.is_null()
            {
                Err(RunError::EnginePtrNull)
            } else {
                self.inner.engine_ptr.store(engine_ptr, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    pub(crate) fn post_platform_callback(&self, callback: MainThreadCallback) {
        self.inner.platform_sender.send(callback).unwrap();
        self.inner.platform_runner.wake();
    }

    #[inline]
    pub fn is_platform_thread(&self) -> bool {
        self.inner.platform_runner.runs_task_on_current_thread()
    }

    pub fn run_on_platform_thread<F>(&self, f: F)
    where
        F: FnOnce(&FlutterEngine) -> () + 'static + Send,
    {
        if self.is_platform_thread() {
            f(self);
        } else {
            self.post_platform_callback(MainThreadCallback::Engine(Box::new(f)));
        }
    }

    pub fn run_on_render_thread<F>(&self, f: F)
    where
        F: FnOnce(&FlutterEngine) -> () + 'static + Send,
    {
        if self.is_platform_thread() {
            f(self);
        } else {
            self.post_platform_callback(MainThreadCallback::RenderThread(Box::new(f)));
        }
    }

    pub fn run_in_background(&self, future: impl Future<Output = ()> + Send + 'static) {
        if let Some(handler) = self.inner.handler.upgrade() {
            handler.run_in_background(Box::new(future));
        }
    }

    pub fn send_window_metrics_event(&self, width: usize, height: usize, pixel_ratio: f64) {
        if !self.is_platform_thread() {
            panic!("Not on platform thread");
        }

        let event = flutter_engine_sys::FlutterWindowMetricsEvent {
            struct_size: std::mem::size_of::<flutter_engine_sys::FlutterWindowMetricsEvent>(),
            width,
            height,
            pixel_ratio,
            #[cfg(all(target_arch = "arm", target_os = "android"))]
            __bindgen_padding_0: 0,
        };
        unsafe {
            flutter_engine_sys::FlutterEngineSendWindowMetricsEvent(self.engine_ptr(), &event);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_pointer_event(
        &self,
        device: i32,
        phase: FlutterPointerPhase,
        (x, y): (f64, f64),
        signal_kind: FlutterPointerSignalKind,
        (scroll_delta_x, scroll_delta_y): (f64, f64),
        device_kind: FlutterPointerDeviceKind,
        buttons: FlutterPointerMouseButtons,
    ) {
        if !self.is_platform_thread() {
            panic!("Not on platform thread");
        }

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let buttons: flutter_engine_sys::FlutterPointerMouseButtons = buttons.into();
        let event = flutter_engine_sys::FlutterPointerEvent {
            struct_size: mem::size_of::<flutter_engine_sys::FlutterPointerEvent>(),
            timestamp: timestamp.as_micros() as usize,
            phase: phase.into(),
            x,
            y,
            device,
            signal_kind: signal_kind.into(),
            scroll_delta_x,
            scroll_delta_y,
            device_kind: device_kind.into(),
            buttons: buttons as i64,
            #[cfg(all(target_arch = "arm", target_os = "android"))]
            __bindgen_padding_0: 0,
            #[cfg(all(target_arch = "arm", target_os = "android"))]
            __bindgen_padding_1: 0,
        };
        unsafe {
            flutter_engine_sys::FlutterEngineSendPointerEvent(self.engine_ptr(), &event, 1);
        }
    }

    pub(crate) fn send_platform_message(&self, message: PlatformMessage) {
        trace!("Sending message on channel {}", message.channel);
        if !self.is_platform_thread() {
            panic!("Not on platform thread");
        }

        unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessage(
                self.engine_ptr(),
                &message.into(),
            );
        }
    }

    pub(crate) fn send_platform_message_response(
        &self,
        response_handle: PlatformMessageResponseHandle,
        bytes: &[u8],
    ) {
        trace!("Sending message response");
        if !self.is_platform_thread() {
            panic!("Not on platform thread");
        }

        unsafe {
            flutter_engine_sys::FlutterEngineSendPlatformMessageResponse(
                self.engine_ptr(),
                response_handle.into(),
                bytes.as_ptr(),
                bytes.len(),
            );
        }
    }

    pub fn shutdown(&self) {
        if !self.is_platform_thread() {
            panic!("Not on platform thread")
        }

        unsafe {
            flutter_engine_sys::FlutterEngineShutdown(self.engine_ptr());
        }
    }

    pub fn execute_platform_tasks(&self) -> Option<Instant> {
        if !self.is_platform_thread() {
            panic!("Not on platform thread")
        }

        let next_task = self.inner.platform_runner.execute_tasks();

        let mut render_thread_fns = Vec::new();
        let callbacks: Vec<MainThreadCallback> = self.inner.platform_receiver.try_iter().collect();
        for cb in callbacks {
            match cb {
                MainThreadCallback::Engine(func) => func(self),
                MainThreadCallback::Channel((name, mut f)) => {
                    self.inner
                        .plugins
                        .write()
                        .channel_registry
                        .with_channel(&name, |channel| {
                            f(channel);
                        });
                }
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
        unsafe {
            flutter_engine_sys::FlutterEngineRunTask(self.engine_ptr(), task as *const FlutterTask);
        }
    }

    fn post_render_thread_task<F>(&self, f: F)
    where
        F: FnOnce() -> () + 'static,
    {
        unsafe {
            let cbk = CallbackBox { cbk: Box::new(f) };
            let b = Box::new(cbk);
            let ptr = Box::into_raw(b);
            flutter_engine_sys::FlutterEnginePostRenderThreadTask(
                self.engine_ptr(),
                Some(render_thread_task),
                ptr as *mut c_void,
            );
        }

        struct CallbackBox {
            pub cbk: Box<dyn FnOnce()>,
        }

        unsafe extern "C" fn render_thread_task(user_data: *mut c_void) {
            let ptr = user_data as *mut CallbackBox;
            let b = Box::from_raw(ptr);
            (b.cbk)()
        }
    }

    pub fn create_texture(&self) -> Texture {
        self.inner.texture_registry.create_texture(self.clone())
    }
}

#[cfg(unix)]
fn path_to_cstring(path: &Path) -> CString {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes()).unwrap()
}

#[cfg(not(unix))]
fn path_to_cstring(path: &Path) -> CString {
    CString::new(path.to_string_lossy().to_string()).unwrap()
}

#[derive(Debug, Eq, PartialEq)]
pub enum RunError {
    NotPlatformThread,
    EnginePtrNull,
}

impl core::fmt::Display for RunError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let msg = match self {
            RunError::NotPlatformThread => "Not on platform thread.",
            RunError::EnginePtrNull => "Engine ptr is null.",
        };
        writeln!(f, "{}", msg)
    }
}

impl std::error::Error for RunError {}
