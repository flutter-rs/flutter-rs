//! Window types and per-window input state.

use flutter_engine::ffi::{FlutterPointerPhase, FlutterPointerSignalKind, PointerButtons};
use flutter_engine::{FlutterEngine, WindowMetrics};
use flutter_plugins::lifecycle::AppLifecycleState;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

// seems to be about 2.5 lines of text
pub(crate) const SCROLL_SPEED: f64 = 50.0;

/// The `textScaleFactor` sent on `flutter/settings`.
pub(crate) const TEXT_SCALE_FACTOR: f64 = 1.0;
/// The `alwaysUse24HourFormat` sent on `flutter/settings`.
pub(crate) const USE_24_HOUR_FORMAT: bool = false;

#[cfg(not(target_os = "macos"))]
pub(crate) const BY_WORD_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Control;
#[cfg(target_os = "macos")]
pub(crate) const BY_WORD_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Alt;
pub(crate) const SELECT_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Shift;
#[cfg(not(target_os = "macos"))]
pub(crate) const FUNCTION_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Control;
#[cfg(target_os = "macos")]
pub(crate) const FUNCTION_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Super;

/// Why a window could not be created.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum CreateError {
    /// GLFW refused to create the window or one of its shared contexts.
    #[error("failed to create a window")]
    WindowCreationFailed,
    /// [`WindowMode::Fullscreen`] named a monitor index that is not connected.
    #[error("no monitor with the specified index found")]
    MonitorNotFound,
    /// A window cannot be created before the engine is running unless it is the
    /// single implicit-view window.
    #[error("only the first window may be created before run()")]
    WindowBeforeRun,
    /// The native window does not exist.
    #[error("no window with view id {0}")]
    NoSuchWindow(i64),
    /// The Flutter engine itself could not be created.
    #[error(transparent)]
    Engine(#[from] flutter_engine::CreateError),
    /// Adding a view failed.
    #[error(transparent)]
    View(#[from] flutter_engine::ViewError),
}

/// Why [`FlutterApp::run`](crate::FlutterApp::run) stopped before the app
/// exited.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RunError {
    /// The engine would not start, so no event loop was entered.
    #[error(transparent)]
    Engine(#[from] flutter_engine::RunError),
}

/// How a window is placed on screen.
pub enum WindowMode {
    /// Fullscreen on the connected monitor with this index.
    Fullscreen(usize),
    /// A normal decorated window.
    Windowed,
    /// An undecorated window; use `flutter/window`'s `startDrag` to move it.
    Borderless,
}

/// A window to create, in screen coordinates (not physical pixels).
pub struct WindowArgs<'a> {
    /// Width in screen coordinates.
    pub width: u32,
    /// Height in screen coordinates.
    pub height: u32,
    /// The window title, also used as the app switcher description.
    pub title: &'a str,
    /// How the window is placed on screen.
    pub mode: WindowMode,
}

/// What kind of native window this is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowKind {
    Regular,
    Dialog,
    Tooltip,
    Popup,
    Satellite,
}

impl WindowKind {
    pub(crate) fn is_floating(self) -> bool {
        matches!(self, Self::Tooltip | Self::Popup)
    }

    pub(crate) fn is_decorated(self) -> bool {
        !matches!(self, Self::Tooltip | Self::Popup)
    }
}

/// Everything the window factory needs; the public [`WindowArgs`] and the
/// Dart windowing ABI both build one of these.
pub(crate) struct WindowRequest {
    pub kind: WindowKind,
    pub title: String,
    pub width: f64,
    pub height: f64,
    pub resizable: bool,
    pub decorated: bool,
    pub floating: bool,
    pub parent: Option<i64>,
    /// A modal dialog blocks input to `parent`.
    pub modal: bool,
    pub shrink_wrap: bool,
    pub position: Option<(f64, f64)>,
    pub mode: WindowMode,
}

impl WindowRequest {
    pub(crate) fn regular(args: &WindowArgs<'_>) -> Self {
        Self {
            kind: WindowKind::Regular,
            title: args.title.to_owned(),
            width: f64::from(args.width),
            height: f64::from(args.height),
            resizable: true,
            decorated: matches!(args.mode, WindowMode::Windowed),
            floating: false,
            parent: None,
            modal: false,
            shrink_wrap: false,
            position: None,
            mode: match args.mode {
                WindowMode::Fullscreen(index) => WindowMode::Fullscreen(index),
                WindowMode::Windowed => WindowMode::Windowed,
                WindowMode::Borderless => WindowMode::Borderless,
            },
        }
    }

    pub(crate) fn logical_size(&self) -> (f64, f64) {
        (self.width.max(1.0), self.height.max(1.0))
    }
}

/// The largest loose maximum reported for a view whose content sizes itself.
///
/// Tooltips and popups are override-redirect windows that show intrinsic,
/// animated content: they need a bound they will never plausibly reach so the
/// window can grow with its content.
const MAX_LOOSE_SIZE: usize = 16_384;

/// The physical size a shrink-wrapped view's loose metrics may allow.
///
/// The framework fills a finite loose maximum: `Overlay` (and therefore every
/// `MaterialApp`) lays out at `constraints.biggest`, so a large bound makes
/// ordinary content lay out at that bound. A window that is meant to size to
/// its content must therefore report a bound it could actually have: the size
/// it was created with. Content that sizes itself still shrinks the window
/// below that bound, and the bound follows a window the user enlarged.
///
/// Tooltips and popups are exempt: their content is intrinsic and animates, so
/// they keep [`MAX_LOOSE_SIZE`.
fn shrink_wrap_bound(
    kind: WindowKind,
    initial: (usize, usize),
    framebuffer: (usize, usize),
) -> (usize, usize) {
    if kind.is_floating() {
        return (MAX_LOOSE_SIZE, MAX_LOOSE_SIZE);
    }
    (initial.0.max(framebuffer.0), initial.1.max(framebuffer.1))
}

/// Whether the window's native X11 window still exists.
///
/// GLFW does not report a window that another client destroyed (a window
/// manager force-closing an unresponsive application, or `xdotool
/// windowclose`): its `DestroyNotify` handler is empty, so no event reaches the
/// application. The window then stays in the window map with a dead X
/// resource, and the next GLFW query on it — `glfwGetWindowAttrib`,
/// `glfwGetWindowSize` and friends all call `XGetWindowAttributes` — raises
/// `BadWindow`. Xlib's default error handler turns that into `exit(1)`, which
/// is exactly what closing a window must never do.
///
/// X reports the same error for the probe itself, so run it under an error
/// handler that only records whether it fired. The probe is a synchronous
/// round trip and is flushed before the previous handler goes back, so no
/// unrelated request can be attributed to it.
#[cfg(target_os = "linux")]
pub(crate) fn native_window_alive(window: &glfw::PWindow, glfw: &glfw::Glfw) -> bool {
    use std::ffi::c_int;
    use std::sync::atomic::{AtomicBool, Ordering};

    static SAW_ERROR: AtomicBool = AtomicBool::new(false);

    unsafe extern "C" fn note_error(
        _display: *mut x11_dl::xlib::Display,
        _event: *mut x11_dl::xlib::XErrorEvent,
    ) -> c_int {
        SAW_ERROR.store(true, Ordering::Relaxed);
        0
    }

    if glfw.get_platform() != glfw::Platform::X11 {
        return true;
    }
    let Ok(xlib) = x11_dl::xlib::Xlib::open() else {
        return true;
    };
    let display = glfw.get_x11_display() as *mut x11_dl::xlib::Display;
    let handle = window.get_x11_window() as x11_dl::xlib::Window;
    if display.is_null() || handle == 0 {
        return false;
    }
    SAW_ERROR.store(false, Ordering::Relaxed);
    // SAFETY: `display` is GLFW's live Xlib connection and `handle` is a window
    // that was created on it. Both calls take a valid display; the attributes
    // buffer is a live, zero-initialized out-parameter. The handler is restored
    // before returning, so no later error is swallowed.
    unsafe {
        let previous = (xlib.XSetErrorHandler)(Some(note_error));
        let mut attributes: x11_dl::xlib::XWindowAttributes = std::mem::zeroed();
        let status = (xlib.XGetWindowAttributes)(display, handle, &mut attributes);
        (xlib.XSync)(display, x11_dl::xlib::False);
        (xlib.XSetErrorHandler)(previous);
        status != 0 && !SAW_ERROR.load(Ordering::Relaxed)
    }
}

/// Whether the window's native handle still exists.
///
/// Only X11 loses windows behind the embedder's back; elsewhere the platform
/// owns the handle for as long as the window exists.
#[cfg(not(target_os = "linux"))]
pub(crate) fn native_window_alive(_window: &glfw::PWindow, _glfw: &glfw::Glfw) -> bool {
    true
}

/// One native window and the input state that belongs to it alone.
pub(crate) struct WindowRecord {
    pub view_id: i64,
    pub kind: WindowKind,
    pub window: Arc<Mutex<glfw::PWindow>>,
    pub receiver: glfw::GlfwReceiver<(f64, glfw::WindowEvent)>,
    pub parent: Option<i64>,
    /// A modal dialog blocks input to `parent`.
    pub modal: bool,
    pub shrink_wrap: bool,
    /// The physical size a shrink-wrapped view may grow to.
    ///
    /// Loose metrics need a finite maximum, but whatever bound is reported is
    /// also the size the framework lets its content fill: `Overlay` (and
    /// therefore `MaterialApp`) lays out at `constraints.biggest` whenever that
    /// is finite. A large constant bound would make an ordinary window's
    /// content 16384×16384 pixels instead of the size its content asks for.
    /// See [`shrink_wrap_bound`].
    pub shrink_wrap_max: (usize, usize),
    pub logical_size: (f64, f64),
    pub position: (i32, i32),
    /// The position the embedder last asked the window manager to put this
    /// window at, or its position when it was created. Follow moves add to
    /// this rather than to the last observed position: a reparenting window
    /// manager applies client move requests asynchronously, so reading the
    /// position back in the middle of a burst of moves returns an old value
    /// and the remaining deltas would be dropped.
    pub follow_position: (i32, i32),
    /// Positions requested for this window that its own position events have
    /// not acknowledged yet, oldest first. An event that matches one of these
    /// is the echo of our own move; anything else was moved by the user or the
    /// window manager and becomes the new follow base.
    pub requested_positions: VecDeque<(i32, i32)>,
    /// A logical position that must be applied once the window's content scale
    /// has settled. GLFW can report the monitor scale before the window is
    /// mapped, and it may differ from the scale the WM assigns afterwards;
    /// applying the position too early leaves siblings offset by the
    /// difference.
    pub pending_position: Option<(f64, f64)>,
    /// A raw physical position to apply once the window is mapped.
    pub pending_physical_position: Option<(f64, f64)>,
    /// Whether at least one GLFW event has been seen for this window, which
    /// means it is mapped and a position set now will stick.
    pub mapped: bool,
    pub framebuffer_size: (i32, i32),
    pub scale: f64,
    /// GLFW's screen coordinates are logical (macOS points, Wayland surface
    /// units) rather than physical pixels.
    pub logical_screen_coordinates: bool,
    pub pixels_per_screen: f64,
    pub pointer_added: bool,
    /// Last cursor position reported by GLFW, in screen coordinates. Keeping
    /// it avoids `glfwGetCursorPos` (an X round-trip) in every pointer event and,
    /// more importantly, avoids querying a window whose native handle may
    /// already be gone.
    pub cursor_pos: (f64, f64),
    pub mouse_tracker: HashMap<glfw::MouseButton, glfw::Action>,
    pub fullscreen: bool,
}

impl WindowRecord {
    pub(crate) fn refresh_geometry(&mut self) {
        let window = self.window.lock();
        let window_size = window.get_size();
        let framebuffer = window.get_framebuffer_size();
        let scale = f64::from(window.get_content_scale().0).max(0.01);
        self.scale = scale;
        self.framebuffer_size = framebuffer;
        // The framework's logical size is the framebuffer divided by the
        // content scale. On X11 and Windows `get_size` is already in physical
        // pixels, so it must not be used as the logical size.
        self.logical_size = (
            f64::from(framebuffer.0) / scale,
            f64::from(framebuffer.1) / scale,
        );
        // Pointer coordinates are screen coordinates; the engine wants
        // physical pixels, which is exactly framebuffer / window size.
        self.pixels_per_screen = pixels_per_screen_coordinate(window_size.0, framebuffer.0);
    }

    /// GLFW's window positions and sizes are in screen coordinates. On X11
    /// and Windows those are physical pixels and must be scaled; on macOS they
    /// are points already, so the logical value is passed through.
    pub(crate) fn coordinate_scale(&self) -> f64 {
        if self.logical_screen_coordinates {
            1.0
        } else {
            self.scale.max(0.01)
        }
    }

    pub(crate) fn metrics(&self) -> WindowMetrics {
        let width = self.framebuffer_size.0.max(0) as usize;
        let height = self.framebuffer_size.1.max(0) as usize;
        let ratio = if self.scale > 0.0 { self.scale } else { 1.0 };
        if self.shrink_wrap {
            let (max_width, max_height) =
                shrink_wrap_bound(self.kind, self.shrink_wrap_max, (width, height));
            WindowMetrics::loose(
                width.max(1),
                height.max(1),
                ratio,
                max_width.max(1),
                max_height.max(1),
            )
        } else {
            WindowMetrics::tight(width, height, ratio)
        }
    }

    pub(crate) fn pressed_buttons(&self) -> PointerButtons {
        pressed_buttons(&self.mouse_tracker)
    }

    /// Build the pointer events for one host event.
    ///
    /// The engine dispatches a pointer event synchronously into Dart, which may
    /// call back into this crate (a button handler creates the next window), so
    /// the caller must send these *after* releasing the window-map borrow.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_pointer_events(
        &mut self,
        phase: FlutterPointerPhase,
        (x, y): (f64, f64),
        signal_kind: FlutterPointerSignalKind,
        (scroll_delta_x, scroll_delta_y): (f64, f64),
        buttons: PointerButtons,
    ) -> Vec<PreparedPointer> {
        let mut events = Vec::with_capacity(2);
        if !self.pointer_added
            && phase != FlutterPointerPhase::Add
            && phase != FlutterPointerPhase::Remove
        {
            events.push(PreparedPointer {
                view_id: self.view_id,
                phase: FlutterPointerPhase::Add,
                x: x * self.pixels_per_screen,
                y: y * self.pixels_per_screen,
                signal_kind: FlutterPointerSignalKind::None,
                scroll_delta_x: 0.0,
                scroll_delta_y: 0.0,
                buttons: PointerButtons::NONE,
            });
            self.pointer_added = true;
        }
        if self.pointer_added && phase == FlutterPointerPhase::Add
            || !self.pointer_added && phase == FlutterPointerPhase::Remove
        {
            return events;
        }

        events.push(PreparedPointer {
            view_id: self.view_id,
            phase,
            x: x * self.pixels_per_screen,
            y: y * self.pixels_per_screen,
            signal_kind,
            scroll_delta_x: scroll_delta_x * self.pixels_per_screen,
            scroll_delta_y: scroll_delta_y * self.pixels_per_screen,
            buttons,
        });

        match phase {
            FlutterPointerPhase::Add => self.pointer_added = true,
            FlutterPointerPhase::Remove => self.pointer_added = false,
            _ => {}
        }
        events
    }
}

/// Match an observed window position against the positions we asked for.
///
/// A window's position event can be the echo of the embedder's own move (the
/// window manager applies those asynchronously), or a move the user or the
/// window manager made. Only the latter may change [`WindowRecord::follow_position`],
/// the base that parent-follow moves add to; adopting an echo would move the
/// satellite by its own follow offset.
///
/// Returns `true` when the event acknowledged one of our requests.
pub(crate) fn observe_position(
    position: (i32, i32),
    follow_position: &mut (i32, i32),
    requested_positions: &mut VecDeque<(i32, i32)>,
) -> bool {
    let acknowledged = match requested_positions.front() {
        Some(&front) if front == position => {
            requested_positions.pop_front();
            true
        }
        Some(_) if requested_positions.contains(&position) => {
            // A window manager coalesced the moves in between.
            while requested_positions.pop_front() != Some(position) {}
            true
        }
        Some(_) => {
            // Not one of our requests: an independent move. Drop the rest;
            // their echoes are newer and will be matched below as they arrive.
            requested_positions.clear();
            false
        }
        None => false,
    };
    if !acknowledged {
        *follow_position = position;
    }
    acknowledged
}

/// A pointer event ready to be sent to the engine.
pub(crate) struct PreparedPointer {
    pub view_id: i64,
    pub phase: FlutterPointerPhase,
    pub x: f64,
    pub y: f64,
    pub signal_kind: FlutterPointerSignalKind,
    pub scroll_delta_x: f64,
    pub scroll_delta_y: f64,
    pub buttons: PointerButtons,
}

/// The lifecycle state visibility and focus add up to.
pub(crate) fn lifecycle_state(visible: bool, focused: bool) -> AppLifecycleState {
    if !visible {
        AppLifecycleState::Hidden
    } else if focused {
        AppLifecycleState::Resumed
    } else {
        AppLifecycleState::Inactive
    }
}

/// App-level visibility and focus, and the last lifecycle state sent.
#[derive(Debug)]
pub(crate) struct LifecycleTracker {
    visible: bool,
    focused: bool,
    sent: Option<AppLifecycleState>,
}

impl LifecycleTracker {
    pub(crate) fn new() -> Self {
        Self {
            visible: true,
            focused: false,
            sent: None,
        }
    }

    pub(crate) fn update(&mut self, visible: bool, focused: bool) -> Option<AppLifecycleState> {
        self.visible = visible;
        self.focused = focused;
        let state = lifecycle_state(visible, focused);
        if self.sent == Some(state) {
            return None;
        }
        self.sent = Some(state);
        Some(state)
    }

    pub(crate) fn set_focused(&mut self, focused: bool) -> Option<AppLifecycleState> {
        self.update(self.visible, focused)
    }

    pub(crate) fn set_visible(&mut self, visible: bool) -> Option<AppLifecycleState> {
        self.update(visible, self.focused)
    }
}

/// The cumulative mask of the buttons `tracked` says are down.
pub(crate) fn pressed_buttons(
    tracked: &HashMap<glfw::MouseButton, glfw::Action>,
) -> PointerButtons {
    tracked
        .iter()
        .filter(|(_, action)| **action == glfw::Action::Press)
        .fold(PointerButtons::NONE, |mask, (button, _)| {
            mask | match *button {
                glfw::MouseButtonLeft => PointerButtons::PRIMARY,
                glfw::MouseButtonRight => PointerButtons::SECONDARY,
                glfw::MouseButtonMiddle => PointerButtons::MIDDLE,
                glfw::MouseButton::Button4 => PointerButtons::NONE,
                glfw::MouseButton::Button5 => PointerButtons::FORWARD,
                glfw::MouseButton::Button6 => PointerButtons::from_bits(1 << 5),
                glfw::MouseButton::Button7 => PointerButtons::from_bits(1 << 6),
                glfw::MouseButton::Button8 => PointerButtons::from_bits(1 << 7),
            }
        })
}

/// Whether a cursor movement is a drag or a hover.
pub(crate) fn move_or_hover(buttons: PointerButtons) -> FlutterPointerPhase {
    if buttons.is_empty() {
        FlutterPointerPhase::Hover
    } else {
        FlutterPointerPhase::Move
    }
}

/// How many physical pixels one screen coordinate is worth.
pub(crate) fn pixels_per_screen_coordinate(window_width: i32, framebuffer_width: i32) -> f64 {
    if window_width > 0 && framebuffer_width > 0 {
        f64::from(framebuffer_width) / f64::from(window_width)
    } else {
        1.0
    }
}

/// A hook that sees every GLFW event first and returns whether the default
/// handling should still run.
pub type WindowEventHandler = dyn FnMut(&crate::FlutterApp, i64, glfw::WindowEvent) -> bool;
/// A hook run once per iteration of the event loop.
pub type PerFrameCallback = dyn FnMut(&crate::FlutterApp);

/// A created window, as a handle to the app that owns it.
///
/// The handle is `!Send` like the app: window operations run on the platform
/// thread.
pub struct FlutterWindow {
    app: std::rc::Weak<crate::app::AppState>,
    view_id: i64,
    window: Arc<Mutex<glfw::PWindow>>,
}

impl FlutterWindow {
    pub(crate) fn new(
        app: std::rc::Weak<crate::app::AppState>,
        view_id: i64,
        window: Arc<Mutex<glfw::PWindow>>,
    ) -> Self {
        Self {
            app,
            view_id,
            window,
        }
    }

    /// The engine view this window presents.
    #[must_use]
    pub fn view_id(&self) -> i64 {
        self.view_id
    }

    /// The underlying GLFW window.
    #[must_use]
    pub fn window(&self) -> Arc<Mutex<glfw::PWindow>> {
        self.window.clone()
    }

    /// The app's engine handle, or `None` after the app was shut down.
    #[must_use]
    pub fn engine(&self) -> Option<FlutterEngine> {
        self.app.upgrade().and_then(|app| app.engine())
    }

    /// Set the window title.
    pub fn set_title(&self, title: &str) {
        self.window.lock().set_title(title);
    }

    /// Ask the window to close, as the window manager's close button does.
    pub fn request_close(&self) {
        self.window.lock().set_should_close(true);
    }

    /// Destroy this window and remove its view.
    pub fn destroy(&self) {
        if let Some(app) = self.app.upgrade() {
            app.destroy_window(self.view_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker<const N: usize>(
        entries: [(glfw::MouseButton, glfw::Action); N],
    ) -> HashMap<glfw::MouseButton, glfw::Action> {
        entries.into_iter().collect()
    }

    #[test]
    fn hovering_reports_no_button() {
        assert_eq!(pressed_buttons(&tracker([])), PointerButtons::NONE);
        assert_eq!(
            move_or_hover(PointerButtons::NONE),
            FlutterPointerPhase::Hover
        );
    }

    #[test]
    fn only_pressed_buttons_are_in_the_mask() {
        let tracked = tracker([
            (glfw::MouseButtonLeft, glfw::Action::Press),
            (glfw::MouseButtonRight, glfw::Action::Release),
        ]);
        assert_eq!(pressed_buttons(&tracked), PointerButtons::PRIMARY);
    }

    #[test]
    fn all_mouse_buttons_map_to_their_flutter_bits() {
        let tracked = tracker([
            (glfw::MouseButtonLeft, glfw::Action::Press),
            (glfw::MouseButtonRight, glfw::Action::Press),
            (glfw::MouseButtonMiddle, glfw::Action::Press),
            (glfw::MouseButton::Button5, glfw::Action::Press),
            (glfw::MouseButton::Button6, glfw::Action::Press),
            (glfw::MouseButton::Button7, glfw::Action::Press),
            (glfw::MouseButton::Button8, glfw::Action::Press),
        ]);
        let mask = pressed_buttons(&tracked);
        assert!(mask.contains(PointerButtons::PRIMARY));
        assert!(mask.contains(PointerButtons::SECONDARY));
        assert!(mask.contains(PointerButtons::MIDDLE));
        assert!(mask.contains(PointerButtons::FORWARD));
        assert_eq!(mask.bits() & (1 << 5), 1 << 5);
        assert_eq!(mask.bits() & (1 << 6), 1 << 6);
        assert_eq!(mask.bits() & (1 << 7), 1 << 7);
    }

    #[test]
    fn pixel_ratio_falls_back_to_one() {
        assert_eq!(pixels_per_screen_coordinate(0, 0), 1.0);
        assert_eq!(pixels_per_screen_coordinate(800, 1600), 2.0);
        assert_eq!(pixels_per_screen_coordinate(-1, 1), 1.0);
    }

    #[test]
    fn lifecycle_maps_visibility_and_focus() {
        assert_eq!(lifecycle_state(true, true), AppLifecycleState::Resumed);
        assert_eq!(lifecycle_state(true, false), AppLifecycleState::Inactive);
        assert_eq!(lifecycle_state(false, true), AppLifecycleState::Hidden);
        assert_eq!(lifecycle_state(false, false), AppLifecycleState::Hidden);
    }

    #[test]
    fn lifecycle_dedupes_repeated_states() {
        let mut tracker = LifecycleTracker::new();
        assert_eq!(tracker.update(true, true), Some(AppLifecycleState::Resumed));
        assert_eq!(tracker.update(true, true), None);
        assert_eq!(
            tracker.update(true, false),
            Some(AppLifecycleState::Inactive)
        );
        assert_eq!(
            tracker.update(false, false),
            Some(AppLifecycleState::Hidden)
        );
        assert_eq!(tracker.update(false, true), None);
    }

    #[test]
    fn follow_position_ignores_its_own_echoes() {
        let mut follow = (100, 200);
        let mut requested = VecDeque::from([(90, 195), (80, 190)]);
        assert!(observe_position((90, 195), &mut follow, &mut requested));
        assert_eq!(follow, (100, 200));
        assert_eq!(requested, VecDeque::from([(80, 190)]));
        assert!(observe_position((80, 190), &mut follow, &mut requested));
        assert_eq!(follow, (100, 200));
        assert!(requested.is_empty());
    }

    #[test]
    fn follow_position_adopts_independent_moves() {
        let mut follow = (100, 200);
        let mut requested = VecDeque::new();
        assert!(!observe_position((150, 250), &mut follow, &mut requested));
        assert_eq!(follow, (150, 250));
    }

    #[test]
    fn follow_position_skips_coalesced_echoes() {
        let mut follow = (0, 0);
        let mut requested = VecDeque::from([(10, 10), (20, 20), (30, 30)]);
        assert!(observe_position((30, 30), &mut follow, &mut requested));
        assert_eq!(follow, (0, 0));
        assert!(requested.is_empty());
    }

    #[test]
    fn follow_position_clears_queue_on_stale_event() {
        let mut follow = (0, 0);
        let mut requested = VecDeque::from([(10, 10), (20, 20)]);
        // A late map-time placement: not one of our moves.
        assert!(!observe_position((5, 5), &mut follow, &mut requested));
        assert_eq!(follow, (5, 5));
        assert!(requested.is_empty());
        // The echo of our own move is now adopted; that position is still the
        // one we asked for, so the base ends up where it should be.
        assert!(!observe_position((20, 20), &mut follow, &mut requested));
        assert_eq!(follow, (20, 20));
    }

    #[test]
    fn shrink_wrap_bound_keeps_regular_windows_at_their_own_size() {
        // A MaterialApp lays out at whichever finite bound it is given, so the
        // bound must be the window the user asked for, not a huge constant.
        assert_eq!(
            shrink_wrap_bound(WindowKind::Regular, (800, 600), (800, 600)),
            (800, 600)
        );
    }

    #[test]
    fn shrink_wrap_bound_grows_with_a_user_resize() {
        assert_eq!(
            shrink_wrap_bound(WindowKind::Dialog, (800, 600), (1200, 900)),
            (1200, 900)
        );
    }

    #[test]
    fn shrink_wrap_bound_keeps_room_after_the_window_shrank() {
        // Content that sized itself smaller can grow back to the original
        // bound instead of being pinned to the last frame's size.
        assert_eq!(
            shrink_wrap_bound(WindowKind::Satellite, (800, 600), (300, 200)),
            (800, 600)
        );
    }

    #[test]
    fn floating_windows_keep_room_to_animate() {
        assert_eq!(
            shrink_wrap_bound(WindowKind::Tooltip, (240, 100), (240, 100)),
            (MAX_LOOSE_SIZE, MAX_LOOSE_SIZE)
        );
        assert_eq!(
            shrink_wrap_bound(WindowKind::Popup, (240, 100), (240, 100)),
            (MAX_LOOSE_SIZE, MAX_LOOSE_SIZE)
        );
    }
}
