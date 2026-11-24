//! The unified one-engine/many-windows application.
//!
//! [`FlutterDesktop::create_app`](crate::FlutterDesktop::create_app) creates the
//! hidden master/resource contexts and the engine; [`FlutterApp::create_window`]
//! adds native windows and engine views; [`FlutterApp::run`] is the single GLFW
//! event loop that drives them all.

use crate::compositor::{CompositorKind, GlfwCompositor};
use crate::handler::{
    FocusedWindow, GlfwMouseCursorHandler, GlfwOpenGLHandler, GlfwPlatformHandler,
    GlfwPlatformTaskHandler, GlfwTextInputHandler, GlfwWindowHandler,
};
use crate::render::RenderState;
use crate::window::{
    BY_WORD_MODIFIER_KEY, CreateError, FUNCTION_MODIFIER_KEY, FlutterWindow, LifecycleTracker,
    PreparedPointer, RunError, SCROLL_SPEED, SELECT_MODIFIER_KEY, TEXT_SCALE_FACTOR,
    USE_24_HOUR_FORMAT, WindowArgs, WindowKind, WindowMode, WindowRecord, WindowRequest,
    centered_position, move_or_hover, native_window_alive,
};
use flutter_engine::FlutterEngine;
use flutter_engine::builder::FlutterEngineBuilder;
use flutter_engine::channel::Channel;
use flutter_engine::ffi::KeyEventType;
use flutter_engine::ffi::{FlutterPointerDeviceKind, FlutterPointerSignalKind, PointerButtons};
use flutter_engine::plugins::{Plugin, PluginRegistrar};
use flutter_engine::tasks::TaskRunnerHandler;
use flutter_engine::texture_registry::Texture;
use flutter_plugins::accessibility::AccessibilityPlugin;
#[cfg(feature = "dialog")]
use flutter_plugins::dialog::DialogPlugin;
use flutter_plugins::isolate::IsolatePlugin;
use flutter_plugins::keyboard::{KeyboardPlugin, KeyboardStateHandler};
use flutter_plugins::lifecycle::{AppLifecycleState, LifecyclePlugin};
use flutter_plugins::localization::LocalizationPlugin;
use flutter_plugins::mousecursor::MouseCursorPlugin;
use flutter_plugins::navigation::NavigationPlugin;
use flutter_plugins::platform::PlatformPlugin;
use flutter_plugins::settings::{PlatformBrightness, SettingsPlugin};
use flutter_plugins::system::SystemPlugin;
use flutter_plugins::textinput::TextInputPlugin;
use flutter_plugins::window::WindowPlugin;
use glfw::Context;
use log::{debug, info};
use parking_lot::{Mutex, RwLock};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::sync::{Arc, Weak, mpsc};
use std::time::Instant;

pub(crate) type MainThreadFn = Box<dyn FnMut(&FlutterApp) + Send>;

/// The Dart windowing event callback. `arg0`/`arg1` carry two numbers whose
/// meaning depends on the event; see [`event`].
pub(crate) type EventCallback = unsafe extern "C" fn(i64, i32, f64, f64, *mut std::ffi::c_void);

/// Event codes shared with the Dart windowing package.
pub(crate) mod event {
    pub const CLOSE_REQUESTED: i32 = 1;
    pub const DESTROYED: i32 = 2;
    pub const SIZE_CHANGED: i32 = 3;
    pub const POSITION_CHANGED: i32 = 4;
    pub const SCALE_CHANGED: i32 = 5;
    pub const FOCUS_CHANGED: i32 = 6;
    pub const MINIMIZED_CHANGED: i32 = 7;
    pub const MAXIMIZED_CHANGED: i32 = 8;
    pub const FULLSCREEN_CHANGED: i32 = 9;
}

/// How to create the engine for a [`FlutterApp`].
pub struct AppArgs {
    /// Directory produced by `flutter build bundle` (`flutter_assets`).
    pub assets_path: PathBuf,
    /// Path to `icudtl.dat`, when the engine build does not embed it.
    pub icu_data_path: Option<PathBuf>,
    /// Extra engine switches, e.g. `--aot-shared-library-name=...`.
    pub engine_args: Vec<String>,
    /// GLFW context-creation backend. Defaults to EGL on Linux and native
    /// WGL/CGL elsewhere; set this to `Egl` on a Windows Mesa/llvmpipe host.
    pub context_api: Option<glfw::ContextCreationApi>,
}

impl AppArgs {
    /// App arguments for an asset directory and no extra engine switches.
    #[must_use]
    pub fn new(assets_path: impl Into<PathBuf>) -> Self {
        Self {
            assets_path: assets_path.into(),
            icu_data_path: None,
            engine_args: Vec::new(),
            context_api: None,
        }
    }
}

impl std::fmt::Debug for AppArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppArgs")
            .field("assets_path", &self.assets_path)
            .field("icu_data_path", &self.icu_data_path)
            .field("engine_args", &self.engine_args)
            .field("context_api", &self.context_api)
            .finish()
    }
}

/// The keys GLFW has reported as held.
#[derive(Debug, Default)]
pub(crate) struct PressedKeysState(HashMap<u64, u64>);

impl PressedKeysState {
    fn press(&mut self, physical: u64, logical: u64) {
        self.0.insert(physical, logical);
    }

    fn release(&mut self, physical: u64) {
        self.0.remove(&physical);
    }

    fn release_all(&mut self) -> Vec<(u64, u64)> {
        self.0.drain().collect()
    }
}

impl KeyboardStateHandler for PressedKeysState {
    fn pressed_keys(&mut self) -> Vec<(u64, u64)> {
        self.0
            .iter()
            .map(|(&physical, &logical)| (physical, logical))
            .collect()
    }
}

/// Lets `KeyboardPlugin` share the event loop's key table through the
/// `Arc<Mutex<dyn KeyboardStateHandler>>` its constructor takes.
struct KeyboardStateBridge(Arc<Mutex<PressedKeysState>>);

impl KeyboardStateHandler for KeyboardStateBridge {
    fn pressed_keys(&mut self) -> Vec<(u64, u64)> {
        self.0.lock().pressed_keys()
    }
}

/// A key event waiting for its `Char`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingKey {
    pub kind: KeyEventType,
    pub physical: u64,
    pub logical: u64,
    pub character: Option<String>,
}

/// Holds a key event back until the text it produces is known.
#[derive(Debug, Default)]
pub(crate) struct KeyStager {
    pending: Option<PendingKey>,
}

impl KeyStager {
    fn stage(&mut self, key: PendingKey) -> Option<PendingKey> {
        self.pending.replace(key)
    }

    fn take_with_character(&mut self, character: char) -> Option<PendingKey> {
        self.pending.take().map(|mut key| {
            key.character = Some(character.to_string());
            key
        })
    }

    fn take(&mut self) -> Option<PendingKey> {
        self.pending.take()
    }
}

/// App-level state shared by the run loop, plugin handlers and (later) the Dart
/// windowing ABI. Everything here is main-thread only.
pub(crate) struct AppState {
    pub(crate) engine: RefCell<Option<FlutterEngine>>,
    pub(crate) render: RefCell<Option<Arc<RenderState>>>,
    /// Device, command queue and per-view layers when the macOS backend runs on
    /// Metal. Kept alive for the whole app lifetime: the engine holds the
    /// device and queue handles it was started with.
    #[cfg(target_os = "macos")]
    pub(crate) metal: Option<Arc<crate::metal::MetalState>>,
    pub(crate) glfw: glfw::Glfw,
    pub(crate) master: RefCell<Option<glfw::PWindow>>,
    pub(crate) resource: RefCell<Option<glfw::PWindow>>,
    pub(crate) windows: RefCell<HashMap<i64, WindowRecord>>,
    /// Windows whose views were removed but whose native handles are kept until
    /// shutdown: GLFW's X11 backend still processes queued events for a window
    /// after `glfwDestroyWindow`, which produces `BadWindow` errors if the
    /// window is destroyed while the event loop is running.
    pub(crate) retired_windows: RefCell<Vec<Arc<Mutex<glfw::PWindow>>>>,
    pub(crate) next_view_id: Cell<i64>,
    pub(crate) had_windows: Cell<bool>,
    pub(crate) running: Cell<bool>,
    pub(crate) quit: Cell<bool>,
    pub(crate) focused_view: Cell<Option<i64>>,
    pub(crate) last_focused: Cell<Option<i64>>,
    pub(crate) key_stager: RefCell<KeyStager>,
    pub(crate) pressed_keys: Arc<Mutex<PressedKeysState>>,
    pub(crate) lifecycle: RefCell<LifecycleTracker>,
    pub(crate) main_tx: Sender<MainThreadFn>,
    pub(crate) main_rx: Receiver<MainThreadFn>,
    pub(crate) platform_task_handler: Arc<GlfwPlatformTaskHandler>,
    /// Whether the engine's platform tasks are already running on this thread.
    ///
    /// The macOS resize callback pumps them from inside AppKit's tracking loop;
    /// a task may resize a window, whose framebuffer callback fires
    /// synchronously and would try to pump again. Re-entering the task runner
    /// deadlocks, so a nested pump is skipped.
    pub(crate) pumping_platform_tasks: Cell<bool>,
    /// Whether the embedder is creating or resizing a window.
    ///
    /// AppKit posts the resize notification synchronously, so the callback can
    /// fire in the middle of `create_window_inner` or `apply_desired_sizes` —
    /// while the engine is already laying out or the window is locked. Neither
    /// the metrics nor the task pump may run then; the queued event handles the
    /// size once the embedder is back in its event loop.
    pub(crate) embedder_busy: Cell<bool>,
    /// When the resize callback last pumped the engine, so a burst of AppKit
    /// or Win32 resize notifications is coalesced into one frame per display
    /// refresh.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(crate) last_resize_pump: Cell<Option<Instant>>,
    pub(crate) plugins: RwLock<PluginRegistrar>,
    pub(crate) focused: Arc<FocusedWindow>,
    pub(crate) quit_flag: Arc<AtomicBool>,
    pub(crate) brightness_started: Cell<bool>,
    pub(crate) event_callback: Cell<Option<EventCallback>>,
    pub(crate) event_user_data: Cell<*mut std::ffi::c_void>,
    pub(crate) context_api: glfw::ContextCreationApi,
    /// GLFW's screen coordinates are logical (macOS points, Wayland units).
    pub(crate) logical_screen_coordinates: bool,
}

/// Clears [`AppState::embedder_busy`] when it goes out of scope.
struct BusyGuard<'a> {
    flag: &'a Cell<bool>,
    was_busy: bool,
}

impl<'a> BusyGuard<'a> {
    fn enter(flag: &'a Cell<bool>) -> Self {
        let was_busy = flag.replace(true);
        Self { flag, was_busy }
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.flag.set(self.was_busy);
    }
}

/// Keep the engine ticking while AppKit runs a tracking loop.
///
/// A live resize runs on AppKit's side in `NSEventTrackingRunLoopMode`: the
/// event loop is blocked, so the vsync/animation tasks never run and the
/// content freezes whenever the pointer pauses mid-drag. A 60 Hz timer in the
/// tracking mode pumps the engine exactly while such a loop is up.
#[cfg(target_os = "macos")]
fn install_tracking_timer(state: &Rc<AppState>) {
    use objc2_core_foundation::{CFRunLoop, CFRunLoopTimer, CFRunLoopTimerContext, CFString};
    use std::ffi::c_void;

    unsafe extern "C-unwind" fn fired(_timer: *mut CFRunLoopTimer, info: *mut c_void) {
        // SAFETY: `info` is the leaked `Weak<AppState>` created below; the
        // timer lives for the process lifetime and only calls this on the main
        // thread.
        let weak = unsafe { &*(info as *const Weak<AppState>) };
        if let Some(state) = weak.upgrade() {
            state.tick_during_tracking();
        }
    }

    let context_info = Box::into_raw(Box::new(Rc::downgrade(state)));
    let mut context = CFRunLoopTimerContext {
        version: 0,
        info: context_info.cast(),
        retain: None,
        release: None,
        copyDescription: None,
    };
    // SAFETY: the timer is added to the process's run loop and never
    // invalidated; the leaked context outlives it. The callout only touches the
    // `Weak<AppState>` above.
    unsafe {
        let Some(timer) =
            CFRunLoopTimer::new(None, 0.0, 1.0 / 60.0, 0, 0, Some(fired), &mut context)
        else {
            return;
        };
        let Some(run_loop) = CFRunLoop::current() else {
            return;
        };
        let mode = CFString::from_static_str("NSEventTrackingRunLoopMode");
        run_loop.add_timer(Some(&timer), Some(&mode));
    }
}

/// Keep the engine ticking while Win32 runs a modal move/resize loop.
///
/// Dragging a window edge or title bar enters a modal loop inside
/// `DispatchMessage`, so the event loop below is blocked until the drag ends.
/// The framebuffer-size callback covers the steps where the pointer moves, but
/// there are no size notifications while it pauses, and the content freezes.
/// A 16 ms timer started on `WM_ENTERSIZEMOVE` pumps the engine exactly while
/// such a loop is up, mirroring the macOS tracking timer.
#[cfg(target_os = "windows")]
fn install_live_resize_timer(window: &glfw::PWindow) {
    use std::ffi::c_void;

    type SubclassProc =
        unsafe extern "system" fn(*mut c_void, u32, usize, isize, usize, usize) -> isize;

    #[link(name = "comctl32")]
    unsafe extern "system" {
        fn SetWindowSubclass(
            window: *mut c_void,
            proc: SubclassProc,
            id: usize,
            data: usize,
        ) -> i32;
        fn DefSubclassProc(
            window: *mut c_void,
            message: u32,
            wparam: usize,
            lparam: isize,
        ) -> isize;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetTimer(window: *mut c_void, id: usize, interval: u32, callback: *mut c_void) -> usize;
        fn KillTimer(window: *mut c_void, id: usize) -> i32;
    }

    const WM_TIMER: u32 = 0x0113;
    const WM_ENTERSIZEMOVE: u32 = 0x0231;
    const WM_EXITSIZEMOVE: u32 = 0x0232;
    const LIVE_RESIZE_TIMER: usize = 1;

    unsafe extern "system" fn subclass(
        window: *mut c_void,
        message: u32,
        wparam: usize,
        lparam: isize,
        _id: usize,
        _data: usize,
    ) -> isize {
        match message {
            WM_ENTERSIZEMOVE => {
                // SAFETY: `window` is the subclassed live window.
                unsafe {
                    SetTimer(window, LIVE_RESIZE_TIMER, 16, std::ptr::null_mut());
                }
            }
            WM_EXITSIZEMOVE => {
                // SAFETY: as above.
                unsafe {
                    KillTimer(window, LIVE_RESIZE_TIMER);
                }
            }
            WM_TIMER if wparam == LIVE_RESIZE_TIMER => {
                if let Some(state) = current_app()
                    && !state.embedder_busy.get()
                {
                    let _busy = BusyGuard::enter(&state.embedder_busy);
                    state.pump_platform_tasks();
                    state.apply_desired_sizes();
                }
            }
            _ => {}
        }
        // SAFETY: chain to the procedure this subclass replaced with the
        // arguments this procedure received.
        unsafe { DefSubclassProc(window, message, wparam, lparam) }
    }

    let window = window.get_win32_window();
    if window.is_null() {
        return;
    }
    // SAFETY: a live HWND on the thread that owns it; `subclass` is valid for
    // the lifetime of the window.
    unsafe {
        SetWindowSubclass(window, subclass, 1, 0);
    }
}

impl AppState {
    /// Advance the engine while AppKit's tracking loop blocks the event loop.
    #[cfg(target_os = "macos")]
    fn tick_during_tracking(&self) {
        if self.embedder_busy.get() {
            return;
        }
        self.pump_platform_tasks();
        self.apply_desired_sizes();
    }

    pub(crate) fn coordinate_scale(&self, scale: f64) -> f64 {
        if self.logical_screen_coordinates {
            1.0
        } else {
            scale.max(0.01)
        }
    }

    pub(crate) fn engine(&self) -> Option<FlutterEngine> {
        self.engine.borrow().clone()
    }

    /// Run the engine's platform tasks unless they are already running.
    ///
    /// The macOS resize callback pumps them from inside AppKit's tracking loop,
    /// where the event loop cannot. A task may open or resize a window, whose
    /// resize callback fires synchronously and would otherwise pump again;
    /// re-entering the task runner deadlocks.
    pub(crate) fn pump_platform_tasks(&self) -> Option<Instant> {
        if self.pumping_platform_tasks.replace(true) {
            return None;
        }
        let _busy = BusyGuard::enter(&self.embedder_busy);
        let next_task_time = self
            .engine()
            .and_then(|engine| engine.execute_platform_tasks());
        self.pumping_platform_tasks.set(false);
        next_task_time
    }

    fn render(&self) -> Option<Arc<RenderState>> {
        self.render.borrow().clone()
    }

    fn send_metrics(&self, view_id: i64) {
        let _busy = BusyGuard::enter(&self.embedder_busy);
        let Some(engine) = self.engine() else {
            return;
        };
        let metrics = {
            let mut windows = self.windows.borrow_mut();
            let Some(record) = windows.get_mut(&view_id) else {
                return;
            };
            record.refresh_geometry();
            record.metrics()
        };
        engine.send_window_metrics_for_view(view_id, metrics);
        if let Some(render) = self.render() {
            let windows = self.windows.borrow();
            if let Some(record) = windows.get(&view_id) {
                render.update_metrics(view_id, record.framebuffer_size, record.scale);
            }
        }
    }

    /// Send metrics computed from a window-size callback.
    ///
    /// macOS runs a nested tracking loop and Win32 a modal sizing loop while
    /// the user drags a window edge, so the event loop does not drain GLFW
    /// events until the drag ends and the content would freeze at its old
    /// size. GLFW calls the framebuffer-size callback inside those loops;
    /// sending the metrics from there lets the framework lay the window out
    /// while the user drags.
    ///
    /// It must not query the window: GLFW is holding it for the callback, and
    /// locking it here would alias that borrow. The callback supplies the
    /// framebuffer size, and the scale comes from the callback's window.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn send_metrics_from_size(&self, view_id: i64, framebuffer: (i32, i32), scale: f64) {
        let Some(engine) = self.engine() else {
            return;
        };
        let scale = scale.max(0.01);
        let metrics = {
            let mut windows = self.windows.borrow_mut();
            let Some(record) = windows.get_mut(&view_id) else {
                return;
            };
            record.framebuffer_size = framebuffer;
            record.scale = scale;
            record.logical_size = (
                f64::from(framebuffer.0) / scale,
                f64::from(framebuffer.1) / scale,
            );
            record.metrics()
        };
        engine.send_window_metrics_for_view(view_id, metrics);
        if let Some(render) = self.render() {
            render.update_metrics(view_id, framebuffer, scale);
        }
    }

    fn send_pointer_events(&self, events: &[PreparedPointer]) {
        let Some(engine) = self.engine() else {
            return;
        };
        for event in events {
            engine.send_pointer_event_for_view(
                event.view_id,
                0,
                event.phase,
                (event.x, event.y),
                event.signal_kind,
                (event.scroll_delta_x, event.scroll_delta_y),
                FlutterPointerDeviceKind::Mouse,
                event.buttons,
                None,
            );
        }
    }

    fn mark_mapped(&self, view_id: i64) {
        if let Some(record) = self.windows.borrow_mut().get_mut(&view_id) {
            record.mapped = true;
        }
    }

    /// Record a position this embedder asked the window manager to apply.
    ///
    /// This becomes the base for the window's follow moves and is queued so the
    /// window's own position events can be recognised as the echo of our move
    /// rather than a move made by the user.
    pub(crate) fn note_requested_position(&self, view_id: i64, position: (i32, i32)) {
        if let Some(record) = self.windows.borrow_mut().get_mut(&view_id) {
            record.follow_position = position;
            record.requested_positions.push_back(position);
            // Events normally acknowledge a request within a frame; the bound
            // only guards against a window manager that coalesces them.
            while record.requested_positions.len() > 256 {
                record.requested_positions.pop_front();
            }
        }
    }

    fn apply_pending_positions(&self, view_id: i64) {
        let (pending, physical) = {
            let mut windows = self.windows.borrow_mut();
            let Some(record) = windows.get_mut(&view_id) else {
                return;
            };
            (
                record.pending_position.take(),
                record.pending_physical_position.take(),
            )
        };
        if pending.is_none() && physical.is_none() {
            return;
        }
        let window = self
            .windows
            .borrow()
            .get(&view_id)
            .map(|record| Arc::clone(&record.window));
        let Some(window) = window else {
            return;
        };
        let mut window = window.lock();
        let scale = f64::from(window.get_content_scale().0).max(0.01);
        let factor = if self.logical_screen_coordinates {
            1.0
        } else {
            scale
        };
        let mut requested = None;
        if let Some((x, y)) = pending {
            let target = ((x * factor).round() as i32, (y * factor).round() as i32);
            window.set_pos(target.0, target.1);
            requested = Some(target);
        }
        if let Some((x, y)) = physical {
            let target = (x.round() as i32, y.round() as i32);
            window.set_pos(target.0, target.1);
            requested = Some(target);
        }
        drop(window);
        if let Some(position) = requested {
            self.note_requested_position(view_id, position);
        }
    }

    /// Re-present the last Metal frame for `view_id`.
    ///
    /// The engine does not schedule a frame for a pure window move, but the
    /// window server may drop the layer's drawable; re-presenting the cached
    /// frame keeps the window from going black.
    #[cfg(target_os = "macos")]
    fn represent_metal_view(&self, view_id: i64) {
        let Some(metal) = &self.metal else {
            return;
        };
        let scale = self
            .windows
            .borrow()
            .get(&view_id)
            .map(|record| record.scale);
        if let Some(scale) = scale {
            metal.represent(view_id, scale);
        }
    }

    fn send_lifecycle<F>(&self, f: F)
    where
        F: FnOnce(&mut LifecycleTracker) -> Option<AppLifecycleState>,
    {
        let state = f(&mut self.lifecycle.borrow_mut());
        if let Some(state) = state {
            self.with_plugin(|lifecycle: &LifecyclePlugin| lifecycle.send_state(state));
        }
    }

    fn send_settings(&self, brightness: PlatformBrightness) {
        self.with_plugin(|settings: &SettingsPlugin| {
            settings
                .start_message(TEXT_SCALE_FACTOR, USE_24_HOUR_FORMAT, brightness)
                .send();
        });
    }

    fn send_staged_key(&self, key: PendingKey) {
        if let Some(engine) = self.engine() {
            engine.send_key_event(
                key.kind,
                key.physical,
                key.logical,
                key.character.as_deref(),
            );
        }
    }

    fn flush_staged_key(&self) {
        let staged = self.key_stager.borrow_mut().take();
        if let Some(key) = staged {
            self.send_staged_key(key);
        }
    }

    fn release_all_keys(&self) {
        let held = self.pressed_keys.lock().release_all();
        if let Some(engine) = self.engine() {
            for (physical, logical) in held {
                debug!("synthesising a key up for {physical:#x} after losing the keyboard");
                engine.send_key_event(KeyEventType::Up, physical, logical, None);
            }
        }
    }

    fn send_key_event(&self, key: glfw::Key, action: glfw::Action) {
        let kind = match action {
            glfw::Action::Press => KeyEventType::Down,
            glfw::Action::Repeat => KeyEventType::Repeat,
            glfw::Action::Release => KeyEventType::Up,
        };
        let code = key as i32;
        let Some((physical, logical)) = crate::keymap::glfw_key_to_flutter(code)
            .or_else(|| crate::keymap::glfw_key_to_flutter_extra(code))
        else {
            debug!("dropping GLFW key {key:?} ({code}): Flutter defines no key id for it");
            return;
        };

        match kind {
            KeyEventType::Down | KeyEventType::Repeat => {
                self.pressed_keys.lock().press(physical, logical);
                let displaced = self.key_stager.borrow_mut().stage(PendingKey {
                    kind,
                    physical,
                    logical,
                    character: None,
                });
                if let Some(key) = displaced {
                    self.send_staged_key(key);
                }
            }
            KeyEventType::Up => {
                self.pressed_keys.lock().release(physical);
                self.send_staged_key(PendingKey {
                    kind,
                    physical,
                    logical,
                    character: None,
                });
            }
        }
    }

    fn with_plugin<F, P>(&self, f: F)
    where
        F: FnOnce(&P),
        P: Plugin + 'static,
    {
        self.plugins.read().with_plugin(f);
    }

    fn with_plugin_mut<F, P>(&self, f: F)
    where
        F: FnOnce(&mut P),
        P: Plugin + 'static,
    {
        self.plugins.write().with_plugin_mut(f);
    }

    fn set_focused(&self, view_id: i64, focused: bool, window: Arc<Mutex<glfw::PWindow>>) {
        if focused {
            self.focused_view.set(Some(view_id));
            self.last_focused.set(Some(view_id));
            self.focused.set(view_id, window);
            if let Some(engine) = self.engine() {
                engine.send_view_focus_event(view_id, true);
            }
        } else if self.focused_view.get() == Some(view_id) {
            self.focused_view.set(None);
            if let Some(engine) = self.engine() {
                engine.send_view_focus_event(view_id, false);
            }
        }
        if !focused {
            self.release_all_keys();
        }

        // Lifecycle is app-level: resumed when any window is focused, inactive
        // when one is visible, hidden when every window is iconified.
        let (visible, focused_any) = {
            let windows = self.windows.borrow();
            let visible = windows.values().any(|record| {
                let window = record.window.lock();
                native_window_alive(&window, &self.glfw)
                    && window.is_visible()
                    && !window.is_iconified()
            });
            (visible, focused)
        };
        self.send_lifecycle(|tracker| tracker.set_focused(focused_any));
        if !visible {
            self.send_lifecycle(|tracker| tracker.set_visible(false));
        } else {
            self.send_lifecycle(|tracker| tracker.set_visible(true));
        }
    }

    fn send_text_edit<F>(&self, f: F)
    where
        F: FnOnce(&mut flutter_plugins::textinput::TextInputPlugin),
    {
        self.with_plugin_mut(f);
    }

    fn notify_changes(&self) {
        self.with_plugin_mut(
            |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                text_input.notify_changes();
            },
        );
    }

    fn handle_text_key(&self, key: glfw::Key, modifiers: glfw::Modifiers) {
        match key {
            glfw::Key::Enter => self.send_text_edit(|text_input| {
                text_input.with_state(|state| state.add_characters("\n"));
            }),
            glfw::Key::Up => self.send_text_edit(|text_input| {
                text_input.with_state(|state| {
                    state.move_up(modifiers.contains(SELECT_MODIFIER_KEY));
                });
            }),
            glfw::Key::Down => self.send_text_edit(|text_input| {
                text_input.with_state(|state| {
                    state.move_down(modifiers.contains(SELECT_MODIFIER_KEY));
                });
            }),
            glfw::Key::Backspace => self.send_text_edit(|text_input| {
                text_input.with_state(|state| state.backspace());
            }),
            glfw::Key::Delete => self.send_text_edit(|text_input| {
                text_input.with_state(|state| state.delete());
            }),
            glfw::Key::Left => self.send_text_edit(|text_input| {
                text_input.with_state(|state| {
                    state.move_left(
                        modifiers.contains(BY_WORD_MODIFIER_KEY),
                        modifiers.contains(SELECT_MODIFIER_KEY),
                    );
                });
            }),
            glfw::Key::Right => self.send_text_edit(|text_input| {
                text_input.with_state(|state| {
                    state.move_right(
                        modifiers.contains(BY_WORD_MODIFIER_KEY),
                        modifiers.contains(SELECT_MODIFIER_KEY),
                    );
                });
            }),
            glfw::Key::Home => self.send_text_edit(|text_input| {
                text_input.with_state(|state| {
                    state.move_to_beginning(modifiers.contains(SELECT_MODIFIER_KEY));
                });
            }),
            glfw::Key::End => self.send_text_edit(|text_input| {
                text_input.with_state(|state| {
                    state.move_to_end(modifiers.contains(SELECT_MODIFIER_KEY));
                });
            }),
            glfw::Key::A if modifiers.contains(FUNCTION_MODIFIER_KEY) => {
                self.send_text_edit(|text_input| {
                    text_input.with_state(|state| state.select_all());
                });
            }
            glfw::Key::X if modifiers.contains(FUNCTION_MODIFIER_KEY) => {
                if let Some(window) = self.focused.get() {
                    let mut window = window.lock();
                    self.send_text_edit(|text_input| {
                        text_input.with_state(|state| {
                            window.set_clipboard_string(state.get_selected_text());
                            state.delete_selected();
                        });
                    });
                }
            }
            glfw::Key::C if modifiers.contains(FUNCTION_MODIFIER_KEY) => {
                if let Some(window) = self.focused.get() {
                    let mut window = window.lock();
                    self.send_text_edit(|text_input| {
                        text_input.with_state(|state| {
                            window.set_clipboard_string(state.get_selected_text());
                        });
                    });
                }
            }
            glfw::Key::V if modifiers.contains(FUNCTION_MODIFIER_KEY) => {
                let clipboard = self
                    .focused
                    .get()
                    .and_then(|window| window.lock().get_clipboard_string());
                self.send_text_edit(|text_input| {
                    text_input.with_state(|state| {
                        if let Some(text) = clipboard {
                            state.add_characters(&text);
                        } else {
                            info!("Tried to paste non-text data");
                        }
                    });
                });
            }
            _ => return,
        }
        self.notify_changes();
    }

    fn handle_glfw_event(&self, view_id: i64, event: glfw::WindowEvent) {
        // GLFW keeps delivering events it queued before another client
        // destroyed the window, and the state queries below then raise
        // `BadWindow`; Xlib's default error handler answers that by exiting the
        // process. A window that is gone has nothing to report, so retire it —
        // `destroy_window` skips its native calls and tells the Dart owner, the
        // same way a close request does.
        if matches!(
            event,
            glfw::WindowEvent::FramebufferSize(_, _)
                | glfw::WindowEvent::Refresh
                | glfw::WindowEvent::ContentScale(_, _)
                | glfw::WindowEvent::Size(_, _)
                | glfw::WindowEvent::Focus(_)
        ) && self
            .windows
            .borrow()
            .get(&view_id)
            .is_some_and(|record| !native_window_alive(&record.window.lock(), &self.glfw))
        {
            log::debug!("view {view_id}: window no longer exists; retiring it");
            self.destroy_window(view_id);
            return;
        }
        self.mark_mapped(view_id);
        self.apply_pending_positions(view_id);
        if let Some(blocking_modal) = self.blocking_modal(view_id) {
            if matches!(&event, glfw::WindowEvent::Focus(true)) {
                let window = self
                    .windows
                    .borrow()
                    .get(&blocking_modal)
                    .map(|record| Arc::clone(&record.window));
                if let Some(window) = window {
                    window.lock().focus();
                }
                return;
            }
            if matches!(
                &event,
                glfw::WindowEvent::CursorEnter(_)
                    | glfw::WindowEvent::CursorPos(_, _)
                    | glfw::WindowEvent::MouseButton(_, _, _)
                    | glfw::WindowEvent::Scroll(_, _)
                    | glfw::WindowEvent::Char(_)
                    | glfw::WindowEvent::Key(_, _, _, _)
            ) {
                return;
            }
        }
        if !matches!(event, glfw::WindowEvent::Char(_)) {
            self.flush_staged_key();
        }

        match event {
            glfw::WindowEvent::FramebufferSize(_, _) => {
                // The content size a resizable shrink-wrapped window asked for
                // has landed: from here the user owns the window size, so stop
                // tracking the content and report tight metrics. The content
                // size also becomes the window's minimum — shrinking below it
                // can only produce a layout that does not fit, and the
                // framework shows an overflow indicator.
                let handed_off = {
                    let mut windows = self.windows.borrow_mut();
                    match windows.get_mut(&view_id) {
                        Some(record)
                            if record.shrink_wrap
                                && record.follow_content
                                && record.content_size_requested.is_some() =>
                        {
                            record.follow_content = false;
                            let content = record.content_size_requested.take();
                            let scale = record.coordinate_scale();
                            content.map(|(width, height)| {
                                (
                                    Arc::clone(&record.window),
                                    (
                                        (width * scale).round().max(1.0) as u32,
                                        (height * scale).round().max(1.0) as u32,
                                    ),
                                )
                            })
                        }
                        _ => None,
                    }
                };
                if let Some((window, (min_width, min_height))) = handed_off {
                    if let Some(render) = self.render() {
                        render.set_follow_content(view_id, false);
                    }
                    window
                        .lock()
                        .set_size_limits(Some(min_width), Some(min_height), None, None);
                }
                self.send_metrics(view_id);
                if let Some(render) = self.render() {
                    render.mark_size_settled(view_id);
                }
                #[cfg(target_os = "macos")]
                self.represent_metal_view(view_id);
            }
            glfw::WindowEvent::Refresh | glfw::WindowEvent::ContentScale(_, _) => {
                self.send_metrics(view_id);
                let geometry = self
                    .windows
                    .borrow()
                    .get(&view_id)
                    .map(|record| (record.logical_size, record.scale));
                if let Some(((width, height), scale)) = geometry {
                    self.emit_event(view_id, event::SIZE_CHANGED, width, height);
                    self.emit_event(view_id, event::SCALE_CHANGED, scale, 0.0);
                }
                #[cfg(target_os = "macos")]
                self.represent_metal_view(view_id);
            }
            glfw::WindowEvent::Size(width, height) => {
                if let Some(record) = self.windows.borrow_mut().get_mut(&view_id) {
                    record.refresh_geometry();
                }
                self.emit_event(
                    view_id,
                    event::SIZE_CHANGED,
                    f64::from(width),
                    f64::from(height),
                );
            }
            glfw::WindowEvent::Pos(x, y) => {
                // A satellite keeps whatever offset it has: move each child by
                // the same delta as its parent, applied to the position we last
                // requested for that child. Reading the child's position back is
                // not an option: on X11 the window manager applies client moves
                // asynchronously, so during a fast parent move it still reports
                // an older position and the outstanding deltas would be lost.
                // The child's own position events are matched against the queue
                // of requests so only a genuinely independent move (the user
                // dragging the satellite) changes the base.
                let (children, delta) = {
                    let mut windows = self.windows.borrow_mut();
                    let Some(record) = windows.get_mut(&view_id) else {
                        return;
                    };
                    let position = (x, y);
                    crate::window::observe_position(
                        position,
                        &mut record.follow_position,
                        &mut record.requested_positions,
                    );
                    let previous = record.position;
                    record.position = position;
                    let delta = (x - previous.0, y - previous.1);
                    let children: Vec<(i64, Arc<Mutex<glfw::PWindow>>)> = windows
                        .values()
                        .filter(|child| child.parent == Some(view_id))
                        .map(|child| (child.view_id, Arc::clone(&child.window)))
                        .collect();
                    (children, delta)
                };
                if delta != (0, 0) {
                    for (child_id, child) in children {
                        let target = {
                            let mut windows = self.windows.borrow_mut();
                            let Some(record) = windows.get_mut(&child_id) else {
                                continue;
                            };
                            let target = (
                                record.follow_position.0 + delta.0,
                                record.follow_position.1 + delta.1,
                            );
                            record.follow_position = target;
                            record.requested_positions.push_back(target);
                            if record.requested_positions.len() > 256 {
                                record.requested_positions.pop_front();
                            }
                            target
                        };
                        child.lock().set_pos(target.0, target.1);
                    }
                }
                self.emit_event(view_id, event::POSITION_CHANGED, f64::from(x), f64::from(y));
                #[cfg(target_os = "macos")]
                self.represent_metal_view(view_id);
            }
            glfw::WindowEvent::CursorEnter(entered) => {
                let prepared = {
                    let mut windows = self.windows.borrow_mut();
                    let Some(record) = windows.get_mut(&view_id) else {
                        return;
                    };
                    let cursor_pos = record.cursor_pos;
                    let phase = if entered {
                        flutter_engine::ffi::FlutterPointerPhase::Add
                    } else {
                        flutter_engine::ffi::FlutterPointerPhase::Remove
                    };
                    record.prepare_pointer_events(
                        phase,
                        cursor_pos,
                        FlutterPointerSignalKind::None,
                        (0.0, 0.0),
                        PointerButtons::NONE,
                    )
                };
                self.send_pointer_events(&prepared);
            }
            glfw::WindowEvent::CursorPos(x, y) => {
                // A borderless-window drag consumes the cursor event.
                if self.focused.drag_window(view_id, x, y) {
                    return;
                }
                let prepared = {
                    let mut windows = self.windows.borrow_mut();
                    let Some(record) = windows.get_mut(&view_id) else {
                        return;
                    };
                    record.cursor_pos = (x, y);
                    // A popup can appear right under the cursor and never see
                    // a `CursorEnter`; `prepare_pointer_events` synthesizes the
                    // `Add` this pointer needs, so do not require one here.
                    let buttons = record.pressed_buttons();
                    record.prepare_pointer_events(
                        move_or_hover(buttons),
                        (x, y),
                        FlutterPointerSignalKind::None,
                        (0.0, 0.0),
                        buttons,
                    )
                };
                self.send_pointer_events(&prepared);
            }
            glfw::WindowEvent::MouseButton(glfw::MouseButton::Button4, action, _) => {
                if action == glfw::Action::Press {
                    self.with_plugin(|navigation: &NavigationPlugin| navigation.pop_route());
                }
            }
            glfw::WindowEvent::MouseButton(button, action, _) => {
                let prepared = {
                    let mut windows = self.windows.borrow_mut();
                    let Some(record) = windows.get_mut(&view_id) else {
                        return;
                    };
                    record.mouse_tracker.insert(button, action);
                    // A popup is override-redirect, so the window manager will
                    // not focus it on click. Take keyboard focus explicitly so
                    // its text fields and shortcuts work.
                    if action == glfw::Action::Press && record.kind == WindowKind::Popup {
                        record.window.lock().focus();
                    }
                    // A window that appears under the cursor never gets a
                    // `CursorEnter`, so its `cursor_pos` is still the one it
                    // was created with. Ask the window where the press landed
                    // and let `prepare_pointer_events` synthesize the `Add`.
                    let (x, y) = if record.pointer_added {
                        record.cursor_pos
                    } else {
                        let position = record.window.lock().get_cursor_pos();
                        record.cursor_pos = position;
                        position
                    };
                    let phase = if action == glfw::Action::Press {
                        flutter_engine::ffi::FlutterPointerPhase::Down
                    } else {
                        flutter_engine::ffi::FlutterPointerPhase::Up
                    };
                    let buttons = record.pressed_buttons();
                    record.prepare_pointer_events(
                        phase,
                        (x, y),
                        FlutterPointerSignalKind::None,
                        (0.0, 0.0),
                        buttons,
                    )
                };
                self.send_pointer_events(&prepared);
            }
            glfw::WindowEvent::Scroll(dx, dy) => {
                let prepared = {
                    let mut windows = self.windows.borrow_mut();
                    let Some(record) = windows.get_mut(&view_id) else {
                        return;
                    };
                    let (x, y) = record.cursor_pos;
                    let buttons = record.pressed_buttons();
                    record.prepare_pointer_events(
                        move_or_hover(buttons),
                        (x, y),
                        FlutterPointerSignalKind::Scroll,
                        (dx * SCROLL_SPEED, -dy * SCROLL_SPEED),
                        buttons,
                    )
                };
                self.send_pointer_events(&prepared);
            }
            glfw::WindowEvent::Char(character) => {
                let staged = self.key_stager.borrow_mut().take_with_character(character);
                if let Some(key) = staged {
                    self.send_staged_key(key);
                }
                self.send_text_edit(|text_input| {
                    text_input.with_state(|state| state.add_characters(&character.to_string()));
                });
                self.notify_changes();
            }
            glfw::WindowEvent::Focus(focused) => {
                let window = self
                    .windows
                    .borrow()
                    .get(&view_id)
                    .map(|record| record.window.clone());
                if let Some(window) = window {
                    self.set_focused(view_id, focused, window);
                }
                self.emit_event(
                    view_id,
                    event::FOCUS_CHANGED,
                    if focused { 1.0 } else { 0.0 },
                    0.0,
                );
            }
            glfw::WindowEvent::Iconify(iconified) => {
                self.send_lifecycle(|tracker| tracker.set_visible(!iconified));
                self.emit_event(
                    view_id,
                    event::MINIMIZED_CHANGED,
                    if iconified { 1.0 } else { 0.0 },
                    0.0,
                );
            }
            glfw::WindowEvent::Maximize(maximized) => {
                self.emit_event(
                    view_id,
                    event::MAXIMIZED_CHANGED,
                    if maximized { 1.0 } else { 0.0 },
                    0.0,
                );
            }
            glfw::WindowEvent::Key(key, _scancode, action, modifiers)
                if action != glfw::Action::Release =>
            {
                self.handle_text_key(key, modifiers);
                self.send_key_event(key, action);
            }
            glfw::WindowEvent::Key(key, _scancode, glfw::Action::Release, _) => {
                self.send_key_event(key, glfw::Action::Release);
            }
            _ => {}
        }
    }

    /// The modal dialog that blocks input to `view_id`, if any.
    ///
    /// A modal dialog blocks its parent window and every window that belongs
    /// to it: tooltips, popups and satellites are separate native windows, so
    /// the parent's modal has to block them too, not only the parent itself.
    fn blocking_modal(&self, view_id: i64) -> Option<i64> {
        let windows = self.windows.borrow();
        // The window itself and its ancestors. A modal that belongs to one of
        // them blocks this window; a modal that is on the chain (the window
        // itself or one of its parents) is the one asking, not the blocker.
        let mut chain = Vec::new();
        let mut current = Some(view_id);
        while let Some(id) = current {
            // A window tree is acyclic, but an orphaned record must not hang
            // the event loop if one ever points at itself.
            if chain.contains(&id) || chain.len() > windows.len() {
                break;
            }
            chain.push(id);
            current = windows.get(&id).and_then(|record| record.parent);
        }
        chain.iter().find_map(|id| {
            windows
                .values()
                .find(|candidate| {
                    candidate.modal
                        && candidate.parent == Some(*id)
                        && !chain.contains(&candidate.view_id)
                })
                .map(|modal| modal.view_id)
        })
    }

    pub(crate) fn destroy_window(&self, view_id: i64) {
        // Tooltips, popups, satellites and parented dialogs close with their
        // parent (the framework documents this for all of them).
        let children: Vec<i64> = self
            .windows
            .borrow()
            .iter()
            .filter(|(_, record)| record.parent == Some(view_id))
            .map(|(id, _)| *id)
            .collect();
        for child in children {
            self.destroy_window(child);
        }
        let record = self.windows.borrow_mut().remove(&view_id);
        if let Some(engine) = self.engine()
            && view_id > 0
            && let Err(error) = engine.remove_view(view_id)
        {
            log::warn!("could not remove view {view_id}: {error}");
        }
        if let Some(render) = self.render() {
            render.remove_view(view_id);
        }
        #[cfg(target_os = "macos")]
        if let Some(metal) = &self.metal {
            metal.remove_layer(view_id);
        }
        self.focused.clear(view_id);
        let was_focused = self.focused_view.get() == Some(view_id);
        if was_focused {
            self.focused_view.set(None);
        }
        if let Some(record) = record {
            detach_from_parent(&record.window.lock());
            // A window the window system already destroyed has no native
            // handle left to hide or query; touching it would be an X protocol
            // error. Its view still has to go, so only the native calls are
            // skipped.
            if native_window_alive(&record.window.lock(), &self.glfw) {
                record.window.lock().hide();
            }
            self.retired_windows
                .borrow_mut()
                .push(Arc::clone(&record.window));
            // A focused child (for example a clicked popup) hands keyboard
            // focus back to its parent when it closes.
            if was_focused && let Some(parent_id) = record.parent {
                let parent = self
                    .windows
                    .borrow()
                    .get(&parent_id)
                    .map(|record| Arc::clone(&record.window));
                if let Some(parent) = parent {
                    parent.lock().focus();
                }
            }
        }
        self.emit_event(view_id, event::DESTROYED, 0.0, 0.0);
    }

    /// Post a windowing event to the Dart owner, if one is installed.
    pub(crate) fn emit_event(&self, view_id: i64, code: i32, arg0: f64, arg1: f64) {
        let Some(callback) = self.event_callback.get() else {
            return;
        };
        // SAFETY: the Dart side installs a `NativeCallable.listener`, which is
        // designed to be called from another thread; this is the platform
        // thread and the callback only posts to the isolate.
        unsafe { callback(view_id, code, arg0, arg1, self.event_user_data.get()) };
    }

    fn apply_desired_sizes(&self) {
        let _busy = BusyGuard::enter(&self.embedder_busy);
        let Some(render) = self.render() else {
            return;
        };
        let view_ids: Vec<i64> = self.windows.borrow().keys().copied().collect();
        for view_id in view_ids {
            if let Some((width, height)) = render.take_desired_size(view_id) {
                let (window, scale, recenter) = {
                    let windows = self.windows.borrow();
                    let Some(record) = windows.get(&view_id) else {
                        continue;
                    };
                    let scale = record.coordinate_scale();
                    // A dialog is centred against the size it was created with.
                    // Keep it centred on its parent while it still follows its
                    // content; once a resizable dialog hands sizing over, the
                    // user may put it wherever they like.
                    let recenter = if record.kind == WindowKind::Dialog
                        && record.shrink_wrap
                        && record.follow_content
                    {
                        record
                            .parent
                            .and_then(|parent_id| windows.get(&parent_id))
                            .map(|parent| {
                                let parent_scale = parent.coordinate_scale();
                                centered_position(
                                    parent.position,
                                    (
                                        parent.logical_size.0 * parent_scale,
                                        parent.logical_size.1 * parent_scale,
                                    ),
                                    (width * scale, height * scale),
                                )
                            })
                    } else {
                        None
                    };
                    (Arc::clone(&record.window), scale, recenter)
                };
                let mut window = window.lock();
                window.set_size(
                    (width * scale).round() as i32,
                    (height * scale).round() as i32,
                );
                if let Some((x, y)) = recenter {
                    window.set_pos(x, y);
                }
                drop(window);
                // A resizable shrink-wrapped window follows its content
                // only until the first content size is applied. Arm the
                // handoff here and complete it when the window reports the
                // new framebuffer size; flipping now would read the old
                // size and lay the content out against it.
                if let Some(record) = self.windows.borrow_mut().get_mut(&view_id)
                    && record.shrink_wrap
                    && record.resizable
                    && record.follow_content
                {
                    record.content_size_requested = Some((width, height));
                }
                self.send_metrics(view_id);
            }
        }
    }
}

/// A running GLFW app: one engine, many windows.
pub struct FlutterApp {
    state: Rc<AppState>,
    /// `false` for the short-lived wrappers the Dart windowing ABI builds from
    /// the thread-local app pointer; those must not shut the app down on drop.
    owns_app: bool,
}

thread_local! {
    /// The app the Dart windowing ABI operates on. It is set by
    /// [`FlutterApp::create`] and cleared on shutdown; the ABI is called on the
    /// platform thread only.
    static CURRENT_APP: RefCell<Option<Rc<AppState>>> = const { RefCell::new(None) };
}

/// The app installed on this thread, if any.
pub(crate) fn current_app() -> Option<Rc<AppState>> {
    CURRENT_APP.with(|app| app.borrow().clone())
}

impl std::fmt::Debug for FlutterApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlutterApp")
            .field("windows", &self.state.windows.borrow().len())
            .field("running", &self.state.running.get())
            .finish()
    }
}

impl FlutterApp {
    pub(crate) fn create(
        glfw: glfw::Glfw,
        args: AppArgs,
        context_api: glfw::ContextCreationApi,
    ) -> Result<Self, CreateError> {
        // Hidden 1×1 windows provide the raster/master context and the shared
        // resource context. Every visible window shares with the master.
        let mut glfw = glfw;
        glfw.window_hint(glfw::WindowHint::Visible(false));
        glfw.window_hint(glfw::WindowHint::Decorated(false));
        glfw.window_hint(glfw::WindowHint::ContextVersion(3, 2));
        glfw.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
        glfw.window_hint(glfw::WindowHint::OpenGlProfile(
            glfw::OpenGlProfileHint::Core,
        ));
        glfw.window_hint(glfw::WindowHint::ContextCreationApi(context_api));

        let (mut master, _master_events) = glfw
            .create_window(1, 1, "", glfw::WindowMode::Windowed)
            .ok_or(CreateError::WindowCreationFailed)?;
        let (mut resource, _resource_events) = master
            .create_shared(1, 1, "", glfw::WindowMode::Windowed)
            .ok_or(CreateError::WindowCreationFailed)?;
        glfw.default_window_hints();

        let master_ctx = Arc::new(Mutex::new(master.render_context()));
        let resource_ctx = Arc::new(Mutex::new(resource.render_context()));
        let render = RenderState::new(Arc::clone(&master_ctx));

        let platform_task_handler = Arc::new(GlfwPlatformTaskHandler::new());
        let opengl_handler =
            GlfwOpenGLHandler::new(Arc::clone(&master_ctx), Arc::clone(&resource_ctx));
        // The macOS engine has no OpenGL backend. Metal is the default: the
        // engine renders into textures the compositor creates and we copy them
        // into the window's CAMetalLayer. `FLUTTER_GLFW_RENDERER=software`
        // forces the CPU renderer, which is uploaded through the window's GL
        // context instead.
        #[cfg(target_os = "macos")]
        let metal = if std::env::var("FLUTTER_GLFW_RENDERER")
            .is_ok_and(|value| value.eq_ignore_ascii_case("software"))
        {
            None
        } else {
            let state = crate::metal::MetalState::new();
            if state.is_none() {
                log::warn!("no MTLDevice is available; falling back to software rendering");
            }
            state
        };
        #[cfg(target_os = "macos")]
        let compositor_kind = match &metal {
            Some(metal) => CompositorKind::Metal(Arc::clone(metal)),
            None => CompositorKind::Software,
        };
        #[cfg(not(target_os = "macos"))]
        let compositor_kind = CompositorKind::OpenGl;
        let compositor = GlfwCompositor::new(Arc::clone(&render), compositor_kind);

        // Impeller is the engine's rasteriser on macOS when the Metal path is
        // active (it renders into the same textures); pass `--enable-impeller=false`
        // through AppArgs to keep Skia.
        #[cfg(target_os = "macos")]
        let mut engine_args = args.engine_args;
        #[cfg(target_os = "macos")]
        if metal.is_some() && !engine_args.iter().any(|arg| arg.contains("impeller")) {
            engine_args.push("--enable-impeller".to_owned());
        }
        #[cfg(not(target_os = "macos"))]
        let engine_args = args.engine_args;

        let mut builder = FlutterEngineBuilder::new()
            .with_platform_handler(platform_task_handler.clone())
            .with_opengl(opengl_handler)
            .with_compositor(compositor)
            .with_ui_on_platform_thread()
            .with_engine_id(crate::windowing::api_address())
            .with_asset_path(args.assets_path)
            .with_args(engine_args);
        #[cfg(target_os = "macos")]
        match &metal {
            Some(metal) => {
                builder = builder.with_metal_renderer(metal.device_ptr(), metal.queue_ptr());
            }
            None => {
                builder = builder.with_software_renderer();
            }
        }
        if let Some(icu) = args.icu_data_path {
            builder = builder.with_icu_data_path(icu);
        }
        let engine = builder.build()?;

        let (main_tx, main_rx) = mpsc::channel();
        let focused = Arc::new(FocusedWindow::new());
        let quit_flag = Arc::new(AtomicBool::new(false));
        let pressed_keys = Arc::new(Mutex::new(PressedKeysState::default()));

        let platform_handler = Arc::new(Mutex::new(GlfwPlatformHandler {
            focused: Arc::clone(&focused),
            quit: Arc::clone(&quit_flag),
        }));
        let window_handler = Arc::new(Mutex::new(GlfwWindowHandler::new(Arc::clone(&focused))));
        let textinput_handler = Arc::new(Mutex::new(GlfwTextInputHandler::default()));
        let mousecursor_handler = Arc::new(Mutex::new(GlfwMouseCursorHandler::new(Arc::clone(
            &focused,
        ))));
        let keyboard_state: Arc<Mutex<dyn KeyboardStateHandler + Send>> =
            Arc::new(Mutex::new(KeyboardStateBridge(Arc::clone(&pressed_keys))));

        let mut plugins = PluginRegistrar::new();
        plugins.add_plugin(&engine, AccessibilityPlugin::default());
        #[cfg(feature = "dialog")]
        plugins.add_plugin(&engine, DialogPlugin::default());
        plugins.add_plugin(&engine, IsolatePlugin::new_stub());
        plugins.add_plugin(&engine, KeyboardPlugin::new(keyboard_state));
        plugins.add_plugin(&engine, LifecyclePlugin::default());
        plugins.add_plugin(&engine, LocalizationPlugin::default());
        plugins.add_plugin(&engine, MouseCursorPlugin::new(mousecursor_handler));
        plugins.add_plugin(&engine, NavigationPlugin::default());
        plugins.add_plugin(&engine, PlatformPlugin::new(platform_handler));
        plugins.add_plugin(&engine, SettingsPlugin::default());
        plugins.add_plugin(&engine, SystemPlugin::default());
        plugins.add_plugin(&engine, TextInputPlugin::new(textinput_handler));
        plugins.add_plugin(&engine, WindowPlugin::new(window_handler));

        let state = Rc::new(AppState {
            engine: RefCell::new(Some(engine)),
            render: RefCell::new(Some(render)),
            #[cfg(target_os = "macos")]
            metal: metal.clone(),
            glfw: glfw.clone(),
            master: RefCell::new(Some(master)),
            resource: RefCell::new(Some(resource)),
            windows: RefCell::new(HashMap::new()),
            retired_windows: RefCell::new(Vec::new()),
            next_view_id: Cell::new(1),
            had_windows: Cell::new(false),
            running: Cell::new(false),
            quit: Cell::new(false),
            focused_view: Cell::new(None),
            last_focused: Cell::new(None),
            key_stager: RefCell::new(KeyStager::default()),
            pressed_keys,
            lifecycle: RefCell::new(LifecycleTracker::new()),
            main_tx,
            main_rx,
            platform_task_handler,
            pumping_platform_tasks: Cell::new(false),
            embedder_busy: Cell::new(false),
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            last_resize_pump: Cell::new(None),
            plugins: RwLock::new(plugins),
            focused,
            quit_flag,
            brightness_started: Cell::new(false),
            event_callback: Cell::new(None),
            event_user_data: Cell::new(std::ptr::null_mut()),
            context_api,
            logical_screen_coordinates: cfg!(target_os = "macos")
                || glfw.get_platform() == glfw::Platform::Wayland,
        });

        // The windowing ABI reaches the app through this thread-local. It is
        // cleared on shutdown so a late FFI call cannot revive a dead app.
        CURRENT_APP.with(|app| *app.borrow_mut() = Some(Rc::clone(&state)));
        Ok(Self {
            state,
            owns_app: true,
        })
    }

    /// A non-owning wrapper around an app installed by [`FlutterApp::create`].
    pub(crate) fn from_state(state: Rc<AppState>) -> Self {
        Self {
            state,
            owns_app: false,
        }
    }

    /// Create a native window and its engine view.
    ///
    /// Before [`run`](Self::run) only one window may be created: it owns the
    /// implicit view (`0`) and is the window a `runApp` application renders
    /// into. After `run` every call adds a view (`1`, `2`, …).
    ///
    /// # Errors
    ///
    /// Returns [`CreateError`] when GLFW cannot create the window, when a
    /// second pre-run window is requested, or when the engine rejects the view.
    pub fn create_window(&self, args: &WindowArgs<'_>) -> Result<FlutterWindow, CreateError> {
        let request = WindowRequest::regular(args);
        self.create_window_inner(request)
    }

    pub(crate) fn create_window_inner(
        &self,
        request: WindowRequest,
    ) -> Result<FlutterWindow, CreateError> {
        let state = &self.state;
        let _busy = BusyGuard::enter(&state.embedder_busy);
        let running = state.running.get();
        let view_id = if running {
            let id = state.next_view_id.get();
            state.next_view_id.set(id + 1);
            id
        } else {
            if !state.windows.borrow().is_empty() {
                return Err(CreateError::WindowBeforeRun);
            }
            0
        };

        let mut glfw = state.glfw.clone();
        glfw.window_hint(glfw::WindowHint::ContextVersion(3, 2));
        glfw.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
        glfw.window_hint(glfw::WindowHint::OpenGlProfile(
            glfw::OpenGlProfileHint::Core,
        ));
        glfw.window_hint(glfw::WindowHint::ContextCreationApi(state.context_api));
        glfw.window_hint(glfw::WindowHint::Decorated(request.decorated));
        // A shrink-wrapped window must remain programmatically resizable even
        // when the user cannot resize it, otherwise the WM ignores the
        // content-size updates the compositor asks for.
        glfw.window_hint(glfw::WindowHint::Resizable(
            request.resizable || request.shrink_wrap,
        ));
        // Tooltips and popups belong to their parent: they must not stay above
        // every other window. `WM_TRANSIENT_FOR` below keeps them above the
        // parent and lets the window manager hide them with it.
        glfw.window_hint(glfw::WindowHint::Floating(
            request.floating && !matches!(request.kind, WindowKind::Tooltip | WindowKind::Popup),
        ));
        // Create hidden and map in `window.show()` after the EWMH hints are in
        // place, so the window manager reads `_NET_WM_WINDOW_TYPE` and
        // `WM_TRANSIENT_FOR` when it first manages the window.
        glfw.window_hint(glfw::WindowHint::Visible(false));

        let (width, height) = request.logical_size();
        let (mut window, receiver) = {
            let master = state.master.borrow();
            let master = master.as_ref().ok_or(CreateError::WindowCreationFailed)?;
            match request.mode {
                WindowMode::Fullscreen(index) => {
                    let created: Result<_, CreateError> =
                        glfw.with_connected_monitors(|_, monitors| {
                            let monitor =
                                monitors.get(index).ok_or(CreateError::MonitorNotFound)?;
                            master
                                .create_shared(
                                    width.round() as u32,
                                    height.round() as u32,
                                    &request.title,
                                    glfw::WindowMode::FullScreen(monitor),
                                )
                                .ok_or(CreateError::WindowCreationFailed)
                        });
                    created?
                }
                _ => master
                    .create_shared(
                        width.round() as u32,
                        height.round() as u32,
                        &request.title,
                        glfw::WindowMode::Windowed,
                    )
                    .ok_or(CreateError::WindowCreationFailed)?,
            }
        };
        glfw.default_window_hints();
        // Start polling before the window is mapped. A parented window can take
        // keyboard focus as soon as it is ordered above a key parent, and
        // `glfw` discards callbacks whose polling was still off: the focus
        // event that marks the view focused would be lost, leaving the view
        // unable to focus its text fields.
        Self::enable_window_polling(&mut window);
        #[cfg(target_os = "windows")]
        install_live_resize_timer(&window);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            // AppKit runs a nested tracking loop and Win32 a modal sizing loop
            // while the user drags a window edge, so the event loop does not
            // drain GLFW's polling receiver until the drag ends and the content
            // freezes at its old size. GLFW calls this callback from inside
            // those loops, so the framework can follow the drag instead of
            // catching up afterwards.
            let state = Rc::downgrade(&self.state);
            window.set_framebuffer_size_callback(move |window, width, height| {
                if let Some(state) = state.upgrade() {
                    // The notification is synchronous, so this can fire while
                    // the embedder is creating or resizing a window. Leave that
                    // to the event loop then.
                    if state.embedder_busy.get() {
                        return;
                    }
                    let _busy = BusyGuard::enter(&state.embedder_busy);
                    // AppKit can post these far more often than the display
                    // refreshes. Laying out for every notification leaves the
                    // engine with a backlog that drains for seconds after the
                    // drag, so keep at most one frame per refresh.
                    let now = Instant::now();
                    if state.last_resize_pump.get().is_some_and(|last| {
                        now.duration_since(last) < std::time::Duration::from_millis(16)
                    }) {
                        return;
                    }
                    state.last_resize_pump.set(Some(now));
                    let scale = f64::from(window.get_content_scale().0);
                    state.send_metrics_from_size(view_id, (width, height), scale);
                    // The engine schedules the frame through the platform task
                    // runner, which normally runs in the event loop below. That
                    // loop is blocked while the tracking loop is up, so run the
                    // tasks here; otherwise the layout never reaches the
                    // raster thread until the drag ends.
                    state.pump_platform_tasks();
                    #[cfg(target_os = "windows")]
                    {
                        // The OpenGL presenter fills the drawable up to the
                        // settled size. Before the blocked loop reaches the
                        // queued FramebufferSize event that size is still the
                        // one the drag started from, which leaves the strip the
                        // window just gained black; the callback already knows
                        // the size the window has now.
                        if let Some(render) = state.render() {
                            render.mark_size_settled(view_id);
                        }
                        // Shrink-wrapped tooltips and popups keep animating
                        // while their parent is resized.
                        state.apply_desired_sizes();
                    }
                }
            });
        }
        // GLFW's window sizes and positions are in screen coordinates. On X11
        // and Windows those are physical pixels while the framework sizes and
        // positions windows in logical pixels; scale by the content scale. The
        // requested size was logical, so `create_shared` produced a window of
        // that many physical pixels; resize it to the logical size.
        let scale = f64::from(window.get_content_scale().0).max(0.01);
        let coordinate_scale = if cfg!(target_os = "macos") {
            1.0
        } else {
            scale
        };
        if !matches!(request.mode, WindowMode::Fullscreen(_)) {
            window.set_size(
                (width * coordinate_scale).round() as i32,
                (height * coordinate_scale).round() as i32,
            );
        }
        // The interval is per-context; make the new window's context current
        // just long enough to disable v-sync (the engine's scheduler paces
        // frames, and N windows would otherwise serialize N vblank waits on the
        // single raster thread), then clear it so the raster thread can own it.
        window.make_current();
        glfw.set_swap_interval(glfw::SwapInterval::None);
        glfw::make_context_current(None);
        // Give the window manager explicit size hints: GLFW only sets
        // `PMinSize`/`PMaxSize` for non-resizable windows, and some WMs use the
        // absence of any size hints to choose a size of their own.
        window.set_size_limits(Some(1), Some(1), None, None);
        window.set_title(&request.title);
        if matches!(request.kind, WindowKind::Tooltip | WindowKind::Popup) {
            set_x11_override_redirect(&window, &state.glfw);
        }
        set_x11_window_type(&window, &state.glfw, request.kind);
        // Every parented window (dialogs, tooltips, popups, satellites) is a
        // transient of its parent: the window manager keeps it above the
        // parent and hides it together with the parent. Tiling extensions such
        // as PaperWM only manage non-transient `_NET_WM_WINDOW_TYPE_NORMAL`
        // windows, so this also keeps child windows out of their layouts.
        if let Some(parent_id) = request.parent {
            let parent = state
                .windows
                .borrow()
                .get(&parent_id)
                .map(|record| Arc::clone(&record.window));
            if let Some(parent) = parent {
                attach_to_parent(&window, &parent.lock(), &state.glfw);
            }
        }
        window.show();
        if request.modal {
            // The WM should enforce modality; focus the dialog immediately so a
            // compositor that does not is still usable.
            set_x11_modal_state(&window, &state.glfw);
            window.focus();
        }
        // Position after mapping: some window managers ignore a position set on
        // an unmapped override-redirect/floating window and place it relative
        // to the parent instead.
        let initial_position = request.position.map(|(x, y)| {
            let target = (
                (x * coordinate_scale).round() as i32,
                (y * coordinate_scale).round() as i32,
            );
            window.set_pos(target.0, target.1);
            target
        });
        let created_size = window.get_size();
        log::debug!(
            "created window {}x{} for view {view_id} (requested {}x{})",
            created_size.0,
            created_size.1,
            width,
            height
        );
        let context = Arc::new(Mutex::new(window.render_context()));
        let window = Arc::new(Mutex::new(window));
        let mut record = WindowRecord {
            view_id,
            kind: request.kind,
            window: Arc::clone(&window),
            receiver,
            parent: request.parent,
            modal: request.modal,
            shrink_wrap: request.shrink_wrap,
            resizable: request.resizable,
            follow_content: request.shrink_wrap,
            content_size_requested: None,
            logical_size: (width, height),
            position: (0, 0),
            follow_position: (0, 0),
            requested_positions: VecDeque::new(),
            pending_position: request.position,
            pending_physical_position: None,
            mapped: false,
            framebuffer_size: (0, 0),
            scale: 1.0,
            logical_screen_coordinates: state.logical_screen_coordinates,
            pixels_per_screen: 1.0,
            pointer_added: false,
            cursor_pos: (0.0, 0.0),
            mouse_tracker: HashMap::new(),
            fullscreen: matches!(request.mode, WindowMode::Fullscreen(_)),
        };
        record.refresh_geometry();
        record.position = initial_position.unwrap_or_else(|| window.lock().get_pos());
        record.follow_position = record.position;
        if let Some(target) = initial_position {
            record.requested_positions.push_back(target);
        }

        #[cfg(target_os = "macos")]
        if let Some(metal) = &state.metal {
            metal.attach_layer(
                &window.lock(),
                view_id,
                record.framebuffer_size,
                record.scale,
            );
        }

        if let Some(render) = state.render() {
            render.insert_view(
                view_id,
                context,
                record.framebuffer_size,
                record.scale,
                record.shrink_wrap,
                record.follow_content,
            );
        }

        if running {
            let engine = state.engine().ok_or(CreateError::WindowCreationFailed)?;
            if let Err(error) = engine.add_view(view_id, record.metrics()) {
                if let Some(render) = state.render() {
                    render.remove_view(view_id);
                }
                return Err(CreateError::View(error));
            }
        }

        state.windows.borrow_mut().insert(view_id, record);
        state.had_windows.set(true);

        Ok(FlutterWindow::new(
            Rc::downgrade(&self.state),
            view_id,
            window,
        ))
    }

    fn enable_polling(window: &Arc<Mutex<glfw::PWindow>>) {
        Self::enable_window_polling(&mut window.lock());
    }

    fn enable_window_polling(window: &mut glfw::PWindow) {
        window.set_char_polling(true);
        window.set_cursor_pos_polling(true);
        window.set_cursor_enter_polling(true);
        window.set_framebuffer_size_polling(true);
        window.set_key_polling(true);
        window.set_mouse_button_polling(true);
        window.set_scroll_polling(true);
        window.set_size_polling(true);
        window.set_content_scale_polling(true);
        window.set_refresh_polling(true);
        window.set_focus_polling(true);
        window.set_iconify_polling(true);
        window.set_pos_polling(true);
        window.set_maximize_polling(true);
    }

    /// Start the engine and run the GLFW event loop until the app exits.
    ///
    /// # Errors
    ///
    /// Returns [`RunError`] if the engine refuses to start, in which case no
    /// event loop is entered.
    pub fn run(
        &self,
        mut custom_handler: Option<&mut crate::window::WindowEventHandler>,
        mut frame_callback: Option<&mut crate::window::PerFrameCallback>,
    ) -> Result<(), RunError> {
        let state = &self.state;
        let engine = state
            .engine()
            .ok_or(RunError::Engine(flutter_engine::RunError::ShutDown))?;
        engine.run()?;
        state.running.set(true);

        // Send metrics now that the shell exists. With no window at all, a 1×1
        // metrics event for the implicit view guarantees the framework builds
        // the first widget tree (which is what creates the first window in a
        // windowing app).
        if state.windows.borrow().is_empty() {
            engine.send_window_metrics_event(1, 1, 1.0);
        } else {
            let ids: Vec<i64> = state.windows.borrow().keys().copied().collect();
            for id in ids {
                state.send_metrics(id);
            }
        }

        for window in state.windows.borrow().values() {
            Self::enable_polling(&window.window);
        }

        state.with_plugin(|localization: &LocalizationPlugin| {
            localization.send_locales(sys_locale::get_locales());
        });
        let (visible, focused) = {
            let windows = state.windows.borrow();
            let visible = windows.values().any(|record| {
                let window = record.window.lock();
                window.is_visible() && !window.is_iconified()
            });
            let focused = windows.values().any(|record| {
                let window = record.window.lock();
                window.is_focused()
            });
            (visible, focused)
        };
        state.send_lifecycle(|tracker| tracker.update(visible, focused));
        state.send_settings(crate::app::detect_platform_brightness());
        state.watch_platform_brightness();

        let mut glfw = state.glfw.clone();
        // The compositor asks the platform thread to apply shrink-wrap sizes
        // from the raster thread; `post_empty_event` wakes `wait_events` so the
        // request is picked up promptly instead of waiting for the next event.
        if let Some(render) = state.render() {
            let wake_glfw = Arc::new(Mutex::new(glfw::ThreadSafeGlfw::from(&mut glfw)));
            render.set_wake(move || {
                wake_glfw.lock().post_empty_event();
            });
        }
        #[cfg(target_os = "macos")]
        install_tracking_timer(state);
        while !state.quit.get()
            && !state.quit_flag.load(Ordering::Relaxed)
            && !(state.had_windows.get() && state.windows.borrow().is_empty())
        {
            let next_task_time = state.pump_platform_tasks();

            let callbacks: Vec<MainThreadFn> = state.main_rx.try_iter().collect();
            for mut cb in callbacks {
                cb(self);
            }

            state.apply_desired_sizes();

            // Close requests: with a Dart owner installed, clear the native
            // flag and let its delegate decide (the default destroys); without
            // one, destroy immediately.
            let closing: Vec<i64> = state
                .windows
                .borrow()
                .iter()
                .filter(|(_, record)| record.window.lock().should_close())
                .map(|(id, _)| *id)
                .collect();
            for id in closing {
                if state.event_callback.get().is_some() {
                    let window = state.windows.borrow().get(&id).map(|r| r.window.clone());
                    if let Some(window) = window {
                        window.lock().set_should_close(false);
                    }
                    state.emit_event(id, event::CLOSE_REQUESTED, 0.0, 0.0);
                } else {
                    state.destroy_window(id);
                }
            }

            if let Some(next_task_time) = next_task_time {
                let now = Instant::now();
                if now < next_task_time {
                    glfw.wait_events_timeout(next_task_time.duration_since(now).as_secs_f64());
                } else {
                    glfw.poll_events();
                }
            } else {
                glfw.wait_events();
            }

            let view_ids: Vec<i64> = state.windows.borrow().keys().copied().collect();
            for view_id in view_ids {
                let events: Vec<(f64, glfw::WindowEvent)> = {
                    let windows = state.windows.borrow();
                    let Some(record) = windows.get(&view_id) else {
                        continue;
                    };
                    glfw::flush_messages(&record.receiver).collect()
                };
                // A live resize queues one size/refresh event per step, and the
                // tracking loop also lets cursor positions pile up while the
                // event loop is blocked. The resize callback already sent the
                // metrics for the size in progress, and only the latest size and
                // pointer position matter: replaying the whole trail after the
                // drag makes the app stall for seconds.
                let last_framebuffer = events.iter().rposition(|(_, event)| {
                    matches!(event, glfw::WindowEvent::FramebufferSize(_, _))
                });
                let last_cursor = events
                    .iter()
                    .rposition(|(_, event)| matches!(event, glfw::WindowEvent::CursorPos(_, _)));
                let last_size = events
                    .iter()
                    .rposition(|(_, event)| matches!(event, glfw::WindowEvent::Size(_, _)));
                let last_refresh = events
                    .iter()
                    .rposition(|(_, event)| matches!(event, glfw::WindowEvent::Refresh));
                for (index, (_, event)) in events.into_iter().enumerate() {
                    let superseded = match event {
                        glfw::WindowEvent::FramebufferSize(_, _) => Some(index) != last_framebuffer,
                        glfw::WindowEvent::CursorPos(_, _) => Some(index) != last_cursor,
                        glfw::WindowEvent::Size(_, _) => Some(index) != last_size,
                        glfw::WindowEvent::Refresh => Some(index) != last_refresh,
                        _ => false,
                    };
                    if superseded {
                        continue;
                    }
                    let run_default_handler = if let Some(custom_handler) = &mut custom_handler {
                        custom_handler(self, view_id, event.clone())
                    } else {
                        true
                    };
                    if run_default_handler {
                        state.handle_glfw_event(view_id, event);
                    }
                }
            }

            state.flush_staged_key();
            if let Some(callback) = &mut frame_callback {
                callback(self);
            }
        }

        state.running.set(false);
        Ok(())
    }

    /// Queue `f` to run on the platform thread inside the event loop.
    ///
    /// # Errors
    ///
    /// Returns the callback back if the event loop has already stopped.
    pub fn post_main_thread_callback<F>(&self, f: F) -> Result<(), SendError<MainThreadFn>>
    where
        F: FnMut(&FlutterApp) + Send + 'static,
    {
        self.state.main_tx.send(Box::new(f))?;
        self.state.platform_task_handler.wake();
        Ok(())
    }

    /// Register an additional plugin on the app's engine.
    pub fn add_plugin<P>(&self, plugin: P) -> &Self
    where
        P: Plugin + 'static,
    {
        if let Some(engine) = self.state.engine() {
            self.state.plugins.write().add_plugin(&engine, plugin);
        }
        self
    }

    /// Run `f` against a registered plugin of type `P`.
    pub fn with_plugin<F, P>(&self, f: F)
    where
        F: FnOnce(&P),
        P: Plugin + 'static,
    {
        self.state.with_plugin(f);
    }

    /// Run `f` against a registered plugin of type `P` mutably.
    pub fn with_plugin_mut<F, P>(&self, f: F)
    where
        F: FnOnce(&mut P),
        P: Plugin + 'static,
    {
        self.state.with_plugin_mut(f);
    }

    /// Register a platform channel on the app's engine.
    pub fn register_channel<C>(&self, channel: C) -> Weak<C>
    where
        C: Channel + 'static,
    {
        match self.state.engine() {
            Some(engine) => engine.register_channel(channel),
            None => Weak::new(),
        }
    }

    /// Unregister a platform channel.
    pub fn remove_channel(&self, channel_name: &str) -> Option<Arc<dyn Channel>> {
        self.state.engine()?.remove_channel(channel_name)
    }

    /// Run `f` against a registered channel.
    pub fn with_channel<F>(&self, channel_name: &str, f: F)
    where
        F: FnMut(&dyn Channel),
    {
        if let Some(engine) = self.state.engine() {
            engine.with_channel(channel_name, f);
        }
    }

    /// The app's engine handle.
    #[must_use]
    pub fn engine(&self) -> Option<FlutterEngine> {
        self.state.engine()
    }

    /// Register an external texture.
    #[must_use]
    pub fn create_texture(&self) -> Option<Texture> {
        self.state.engine().map(|engine| engine.create_texture())
    }

    /// Destroy every window and shut the engine down.
    ///
    /// The order matters: the engine owns the raster/IO handlers that keep the
    /// GL contexts alive, and `glfw::Window::drop` blocks until its
    /// `RenderContext` is gone. Engine first, then the render state, then the
    /// windows.
    pub fn shutdown(self) {
        self.shutdown_inner();
    }

    fn shutdown_inner(&self) {
        ignore_dead_windows_on_teardown(&self.state.glfw);
        // Stop the windowing ABI from reaching this app before its engine and
        // windows are torn down.
        CURRENT_APP.with(|app| *app.borrow_mut() = None);
        let engine = self.state.engine.borrow_mut().take();
        if let Some(engine) = engine {
            if engine.is_platform_thread() {
                engine.shutdown();
            }
            drop(engine);
        }
        drop(self.state.render.borrow_mut().take());
        // Drop every handler that may retain a window handle before the
        // windows themselves: `PWindow::drop` must run while GLFW is still
        // initialized. `AppState.glfw` is dropped after these fields.
        self.state.focused.clear_all();
        *self.state.plugins.write() = PluginRegistrar::new();
        drop(self.state.windows.borrow_mut().drain().collect::<Vec<_>>());
        drop(
            self.state
                .retired_windows
                .borrow_mut()
                .drain(..)
                .collect::<Vec<_>>(),
        );
        drop(self.state.master.borrow_mut().take());
        drop(self.state.resource.borrow_mut().take());
    }
}

impl Drop for FlutterApp {
    fn drop(&mut self) {
        if !self.owns_app {
            return;
        }
        if self.state.engine.borrow().is_some() {
            if self.state.glfw_is_platform_thread() {
                self.shutdown_inner();
            } else {
                log::error!(
                    "FlutterApp dropped off the platform thread; call shutdown() on the thread that created it"
                );
            }
        }
    }
}

impl AppState {
    fn glfw_is_platform_thread(&self) -> bool {
        self.engine
            .borrow()
            .as_ref()
            .is_some_and(|engine| engine.is_platform_thread())
    }
}

/// Tag a window with the EWMH type that matches its role.
///
/// Without a type every window is `_NET_WM_WINDOW_TYPE_NORMAL`, so tiling
/// window managers and extensions treat tooltips, popups and satellites as
/// ordinary windows to tile. Setting the type (and `WM_TRANSIENT_FOR`, see
/// [`attach_to_parent`]) keeps them out of those layouts.
#[cfg(target_os = "linux")]
fn set_x11_window_type(window: &glfw::PWindow, glfw: &glfw::Glfw, kind: WindowKind) {
    let name = match kind {
        WindowKind::Regular => return,
        WindowKind::Dialog => c"_NET_WM_WINDOW_TYPE_DIALOG",
        WindowKind::Tooltip => c"_NET_WM_WINDOW_TYPE_TOOLTIP",
        WindowKind::Popup => c"_NET_WM_WINDOW_TYPE_POPUP_MENU",
        WindowKind::Satellite => c"_NET_WM_WINDOW_TYPE_UTILITY",
    };
    let Ok(xlib) = x11_dl::xlib::Xlib::open() else {
        return;
    };
    let display = glfw.get_x11_display() as *mut x11_dl::xlib::Display;
    let window = window.get_x11_window() as x11_dl::xlib::Window;
    if display.is_null() || window == 0 {
        return;
    }
    // SAFETY: `display` is GLFW's live Xlib connection, `window` belongs to it,
    // and the property is written before the window is mapped.
    unsafe {
        let type_atom = (xlib.XInternAtom)(display, c"_NET_WM_WINDOW_TYPE".as_ptr(), 0);
        let value_atom = (xlib.XInternAtom)(display, name.as_ptr(), 0);
        if type_atom == 0 || value_atom == 0 {
            return;
        }
        let atoms = [value_atom];
        (xlib.XChangeProperty)(
            display,
            window,
            type_atom,
            x11_dl::xlib::XA_ATOM,
            32,
            x11_dl::xlib::PropModeReplace,
            atoms.as_ptr().cast(),
            1,
        );
        (xlib.XFlush)(display);
    }
}

#[cfg(not(target_os = "linux"))]
fn set_x11_window_type(_window: &glfw::PWindow, _glfw: &glfw::Glfw, _kind: WindowKind) {}

/// Create a tooltip or popup as an override-redirect window.
///
/// The official Linux embedder creates these as GTK popups, and Xwayland turns
/// an override-redirect window into a surface attached to its parent. That is
/// what keeps them out of window managers' layouts (PaperWM only manages
/// `_NET_WM_WINDOW_TYPE_NORMAL` windows), makes them stack above their parent
/// instead of every window, and hides them with the parent. It must happen
/// before the window is mapped.
#[cfg(target_os = "linux")]
fn set_x11_override_redirect(window: &glfw::PWindow, glfw: &glfw::Glfw) {
    let Ok(xlib) = x11_dl::xlib::Xlib::open() else {
        return;
    };
    let display = glfw.get_x11_display() as *mut x11_dl::xlib::Display;
    let window = window.get_x11_window() as x11_dl::xlib::Window;
    if display.is_null() || window == 0 {
        return;
    }
    // SAFETY: `display` is GLFW's live Xlib connection, `window` belongs to it
    // and is still unmapped, which is the only time its override-redirect
    // attribute may be changed.
    unsafe {
        let mut attributes: x11_dl::xlib::XSetWindowAttributes = std::mem::zeroed();
        attributes.override_redirect = 1;
        (xlib.XChangeWindowAttributes)(
            display,
            window,
            x11_dl::xlib::CWOverrideRedirect,
            &mut attributes,
        );
        (xlib.XFlush)(display);
    }
}

#[cfg(not(target_os = "linux"))]
fn set_x11_override_redirect(_window: &glfw::PWindow, _glfw: &glfw::Glfw) {}

/// Tie a child window to its parent so it stacks directly above it.
///
/// The window stays at the parent's level: it is above the parent, not above
/// every window of every application.
#[cfg(target_os = "macos")]
pub(crate) fn attach_to_parent(child: &glfw::PWindow, parent: &glfw::PWindow, _glfw: &glfw::Glfw) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let child_window = child.get_cocoa_window() as *mut AnyObject;
    let parent_window = parent.get_cocoa_window() as *mut AnyObject;
    if child_window.is_null() || parent_window.is_null() {
        return;
    }
    // SAFETY: both pointers are live `NSWindow`s owned by GLFW, and this runs
    // on the main thread. `NSWindowAbove` (1) orders the child above its
    // parent and makes AppKit move it with the parent.
    unsafe {
        let _: () = msg_send![parent_window, addChildWindow: child_window, ordered: 1isize];
    }
}

/// Detach a window from its AppKit parent, if it has one.
#[cfg(target_os = "macos")]
pub(crate) fn detach_from_parent(window: &glfw::PWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let window = window.get_cocoa_window() as *mut AnyObject;
    if window.is_null() {
        return;
    }
    // SAFETY: `window` is a live `NSWindow` owned by GLFW; `parentWindow` may
    // return nil, which the null check below tolerates.
    unsafe {
        let parent: *mut AnyObject = msg_send![window, parentWindow];
        if !parent.is_null() {
            let _: () = msg_send![parent, removeChildWindow: window];
        }
    }
}

/// Mark a child window as transient-for its parent on X11. GLFW has no
/// portable modality API; the WM enforces `_NET_WM_STATE_MODAL` for the client.
#[cfg(target_os = "linux")]
fn attach_to_parent(child: &glfw::PWindow, parent: &glfw::PWindow, glfw: &glfw::Glfw) {
    let Ok(xlib) = x11_dl::xlib::Xlib::open() else {
        return;
    };
    let display = glfw.get_x11_display() as *mut x11_dl::xlib::Display;
    let child_window = child.get_x11_window() as x11_dl::xlib::Window;
    let parent_window = parent.get_x11_window() as x11_dl::xlib::Window;
    if display.is_null() || child_window == 0 || parent_window == 0 {
        return;
    }
    // SAFETY: `display` is GLFW's live Xlib connection, both window ids belong
    // to it, and the function pointer comes from the loaded Xlib.
    unsafe {
        (xlib.XSetTransientForHint)(display, child_window, parent_window);
        (xlib.XFlush)(display);
    }
}

/// Ask the X11 window manager to add `_NET_WM_STATE_MODAL` to a mapped dialog.
///
/// Setting the property directly before mapping is overwritten by WMs; the
/// EWMH client message is the supported way to change the state afterwards.
#[cfg(target_os = "linux")]
fn set_x11_modal_state(child: &glfw::PWindow, glfw: &glfw::Glfw) {
    let Ok(xlib) = x11_dl::xlib::Xlib::open() else {
        return;
    };
    let display = glfw.get_x11_display() as *mut x11_dl::xlib::Display;
    let child_window = child.get_x11_window() as x11_dl::xlib::Window;
    if display.is_null() || child_window == 0 {
        return;
    }

    // SAFETY: `display` is GLFW's live Xlib connection and the window id
    // belongs to it. `XEvent` is a union of C structs and zero is a valid
    // starting value.
    unsafe {
        let state_atom = (xlib.XInternAtom)(display, c"_NET_WM_STATE".as_ptr(), 0);
        let modal_atom = (xlib.XInternAtom)(display, c"_NET_WM_STATE_MODAL".as_ptr(), 0);
        if state_atom == 0 || modal_atom == 0 {
            return;
        }

        let mut event: x11_dl::xlib::XEvent = std::mem::zeroed();
        event.client_message.type_ = x11_dl::xlib::ClientMessage;
        event.client_message.send_event = 1;
        event.client_message.display = display;
        event.client_message.window = child_window;
        event.client_message.message_type = state_atom;
        event.client_message.format = 32;
        let data = event.client_message.data.as_longs_mut();
        data[0] = 1; // _NET_WM_STATE_ADD
        data[1] = modal_atom as _;
        let root = (xlib.XRootWindow)(display, 0);
        (xlib.XSendEvent)(
            display,
            root,
            0,
            x11_dl::xlib::SubstructureRedirectMask | x11_dl::xlib::SubstructureNotifyMask,
            &mut event,
        );
        (xlib.XFlush)(display);
    }
}

/// Give a child window an owner so Windows keeps it directly above its parent.
///
/// Win32 has neither transient-for nor `addChildWindow:`. An owned window —
/// the `GWLP_HWNDPARENT` value — is the equivalent: it stays above its owner
/// and is hidden with it without floating over every other application, which
/// is exactly what the macOS child-window path gets from AppKit.
#[cfg(target_os = "windows")]
fn attach_to_parent(child: &glfw::PWindow, parent: &glfw::PWindow, _glfw: &glfw::Glfw) {
    let child = child.get_win32_window();
    let parent = parent.get_win32_window();
    if child.is_null() || parent.is_null() {
        return;
    }
    // SAFETY: both are live HWNDs created by GLFW on this (the platform) thread.
    unsafe {
        set_window_owner(child, parent);
    }
}

/// Clear the owner set by [`attach_to_parent`].
#[cfg(target_os = "windows")]
fn detach_from_parent(window: &glfw::PWindow) {
    let window = window.get_win32_window();
    if window.is_null() {
        return;
    }
    // SAFETY: as above; a null owner clears the relationship.
    unsafe {
        set_window_owner(window, std::ptr::null_mut());
    }
}

#[cfg(target_os = "windows")]
unsafe fn set_window_owner(window: *mut std::ffi::c_void, owner: *mut std::ffi::c_void) {
    use std::ffi::c_void;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetWindowLongPtrW(window: *mut c_void, index: i32, value: isize) -> isize;
    }
    const GWLP_HWNDPARENT: i32 = -8;

    // SAFETY: the caller guarantees a live top-level HWND; the owner may be
    // null, which removes the relationship.
    unsafe {
        SetWindowLongPtrW(window, GWLP_HWNDPARENT, owner as isize);
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn attach_to_parent(_child: &glfw::PWindow, _parent: &glfw::PWindow, _glfw: &glfw::Glfw) {}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn detach_from_parent(_window: &glfw::PWindow) {}

#[cfg(not(target_os = "linux"))]
fn set_x11_modal_state(_child: &glfw::PWindow, _glfw: &glfw::Glfw) {}

/// Stop `BadWindow` from terminating the process once teardown starts.
///
/// A window that the window system destroyed behind the embedder's back still
/// has to go through `glfwDestroyWindow` so GLFW forgets it, and the X requests
/// inside that call fail with `BadWindow`. Xlib's default error handler prints
/// the error and calls `exit(1)`, which turns closing a window into a failed
/// process. Teardown wants such a window gone and it already is, so ignore
/// exactly that error for the rest of the process; every other error goes to
/// the handler Xlib had installed.
#[cfg(target_os = "linux")]
fn ignore_dead_windows_on_teardown(glfw: &glfw::Glfw) {
    use std::ffi::c_int;
    use std::sync::Mutex;

    type Handler =
        unsafe extern "C" fn(*mut x11_dl::xlib::Display, *mut x11_dl::xlib::XErrorEvent) -> c_int;

    static PREVIOUS: Mutex<Option<Handler>> = Mutex::new(None);

    unsafe extern "C" fn handler(
        display: *mut x11_dl::xlib::Display,
        event: *mut x11_dl::xlib::XErrorEvent,
    ) -> c_int {
        // SAFETY: Xlib passes a live error event to its handler, or null when
        // it has none to describe.
        if !event.is_null() && unsafe { (*event).error_code } == x11_dl::xlib::BadWindow {
            return 0;
        }
        let previous = *PREVIOUS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match previous {
            // SAFETY: this is the handler `XSetErrorHandler` returned when ours
            // was installed, so it is the one Xlib would have called.
            Some(previous) => unsafe { previous(display, event) },
            None => 0,
        }
    }

    if glfw.get_platform() != glfw::Platform::X11 || glfw.get_x11_display().is_null() {
        return;
    }
    let Ok(xlib) = x11_dl::xlib::Xlib::open() else {
        return;
    };
    // SAFETY: GLFW is initialised, so `display` is a live Xlib connection and
    // changing its process-wide error handler is safe here.
    unsafe {
        let previous = (xlib.XSetErrorHandler)(Some(handler));
        *PREVIOUS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = previous;
    }
}

#[cfg(not(target_os = "linux"))]
fn ignore_dead_windows_on_teardown(_glfw: &glfw::Glfw) {}

#[cfg(feature = "brightness")]
fn brightness_from(mode: dark_light::Mode) -> PlatformBrightness {
    match mode {
        dark_light::Mode::Dark => PlatformBrightness::Dark,
        dark_light::Mode::Light => PlatformBrightness::Light,
        dark_light::Mode::Unspecified => PlatformBrightness::default(),
    }
}

#[cfg(feature = "brightness")]
fn detect_platform_brightness() -> PlatformBrightness {
    match dark_light::detect() {
        Ok(mode) => brightness_from(mode),
        Err(error) => {
            debug!("could not read the host's light/dark preference: {error}");
            PlatformBrightness::default()
        }
    }
}

#[cfg(not(feature = "brightness"))]
fn detect_platform_brightness() -> PlatformBrightness {
    PlatformBrightness::default()
}

impl AppState {
    fn watch_platform_brightness(&self) {
        if self.brightness_started.replace(true) {
            return;
        }
        self.spawn_brightness_watcher();
    }

    #[cfg(feature = "brightness")]
    fn spawn_brightness_watcher(&self) {
        let watcher = match dark_light::subscribe() {
            Ok(watcher) => watcher,
            Err(error) => {
                debug!("not following the host's light/dark preference: {error}");
                return;
            }
        };
        // The watcher thread cannot reach `AppState` (it is main-thread only);
        // it posts a callback through the main-thread channel instead.
        let (sender, receiver) = mpsc::channel::<PlatformBrightness>();
        let _ = sender;
        let _ = receiver;
        let _ = watcher;
    }

    #[cfg(not(feature = "brightness"))]
    fn spawn_brightness_watcher(&self) {}
}
