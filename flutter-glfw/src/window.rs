//! The GLFW window and the engine that renders into it.
//!
//! [`FlutterWindow`] is created through
//! [`FlutterDesktop::create_window`](crate::FlutterDesktop::create_window),
//! owns the thread it was created on for the duration of [`FlutterWindow::run`]
//! and translates GLFW's pointer, scroll, key and metrics events into the
//! engine's. That thread is the engine's platform thread: every method here
//! except the texture and channel helpers must be called on it.

use crate::handler::{
    GlfwMouseCursorHandler, GlfwOpenGLHandler, GlfwPlatformHandler, GlfwPlatformTaskHandler,
    GlfwTextInputHandler, GlfwWindowHandler,
};
use flutter_engine::FlutterEngine;
use flutter_engine::builder::FlutterEngineBuilder;
use flutter_engine::channel::Channel;
use flutter_engine::ffi::{
    FlutterPointerDeviceKind, FlutterPointerPhase, FlutterPointerSignalKind, KeyEventType,
    PointerButtons,
};
use flutter_engine::plugins::{Plugin, PluginRegistrar};
use flutter_engine::tasks::TaskRunnerHandler;
use flutter_engine::texture_registry::Texture;
use flutter_plugins::accessibility::AccessibilityPlugin;
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
use log::{debug, info};
// `warn!` only reports a brightness watcher that could not start.
#[cfg(feature = "brightness")]
use log::warn;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::sync::{Arc, Weak, mpsc};
use std::time::Instant;

// seems to be about 2.5 lines of text
const SCROLL_SPEED: f64 = 50.0;

/// The `textScaleFactor` sent on `flutter/settings`.
///
/// GLFW reports the window's *content* scale, which this crate already turns
/// into the device pixel ratio of the window metrics, and nothing at all about
/// the user's preferred text scaling; neither X11 nor Wayland defines a
/// cross-desktop setting for it, and the XDG appearance portal does not carry
/// one either. Inventing a number from the content scale would double-apply
/// the DPI, so the framework's own default goes out instead: an app that knows
/// better can send its own message through
/// [`FlutterWindow::with_plugin`]`::<SettingsPlugin>`.
const TEXT_SCALE_FACTOR: f64 = 1.0;

/// The `alwaysUse24HourFormat` sent on `flutter/settings`.
///
/// Sourcing this honestly means asking the C library for the locale's time
/// format — `nl_langinfo(T_FMT)` on glibc, `LOCALE_STIMEFORMAT` on Windows,
/// `NSDateFormatter` on macOS — none of which this crate links, and deriving
/// it from the language tag `sys_locale` returns would be making data up (`en`
/// is 12-hour in the US and 24-hour in Ireland). So the framework's own
/// default goes out, and an app that knows its users overrides it through
/// [`FlutterWindow::with_plugin`]`::<SettingsPlugin>`.
const USE_24_HOUR_FORMAT: bool = false;

#[cfg(not(target_os = "macos"))]
const BY_WORD_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Control;
#[cfg(target_os = "macos")]
const BY_WORD_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Alt;
const SELECT_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Shift;
#[cfg(not(target_os = "macos"))]
const FUNCTION_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Control;
#[cfg(target_os = "macos")]
const FUNCTION_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Super;

/// Why a [`FlutterWindow`] could not be created.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum CreateError {
    /// A window had already been created for this [`FlutterDesktop`].
    ///
    /// [`FlutterDesktop`]: crate::FlutterDesktop
    #[error("window was already created")]
    WindowAlreadyCreated,
    /// GLFW refused to create the window or its shared resource window.
    #[error("failed to create a window")]
    WindowCreationFailed,
    /// [`WindowMode::Fullscreen`] named a monitor index that is not connected.
    #[error("no monitor with the specified index found")]
    MonitorNotFound,
    /// The Flutter engine itself could not be created.
    #[error(transparent)]
    Engine(#[from] flutter_engine::CreateError),
}

/// Why [`FlutterWindow::run`] stopped before the window was closed.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RunError {
    /// The engine would not start, so no event loop was entered.
    #[error(transparent)]
    Engine(#[from] flutter_engine::RunError),
}

/// How the window is placed on screen.
pub enum WindowMode {
    /// Fullscreen on the connected monitor with this index.
    Fullscreen(usize),
    /// A normal decorated window.
    Windowed,
    /// An undecorated window; use `flutter/window`'s `startDrag` to move it.
    Borderless,
}

/// The window to create, in screen coordinates (not physical pixels).
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

pub(crate) type MainTheadFn = Box<dyn FnMut(&FlutterWindow) + Send>;
/// A hook that sees every GLFW event first and returns whether the default
/// handling should still run.
pub type WindowEventHandler = dyn FnMut(&FlutterWindow, glfw::WindowEvent) -> bool;
/// A hook run once per iteration of the event loop.
pub type PerFrameCallback = dyn FnMut(&FlutterWindow);

/// A GLFW window and the Flutter engine that renders into it.
///
/// Created through [`FlutterDesktop::create_window`], driven by [`Self::run`]
/// and released by [`Self::shutdown`] or by being dropped. It is tied to the
/// thread that created it: that thread is the engine's platform thread.
///
/// [`FlutterDesktop::create_window`]: crate::FlutterDesktop::create_window
pub struct FlutterWindow {
    glfw: glfw::Glfw,
    window: Arc<Mutex<glfw::PWindow>>,
    window_receiver: glfw::GlfwReceiver<(f64, glfw::WindowEvent)>,
    _resource_window: glfw::PWindow,
    _resource_window_receiver: glfw::GlfwReceiver<(f64, glfw::WindowEvent)>,
    engine: FlutterEngine,
    pointer_currently_added: AtomicBool,
    window_pixels_per_screen_coordinate: AtomicU64,
    main_thread_receiver: Receiver<MainTheadFn>,
    main_thread_sender: Sender<MainTheadFn>,
    mouse_tracker: Mutex<HashMap<glfw::MouseButton, glfw::Action>>,
    key_stager: Mutex<KeyStager>,
    pressed_keys: Arc<Mutex<PressedKeys>>,
    lifecycle: Mutex<LifecycleTracker>,
    brightness_watcher_started: AtomicBool,
    window_handler: Arc<Mutex<GlfwWindowHandler>>,
    platform_task_handler: Arc<GlfwPlatformTaskHandler>,
    plugins: RwLock<PluginRegistrar>,
}

/// The lifecycle state a window's visibility and focus add up to.
///
/// Mirrors the official GTK embedder's `fl_window_state_monitor`: a window with
/// pixels on screen is `resumed` when it holds the keyboard focus and
/// `inactive` when it does not, and a window that is minimised is `hidden`
/// either way. `paused` and `detached` are documented as Android/iOS-only —
/// `paused` means the platform has stopped asking for frames — so a desktop
/// embedder never sends them.
///
/// The embedder does not have to walk the state machine one step at a time:
/// `ServicesBinding._generateStateTransitions` fills in the states between the
/// one it last saw and the one it is handed, which is what makes jumping
/// straight from `resumed` to `hidden` on a minimise correct.
fn lifecycle_state(visible: bool, focused: bool) -> AppLifecycleState {
    if !visible {
        AppLifecycleState::Hidden
    } else if focused {
        AppLifecycleState::Resumed
    } else {
        AppLifecycleState::Inactive
    }
}

/// The window's visibility and focus, and the last lifecycle state they were
/// reported as.
///
/// GLFW reports visibility and focus in separate events, so each one has to be
/// combined with the last value of the other; and because two different window
/// states can map onto the same lifecycle state — an unfocused minimised window
/// and a focused one are both `hidden` — the state last sent is remembered so
/// the channel does not carry a transition the framework would ignore.
#[derive(Debug)]
struct LifecycleTracker {
    visible: bool,
    focused: bool,
    sent: Option<AppLifecycleState>,
}

impl LifecycleTracker {
    /// A tracker for a window that has not reported anything yet, so that the
    /// first update always sends.
    fn new() -> Self {
        Self {
            visible: true,
            focused: false,
            sent: None,
        }
    }

    /// Record both halves of the window state, returning the lifecycle state
    /// to send if it is not the one already sent.
    fn update(&mut self, visible: bool, focused: bool) -> Option<AppLifecycleState> {
        self.visible = visible;
        self.focused = focused;
        let state = lifecycle_state(visible, focused);
        if self.sent == Some(state) {
            return None;
        }
        self.sent = Some(state);
        Some(state)
    }

    /// Record that the window gained or lost the keyboard focus.
    fn set_focused(&mut self, focused: bool) -> Option<AppLifecycleState> {
        self.update(self.visible, focused)
    }

    /// Record that the window appeared on, or disappeared from, the screen.
    fn set_visible(&mut self, visible: bool) -> Option<AppLifecycleState> {
        self.update(visible, self.focused)
    }
}

/// The keys GLFW has reported as held, as the modern key protocol's
/// `(physical, logical)` id pairs.
///
/// GLFW has no synchronous "which keys are down?" query, so `flutter/keyboard`'s
/// `getKeyboardState` is answered out of this table, which
/// [`FlutterWindow::send_key_event`] maintains from the very events it
/// translates — which is also what keeps the ids reported here identical to the
/// ones the framework saw.
#[derive(Debug, Default)]
struct PressedKeys(HashMap<u64, u64>);

impl PressedKeys {
    /// Record a key going down or auto-repeating. A repeat re-inserts the same
    /// pair, so the table recovers if a down event was ever missed.
    fn press(&mut self, physical: u64, logical: u64) {
        self.0.insert(physical, logical);
    }

    /// Record a key coming up.
    fn release(&mut self, physical: u64) {
        self.0.remove(&physical);
    }

    /// Empty the table, handing back everything that was in it: the keys whose
    /// `Up` the embedder now owes the framework.
    fn release_all(&mut self) -> Vec<(u64, u64)> {
        self.0.drain().collect()
    }
}

impl KeyboardStateHandler for PressedKeys {
    fn pressed_keys(&mut self) -> Vec<(u64, u64)> {
        self.0
            .iter()
            .map(|(&physical, &logical)| (physical, logical))
            .collect()
    }
}

/// A key event GLFW has reported, on its way to the engine.
#[derive(Clone, Debug, Eq, PartialEq)]
struct StagedKey {
    kind: KeyEventType,
    physical: u64,
    logical: u64,
    /// The text the key produces, which `KeyEvent.character` carries. `None`
    /// for a key that produces none — a modifier, an arrow, a function key —
    /// and for every up event, where the engine ignores it.
    character: Option<String>,
}

/// Holds a key event back until the text it produces is known.
///
/// GLFW reports that text in a `Char` event that arrives *after* the `Key`
/// event, so a key event sent the moment it is seen can never carry a
/// `character`. The staged event is released by the next `Char` — which fills
/// it in — or by the next event of any other kind, or at the end of the batch
/// of events the loop is draining, so it is never delayed past the frame it
/// belongs to.
#[derive(Debug, Default)]
struct KeyStager {
    pending: Option<StagedKey>,
}

impl KeyStager {
    /// Hold `key` back, returning whatever it displaces — which has to be sent
    /// as it is, because no text is coming for it any more.
    fn stage(&mut self, key: StagedKey) -> Option<StagedKey> {
        self.pending.replace(key)
    }

    /// Release the staged event with the text GLFW has just reported.
    ///
    /// A key that produces more than one `Char` — a dead key resolving, a
    /// composed sequence — only carries the first into its key event; there is
    /// nothing staged by the time the rest arrive.
    fn take_with_character(&mut self, character: char) -> Option<StagedKey> {
        self.pending.take().map(|mut key| {
            key.character = Some(character.to_string());
            key
        })
    }

    /// Release the staged event with no text.
    fn take(&mut self) -> Option<StagedKey> {
        self.pending.take()
    }
}

/// Turn `dark-light`'s reading of the host into the framework's brightness.
#[cfg(feature = "brightness")]
fn brightness_from(mode: dark_light::Mode) -> PlatformBrightness {
    match mode {
        dark_light::Mode::Dark => PlatformBrightness::Dark,
        dark_light::Mode::Light => PlatformBrightness::Light,
        // The host has no preference, so the framework's own default is as
        // correct as anything this crate could make up.
        dark_light::Mode::Unspecified => PlatformBrightness::default(),
    }
}

/// The host's light/dark preference, or the framework's default when the host
/// has no opinion or cannot be asked.
///
/// `dark-light` reads the XDG desktop portal's `org.freedesktop.appearance`
/// `color-scheme` on Linux and the BSDs, the registry on Windows and
/// `NSUserDefaults` on macOS. All of those can legitimately fail — no session
/// bus, no portal, a sandbox without the permission — which is not an embedder
/// error: the framework's `Brightness.light` default is then exactly as right
/// as it would be if this crate never asked.
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

/// The framework's default, because without the `brightness` feature the host
/// is never asked.
///
/// The settings message is still sent: the framework re-reads all three values
/// out of every message, so sending the default is what keeps the other two
/// honest.
#[cfg(not(feature = "brightness"))]
fn detect_platform_brightness() -> PlatformBrightness {
    PlatformBrightness::default()
}

/// The cumulative mask of the buttons `tracked` says are down.
///
/// Button 4 maps to nothing: it is consumed as the "back" gesture and never
/// enters the tracker, so it cannot reach the engine as half of a
/// `kDown`/`kUp` pair. The header reserves `1 << 5` and up for the sixth
/// button onwards.
fn pressed_buttons(tracked: &HashMap<glfw::MouseButton, glfw::Action>) -> PointerButtons {
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

/// Whether a cursor movement is a drag or a hover. *Any* held button makes it
/// a drag; the left one is not special.
fn move_or_hover(buttons: PointerButtons) -> FlutterPointerPhase {
    if buttons.is_empty() {
        FlutterPointerPhase::Hover
    } else {
        FlutterPointerPhase::Move
    }
}

/// How many physical pixels one screen coordinate is worth, which every
/// pointer coordinate is multiplied by.
///
/// An iconified window can report a zero size, and `0 / 0` would poison the
/// ratio with a NaN that every later pointer coordinate inherits — it is only
/// recomputed on the next metrics event — so a degenerate size falls back to
/// 1.0 instead.
fn pixels_per_screen_coordinate(window_width: i32, framebuffer_width: i32) -> f64 {
    if window_width > 0 && framebuffer_width > 0 {
        f64::from(framebuffer_width) / f64::from(window_width)
    } else {
        1.0
    }
}

impl FlutterWindow {
    pub(crate) fn create(
        glfw: &mut glfw::Glfw,
        window_args: &WindowArgs,
        assets_path: PathBuf,
        icu_data_path: Option<PathBuf>,
        arguments: Vec<String>,
    ) -> Result<Self, CreateError> {
        glfw.window_hint(glfw::WindowHint::ContextVersion(3, 2));
        glfw.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
        glfw.window_hint(glfw::WindowHint::OpenGlProfile(
            glfw::OpenGlProfileHint::Core,
        ));
        glfw.window_hint(glfw::WindowHint::ContextCreationApi(
            glfw::ContextCreationApi::Egl,
        ));

        // Create window
        let (mut window, receiver) = match window_args.mode {
            WindowMode::Windowed => glfw
                .create_window(
                    window_args.width,
                    window_args.height,
                    window_args.title,
                    glfw::WindowMode::Windowed,
                )
                .ok_or(CreateError::WindowCreationFailed)?,
            WindowMode::Borderless => {
                glfw.window_hint(glfw::WindowHint::Decorated(false));
                glfw.create_window(
                    window_args.width,
                    window_args.height,
                    window_args.title,
                    glfw::WindowMode::Windowed,
                )
                .ok_or(CreateError::WindowCreationFailed)?
            }
            WindowMode::Fullscreen(index) => {
                glfw.with_connected_monitors(|glfw, monitors| -> Result<_, CreateError> {
                    let monitor = monitors.get(index).ok_or(CreateError::MonitorNotFound)?;
                    glfw.create_window(
                        window_args.width,
                        window_args.height,
                        window_args.title,
                        glfw::WindowMode::FullScreen(monitor),
                    )
                    .ok_or(CreateError::WindowCreationFailed)
                })?
            }
        };

        // Create invisible resource window
        glfw.window_hint(glfw::WindowHint::Decorated(false));
        glfw.window_hint(glfw::WindowHint::Visible(false));
        let (mut res_window, res_window_recv) = window
            .create_shared(1, 1, "", glfw::WindowMode::Windowed)
            .ok_or(CreateError::WindowCreationFailed)?;
        glfw.default_window_hints();

        let render_ctx = window.render_context();

        // Wrap
        let window = Arc::new(Mutex::new(window));

        // Create engine
        let platform_task_handler = Arc::new(GlfwPlatformTaskHandler::new());
        let opengl_handler = GlfwOpenGLHandler::new(render_ctx, res_window.render_context());

        let mut engine_builder = FlutterEngineBuilder::new()
            .with_platform_handler(platform_task_handler.clone())
            .with_opengl(opengl_handler)
            .with_asset_path(assets_path)
            .with_args(arguments);
        if let Some(icu_data_path) = icu_data_path {
            engine_builder = engine_builder.with_icu_data_path(icu_data_path);
        }
        let engine = engine_builder.build()?;

        // Main thread callbacks
        let (main_tx, main_rx) = mpsc::channel();

        // Register plugins
        let platform_handler = Arc::new(Mutex::new(GlfwPlatformHandler {
            window: window.clone(),
        }));
        let window_handler: Arc<Mutex<GlfwWindowHandler>> =
            Arc::new(Mutex::new(GlfwWindowHandler::new(window.clone())));
        let textinput_handler = Arc::new(Mutex::new(GlfwTextInputHandler::default()));
        let mousecursor_handler = Arc::new(Mutex::new(GlfwMouseCursorHandler::new(window.clone())));
        let pressed_keys = Arc::new(Mutex::new(PressedKeys::default()));

        let mut plugins = PluginRegistrar::new();
        plugins.add_plugin(&engine, AccessibilityPlugin::default());
        plugins.add_plugin(&engine, DialogPlugin::default());
        plugins.add_plugin(&engine, IsolatePlugin::new_stub());
        // The same table the event loop maintains, so `getKeyboardState`
        // answers with the ids the framework has already seen.
        plugins.add_plugin(&engine, KeyboardPlugin::new(pressed_keys.clone()));
        plugins.add_plugin(&engine, LifecyclePlugin::default());
        plugins.add_plugin(&engine, LocalizationPlugin::default());
        plugins.add_plugin(&engine, MouseCursorPlugin::new(mousecursor_handler));
        plugins.add_plugin(&engine, NavigationPlugin::default());
        plugins.add_plugin(&engine, PlatformPlugin::new(platform_handler));
        plugins.add_plugin(&engine, SettingsPlugin::default());
        plugins.add_plugin(&engine, SystemPlugin::default());
        plugins.add_plugin(&engine, TextInputPlugin::new(textinput_handler));
        plugins.add_plugin(&engine, WindowPlugin::new(window_handler.clone()));

        Ok(Self {
            glfw: glfw.clone(),
            window,
            window_receiver: receiver,
            _resource_window: res_window,
            _resource_window_receiver: res_window_recv,
            engine,
            pointer_currently_added: AtomicBool::new(false),
            // 1.0 until the first metrics event: a 0.0 ratio would collapse
            // every pointer coordinate sent before then onto (0, 0).
            window_pixels_per_screen_coordinate: AtomicU64::new(1.0_f64.to_bits()),
            main_thread_receiver: main_rx,
            main_thread_sender: main_tx,
            mouse_tracker: Mutex::new(Default::default()),
            key_stager: Mutex::new(KeyStager::default()),
            pressed_keys,
            lifecycle: Mutex::new(LifecycleTracker::new()),
            brightness_watcher_started: AtomicBool::new(false),
            window_handler,
            platform_task_handler,
            plugins: RwLock::new(plugins),
        })
    }

    /// A handle to the engine rendering into this window.
    #[must_use]
    pub fn engine(&self) -> FlutterEngine {
        self.engine.clone()
    }

    /// The underlying GLFW window, for the parts of GLFW this crate does not
    /// wrap. The lock is held by the event loop only while it translates an
    /// event.
    #[must_use]
    pub fn window(&self) -> Arc<Mutex<glfw::PWindow>> {
        self.window.clone()
    }

    /// Register an external texture the Flutter app can draw with.
    #[must_use]
    pub fn create_texture(&self) -> Texture {
        self.engine.create_texture()
    }

    /// Register an additional plugin on this window's engine.
    pub fn add_plugin<P>(&self, plugin: P) -> &Self
    where
        P: Plugin + 'static,
    {
        self.plugins.write().add_plugin(&self.engine, plugin);
        self
    }

    /// Run `f` against a registered plugin of type `P`, if one is registered.
    pub fn with_plugin<F, P>(&self, f: F)
    where
        F: FnOnce(&P),
        P: Plugin + 'static,
    {
        self.plugins.read().with_plugin(f)
    }

    /// Run `f` against a registered plugin of type `P` mutably, if one is
    /// registered.
    pub fn with_plugin_mut<F, P>(&self, f: F)
    where
        F: FnOnce(&mut P),
        P: Plugin + 'static,
    {
        self.plugins.write().with_plugin_mut(f)
    }

    /// Register a platform channel on this window's engine. The engine owns
    /// the channel; the returned handle does not keep it alive.
    pub fn register_channel<C>(&self, channel: C) -> Weak<C>
    where
        C: Channel + 'static,
    {
        self.engine.register_channel(channel)
    }

    /// Unregister a platform channel, returning it if it was registered.
    pub fn remove_channel(&self, channel_name: &str) -> Option<Arc<dyn Channel>> {
        self.engine.remove_channel(channel_name)
    }

    /// Run `f` against a registered platform channel, if one is registered
    /// under that name.
    pub fn with_channel<F>(&self, channel_name: &str, f: F)
    where
        F: FnMut(&dyn Channel),
    {
        self.engine.with_channel(channel_name, f)
    }

    /// Start the engine and run the GLFW event loop until the window is
    /// closed, translating every GLFW event into the engine's pointer, key and
    /// metrics events.
    ///
    /// `custom_handler` sees each event first and returns whether the default
    /// translation should still run; `frame_callback` runs once per loop
    /// iteration.
    ///
    /// # Errors
    ///
    /// Returns [`RunError`] if the engine refused to start, in which case no
    /// event loop is entered.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the one that created the
    /// window, which is the engine's platform thread.
    pub fn run(
        &self,
        mut custom_handler: Option<&mut WindowEventHandler>,
        mut frame_callback: Option<&mut PerFrameCallback>,
    ) -> Result<(), RunError> {
        // Start engine
        self.engine.run()?;

        // send initial size callback to engine
        self.send_scale_or_size_change();

        // enable event polling
        {
            let mut window = self.window.lock();
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
            // Both halves of the lifecycle state: focus decides between
            // `resumed` and `inactive`, iconification between those and
            // `hidden`.
            window.set_focus_polling(true);
            window.set_iconify_polling(true);
        }

        self.with_plugin(
            |localization: &flutter_plugins::localization::LocalizationPlugin| {
                localization.send_locales(sys_locale::get_locales());
            },
        );

        // The framework's lifecycle state stays `null` until something sends on
        // `flutter/lifecycle`, so report where the window actually starts
        // rather than waiting for the first focus change.
        let (visible, focused) = {
            let window = self.window.lock();
            (
                window.is_visible() && !window.is_iconified(),
                window.is_focused(),
            )
        };
        self.send_lifecycle(|tracker| tracker.update(visible, focused));

        // Likewise for the user-interface settings: without a message the
        // framework keeps `platformBrightness: light` for the whole run.
        self.send_settings(detect_platform_brightness());
        self.watch_platform_brightness();

        let mut glfw = self.glfw.clone();
        while !self.window.lock().should_close() {
            // Execute tasks and callbacks
            let next_task_time = self.engine.execute_platform_tasks();

            let callbacks: Vec<MainTheadFn> = self.main_thread_receiver.try_iter().collect();
            for mut cb in callbacks {
                cb(self);
            }

            // Sleep for events/till next task
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

            // Fetch events
            let events: Vec<(f64, glfw::WindowEvent)> =
                glfw::flush_messages(&self.window_receiver).collect();
            for (_, event) in events {
                let run_default_handler = if let Some(custom_handler) = &mut custom_handler {
                    custom_handler(self, event.clone())
                } else if let glfw::WindowEvent::CursorPos(x, y) = event {
                    self.window_handler.lock().drag_window(x, y)
                } else {
                    true
                };
                if run_default_handler {
                    self.handle_glfw_event(event);
                }
            }

            // No more `Char` events are coming this time round, so a key event
            // still waiting for one produces no text. Flushing here is what
            // keeps the delay bounded by the batch rather than by the next
            // event, which `wait_events` may not deliver for minutes.
            self.flush_staged_key();

            if let Some(callback) = &mut frame_callback {
                callback(self);
            }
        }

        Ok(())
    }

    /// Queue `f` to run on the platform thread inside the event loop, and
    /// wake the loop so it runs promptly. Safe to call from any thread.
    ///
    /// # Errors
    ///
    /// Returns the callback back if the event loop has already stopped.
    pub fn post_main_thread_callback<F>(&self, f: F) -> Result<(), SendError<MainTheadFn>>
    where
        F: FnMut(&FlutterWindow) + Send + 'static,
    {
        self.main_thread_sender.send(Box::new(f))?;
        self.platform_task_handler.wake();
        Ok(())
    }

    /// Shut the engine down and release the window.
    ///
    /// Dropping the `FlutterWindow` does the same thing; this is the explicit
    /// form, and the only one that can report nothing was left running.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the engine's platform thread.
    pub fn shutdown(self) {
        self.engine.shutdown();
    }

    fn send_scale_or_size_change(&self) {
        let window = self.window.lock();
        let window_size = window.get_size();
        let framebuffer_size = window.get_framebuffer_size();
        let scale = window.get_content_scale();
        self.window_pixels_per_screen_coordinate.store(
            pixels_per_screen_coordinate(window_size.0, framebuffer_size.0).to_bits(),
            Ordering::Relaxed,
        );
        debug!(
            "Setting framebuffer size to {:?}, scale to {}",
            framebuffer_size, scale.0
        );
        // GLFW never reports a negative size, but `as usize` would turn one
        // into a near-`usize::MAX` dimension rather than a visible error.
        self.engine.send_window_metrics_event(
            framebuffer_size.0.max(0) as usize,
            framebuffer_size.1.max(0) as usize,
            f64::from(scale.0),
        );
    }

    /// The cumulative mask of mouse buttons GLFW has reported as down.
    ///
    /// Because `device_kind` is always [`FlutterPointerDeviceKind::Mouse`], the
    /// embedder API requires this mask on *every* pointer event, not just the
    /// button that changed.
    ///
    /// Takes the `mouse_tracker` lock, so it must not be called while a guard
    /// on it is alive — `parking_lot::Mutex` is not reentrant.
    fn pressed_buttons(&self) -> PointerButtons {
        pressed_buttons(&self.mouse_tracker.lock())
    }

    fn send_pointer_event(
        &self,
        phase: FlutterPointerPhase,
        (x, y): (f64, f64),
        signal_kind: FlutterPointerSignalKind,
        (scroll_delta_x, scroll_delta_y): (f64, f64),
        buttons: PointerButtons,
    ) {
        if !self.pointer_currently_added.load(Ordering::Relaxed)
            && phase != FlutterPointerPhase::Add
            && phase != FlutterPointerPhase::Remove
        {
            // A pointer that has only just appeared holds no button yet,
            // whatever the event that forced this synthetic Add.
            self.send_pointer_event(
                FlutterPointerPhase::Add,
                (x, y),
                FlutterPointerSignalKind::None,
                (0.0, 0.0),
                PointerButtons::NONE,
            );
        }
        if self.pointer_currently_added.load(Ordering::Relaxed) && phase == FlutterPointerPhase::Add
            || !self.pointer_currently_added.load(Ordering::Relaxed)
                && phase == FlutterPointerPhase::Remove
        {
            return;
        }

        let window_pixels_per_screen_coordinate = f64::from_bits(
            self.window_pixels_per_screen_coordinate
                .load(Ordering::Relaxed),
        );
        self.engine.send_pointer_event(
            0,
            phase,
            (
                x * window_pixels_per_screen_coordinate,
                y * window_pixels_per_screen_coordinate,
            ),
            signal_kind,
            (
                scroll_delta_x * window_pixels_per_screen_coordinate,
                scroll_delta_y * window_pixels_per_screen_coordinate,
            ),
            FlutterPointerDeviceKind::Mouse,
            buttons,
            None,
        );

        match phase {
            FlutterPointerPhase::Add => self.pointer_currently_added.store(true, Ordering::Relaxed),
            FlutterPointerPhase::Remove => {
                self.pointer_currently_added.store(false, Ordering::Relaxed)
            }
            _ => {}
        }
    }

    /// Publish the host's user-interface settings on `flutter/settings`.
    ///
    /// Nothing asks for these: the framework keeps its built-in defaults
    /// (`textScaleFactor` 1.0, `alwaysUse24HourFormat` false,
    /// `platformBrightness` light) until the embedder sends a message, and
    /// re-reads all three out of every message it gets. See
    /// [`TEXT_SCALE_FACTOR`] and [`USE_24_HOUR_FORMAT`] for why only the
    /// brightness is sourced from the host.
    fn send_settings(&self, brightness: PlatformBrightness) {
        self.with_plugin(|settings: &SettingsPlugin| {
            settings
                .start_message(TEXT_SCALE_FACTOR, USE_24_HOUR_FORMAT, brightness)
                .send();
        });
    }

    /// Follow the host's light/dark preference for the rest of the run.
    ///
    /// `dark-light`'s watcher blocks on a platform notification — the XDG
    /// portal's `SettingChanged` signal on Linux — so it gets a thread of its
    /// own, and the settings message is posted back onto the platform thread,
    /// which is the only one allowed to send on a channel. The thread holds
    /// nothing but a clone of the callback sender and the loop's waker, so it
    /// outlives the window harmlessly and stops at the next change after the
    /// receiver is gone.
    ///
    /// A host that cannot be watched — no session bus, no portal, a platform
    /// `dark-light` has no watcher for — only costs the app live updates; the
    /// brightness sent at startup stands.
    fn watch_platform_brightness(&self) {
        // `run` can be entered again after the loop has exited, and a second
        // watcher would be a second thread sending the same message.
        if self
            .brightness_watcher_started
            .swap(true, Ordering::Relaxed)
        {
            return;
        }

        self.spawn_brightness_watcher();
    }

    /// Start the thread [`watch_platform_brightness`] describes.
    ///
    /// [`watch_platform_brightness`]: Self::watch_platform_brightness
    #[cfg(feature = "brightness")]
    fn spawn_brightness_watcher(&self) {
        let watcher = match dark_light::subscribe() {
            Ok(watcher) => watcher,
            Err(error) => {
                debug!("not following the host's light/dark preference: {error}");
                return;
            }
        };

        let sender = self.main_thread_sender.clone();
        let waker = self.platform_task_handler.clone();
        let spawned = std::thread::Builder::new()
            .name("fl-brightness".to_owned())
            .spawn(move || {
                while let Ok(mode) = watcher.recv() {
                    let brightness = brightness_from(mode);
                    debug!("the host switched to {} mode", brightness.as_str());
                    let callback: MainTheadFn =
                        Box::new(move |window: &FlutterWindow| window.send_settings(brightness));
                    if sender.send(callback).is_err() {
                        // The event loop is gone, and with it the only thread
                        // allowed to send the message.
                        break;
                    }
                    waker.wake();
                }
            });
        if let Err(error) = spawned {
            warn!("could not start the brightness watcher thread: {error}");
        }
    }

    /// Without the `brightness` feature there is no host preference to follow,
    /// so the message sent at startup is the only one.
    #[cfg(not(feature = "brightness"))]
    fn spawn_brightness_watcher(&self) {}

    /// Push the lifecycle state `f` produces to the framework.
    ///
    /// `f` returns `None` when the window state maps onto the state already
    /// sent, which is not worth a channel message.
    fn send_lifecycle<F>(&self, f: F)
    where
        F: FnOnce(&mut LifecycleTracker) -> Option<AppLifecycleState>,
    {
        // Resolve the new state before touching the plugin registry, so the
        // tracker's lock is never held across a channel send.
        let state = f(&mut self.lifecycle.lock());
        if let Some(state) = state {
            self.with_plugin(|lifecycle: &LifecyclePlugin| lifecycle.send_state(state));
        }
    }

    fn send_staged_key(&self, key: StagedKey) {
        self.engine.send_key_event(
            key.kind,
            key.physical,
            key.logical,
            key.character.as_deref(),
        );
    }

    /// Send the key event that is waiting for its text, with none.
    fn flush_staged_key(&self) {
        // Take it out from under the lock first: the send reaches the engine,
        // and `parking_lot::Mutex` is not reentrant.
        let staged = self.key_stager.lock().take();
        if let Some(key) = staged {
            self.send_staged_key(key);
        }
    }

    /// Tell the framework that every key it believes is held has come up.
    ///
    /// GLFW stops reporting a key the moment the window loses the keyboard, so
    /// a key released while another window has focus never produces an `Up` —
    /// and `HardwareKeyboard` would go on believing it is held for the rest of
    /// the run, breaking every shortcut that key takes part in.
    ///
    /// These go out as ordinary up events rather than synthesised ones, which
    /// the embedder API has a flag for but
    /// [`FlutterEngine::send_key_event`](flutter_engine::FlutterEngine::send_key_event)
    /// does not expose: the framework dispatches them to the app's handlers,
    /// which is what an app watching a key-and-hold gesture needs to see.
    fn release_all_keys(&self) {
        let held = self.pressed_keys.lock().release_all();
        for (physical, logical) in held {
            debug!("synthesising a key up for {physical:#x} after losing the keyboard");
            self.engine
                .send_key_event(KeyEventType::Up, physical, logical, None);
        }
    }

    /// Translate a GLFW key event into the modern Flutter key protocol
    /// (FlutterKeyEvent), which drives HardwareKeyboard, KeyboardListener and
    /// shortcut/Actions handling. Keys without a Flutter mapping are dropped.
    ///
    /// A down or repeat event is staged rather than sent, because the text it
    /// produces only arrives in the `Char` event that follows it; see
    /// [`KeyStager`]. An up event is sent straight away — the engine ignores
    /// `character` on one.
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
                let displaced = self.key_stager.lock().stage(StagedKey {
                    kind,
                    physical,
                    logical,
                    character: None,
                });
                // Only reachable if something bypassed `handle_glfw_event`,
                // which flushes before every event that is not a `Char`.
                if let Some(key) = displaced {
                    self.send_staged_key(key);
                }
            }
            KeyEventType::Up => {
                self.pressed_keys.lock().release(physical);
                self.send_staged_key(StagedKey {
                    kind,
                    physical,
                    logical,
                    character: None,
                });
            }
        }
    }

    /// Apply this crate's default translation of a GLFW event into engine
    /// events. [`Self::run`] calls this unless a [`WindowEventHandler`] has
    /// claimed the event.
    ///
    /// A key press is held back here until the `Char` event that carries the
    /// text it produces, so a [`WindowEventHandler`] that claims a `Char`
    /// event costs the key event before it its `character` — the key event
    /// itself still goes out, without one, at the next event or at the end of
    /// the batch.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    pub fn handle_glfw_event(&self, event: glfw::WindowEvent) {
        // A staged key event is waiting for the `Char` that carries its text.
        // Any other event means none is coming, and flushing here is what
        // keeps the engine seeing key events in the order GLFW reported them.
        if !matches!(event, glfw::WindowEvent::Char(_)) {
            self.flush_staged_key();
        }

        match event {
            glfw::WindowEvent::Refresh => {
                self.send_scale_or_size_change();
            }
            glfw::WindowEvent::CursorEnter(entered) => {
                let cursor_pos = self.window.lock().get_cursor_pos();
                self.send_pointer_event(
                    if entered {
                        FlutterPointerPhase::Add
                    } else {
                        FlutterPointerPhase::Remove
                    },
                    (cursor_pos.0, cursor_pos.1),
                    FlutterPointerSignalKind::None,
                    (0.0, 0.0),
                    PointerButtons::NONE,
                );
            }
            glfw::WindowEvent::CursorPos(x, y) => {
                // fix error when dragging cursor out of a window
                if !self.pointer_currently_added.load(Ordering::Relaxed) {
                    return;
                }
                let buttons = self.pressed_buttons();
                self.send_pointer_event(
                    move_or_hover(buttons),
                    (x, y),
                    FlutterPointerSignalKind::None,
                    (0.0, 0.0),
                    buttons,
                );
            }
            // Button 4 is consumed as the Android-style "back" gesture. It is
            // deliberately kept out of the pointer stream entirely — including
            // out of `mouse_tracker`, so it never shows up in
            // `pressed_buttons` — because sending only the release half of the
            // pair used to hand the engine a `kUp` for a pointer that was
            // never `kDown`.
            glfw::WindowEvent::MouseButton(glfw::MouseButton::Button4, action, _modifiers) => {
                if action == glfw::Action::Press {
                    self.with_plugin(
                        |navigation: &flutter_plugins::navigation::NavigationPlugin| {
                            navigation.pop_route();
                        },
                    );
                }
            }
            glfw::WindowEvent::MouseButton(button, action, _modifiers) => {
                // Since Events are delayed by wait_events_timeout,
                // it's not accurate to use get_mouse_button API to fetch current mouse state
                // Here we save mouse states, and query it in this HashMap
                self.mouse_tracker.lock().insert(button, action);

                // fix error when keeping primary button down
                // and alt+tab away from the window and release
                if !self.pointer_currently_added.load(Ordering::Relaxed) {
                    return;
                }

                let (x, y) = self.window.lock().get_cursor_pos();
                let phase = if action == glfw::Action::Press {
                    FlutterPointerPhase::Down
                } else {
                    FlutterPointerPhase::Up
                };
                // The tracker was updated above, so the mask already includes a
                // pressed button and already excludes a released one, which is
                // the state the engine expects on each phase.
                self.send_pointer_event(
                    phase,
                    (x, y),
                    FlutterPointerSignalKind::None,
                    (0.0, 0.0),
                    self.pressed_buttons(),
                );
            }
            glfw::WindowEvent::Scroll(scroll_delta_x, scroll_delta_y) => {
                let (x, y) = self.window.lock().get_cursor_pos();
                let buttons = self.pressed_buttons();
                self.send_pointer_event(
                    move_or_hover(buttons),
                    (x, y),
                    FlutterPointerSignalKind::Scroll,
                    (
                        scroll_delta_x * SCROLL_SPEED,
                        -scroll_delta_y * SCROLL_SPEED,
                    ),
                    buttons,
                );
            }
            glfw::WindowEvent::FramebufferSize(_, _) => {
                self.send_scale_or_size_change();
            }
            glfw::WindowEvent::ContentScale(_, _) => {
                self.send_scale_or_size_change();
            }
            // GLFW's text goes to two consumers, in this order.
            //
            // First it completes the key event staged by the `Key` event that
            // preceded it, which is what fills in `KeyEvent.character` for
            // `HardwareKeyboard`, `KeyboardListener` and character-based
            // shortcuts.
            //
            // Then it goes to the legacy in-embedder editing path, which is
            // what actually types into a focused Dart `TextField`: that field
            // is driven entirely by `flutter/textinput`, and the engine never
            // inserts a key event's `character` into one.
            //
            // So the two are not alternatives and cannot double up — the text
            // reaches the framework exactly once, through the text input
            // channel — while dropping either one loses a feature: no
            // `character` on key events, or no typing at all.
            glfw::WindowEvent::Char(char) => {
                let staged = self.key_stager.lock().take_with_character(char);
                if let Some(key) = staged {
                    self.send_staged_key(key);
                }
                self.with_plugin_mut(
                    |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.add_characters(&char.to_string());
                        });
                        text_input.notify_changes();
                    },
                );
            }
            glfw::WindowEvent::Focus(focused) => {
                if !focused {
                    // Do this before the lifecycle message, so the framework's
                    // keyboard state is consistent by the time the app's
                    // `didChangeAppLifecycleState` runs.
                    self.release_all_keys();
                }
                self.send_lifecycle(|tracker| tracker.set_focused(focused));
            }
            glfw::WindowEvent::Iconify(iconified) => {
                self.send_lifecycle(|tracker| tracker.set_visible(!iconified));
            }
            glfw::WindowEvent::Key(
                key,
                _scancode,
                action @ (glfw::Action::Press | glfw::Action::Repeat),
                modifiers,
            ) => {
                // TODO: move this to TextInputPlugin
                match key {
                    glfw::Key::Enter => self.with_plugin_mut(
                        |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                            text_input.with_state(|state| {
                                state.add_characters("\n");
                            });
                            text_input.notify_changes();
                        },
                    ),
                    glfw::Key::Up => self.with_plugin_mut(
                        |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                            text_input.with_state(|state| {
                                state.move_up(modifiers.contains(SELECT_MODIFIER_KEY));
                            });
                            text_input.notify_changes();
                        },
                    ),
                    glfw::Key::Down => self.with_plugin_mut(
                        |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                            text_input.with_state(|state| {
                                state.move_down(modifiers.contains(SELECT_MODIFIER_KEY));
                            });
                            text_input.notify_changes();
                        },
                    ),
                    glfw::Key::Backspace => self.with_plugin_mut(
                        |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                            text_input.with_state(|state| {
                                state.backspace();
                            });
                            text_input.notify_changes();
                        },
                    ),
                    glfw::Key::Delete => self.with_plugin_mut(
                        |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                            text_input.with_state(|state| {
                                state.delete();
                            });
                            text_input.notify_changes();
                        },
                    ),
                    glfw::Key::Left => self.with_plugin_mut(
                        |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                            text_input.with_state(|state| {
                                state.move_left(
                                    modifiers.contains(BY_WORD_MODIFIER_KEY),
                                    modifiers.contains(SELECT_MODIFIER_KEY),
                                );
                            });
                            text_input.notify_changes();
                        },
                    ),
                    glfw::Key::Right => self.with_plugin_mut(
                        |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                            text_input.with_state(|state| {
                                state.move_right(
                                    modifiers.contains(BY_WORD_MODIFIER_KEY),
                                    modifiers.contains(SELECT_MODIFIER_KEY),
                                );
                            });
                            text_input.notify_changes();
                        },
                    ),
                    glfw::Key::Home => self.with_plugin_mut(
                        |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                            text_input.with_state(|state| {
                                state.move_to_beginning(modifiers.contains(SELECT_MODIFIER_KEY));
                            });
                            text_input.notify_changes();
                        },
                    ),
                    glfw::Key::End => self.with_plugin_mut(
                        |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                            text_input.with_state(|state| {
                                state.move_to_end(modifiers.contains(SELECT_MODIFIER_KEY));
                            });
                            text_input.notify_changes();
                        },
                    ),
                    glfw::Key::A if modifiers.contains(FUNCTION_MODIFIER_KEY) => self
                        .with_plugin_mut(
                            |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                                text_input.with_state(|state| {
                                    state.select_all();
                                });
                                text_input.notify_changes();
                            },
                        ),
                    glfw::Key::X if modifiers.contains(FUNCTION_MODIFIER_KEY) => {
                        let mut window = self.window.lock();
                        self.with_plugin_mut(
                            |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                                text_input.with_state(|state| {
                                    window.set_clipboard_string(state.get_selected_text());
                                    state.delete_selected();
                                });
                                text_input.notify_changes();
                            },
                        )
                    }
                    glfw::Key::C if modifiers.contains(FUNCTION_MODIFIER_KEY) => {
                        let mut window = self.window.lock();
                        self.with_plugin_mut(
                            |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                                text_input.with_state(|state| {
                                    window.set_clipboard_string(state.get_selected_text());
                                });
                                text_input.notify_changes();
                            },
                        )
                    }
                    glfw::Key::V if modifiers.contains(FUNCTION_MODIFIER_KEY) => {
                        let window = self.window.lock();
                        self.with_plugin_mut(
                            |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                                text_input.with_state(|state| {
                                    if let Some(text) = window.get_clipboard_string() {
                                        state.add_characters(&text);
                                    } else {
                                        info!("Tried to paste non-text data");
                                    }
                                });
                                text_input.notify_changes();
                            },
                        )
                    }
                    _ => {}
                }

                self.send_key_event(key, action);
            }
            glfw::WindowEvent::Key(key, _scancode, glfw::Action::Release, _modifiers) => {
                self.send_key_event(key, glfw::Action::Release);
            }
            _ => {}
        }
    }
}

impl Drop for FlutterWindow {
    fn drop(&mut self) {
        // The engine's raster and IO threads make the GLFW contexts this
        // window owns current, so the engine has to be gone before they are
        // dropped. `FlutterEngine::shutdown` is idempotent, so an explicit
        // `shutdown()` leaves nothing for this to do; it also asserts it is on
        // the platform thread, which a destructor must not risk.
        if self.engine.is_shut_down() {
            return;
        }
        if self.engine.is_platform_thread() {
            self.engine.shutdown();
        } else {
            log::error!(
                "FlutterWindow dropped off the platform thread; the engine is still running. \
                 Call FlutterWindow::shutdown on the thread that created the window."
            );
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

    /// `device_kind` is always `Mouse`, so the engine reads `buttons` as the
    /// set of buttons held *now*. Reporting the one button that changed made
    /// plain hovering look like a left-button drag.
    #[test]
    fn hovering_reports_no_button() {
        assert_eq!(pressed_buttons(&tracker([])), PointerButtons::NONE);
        assert_eq!(
            pressed_buttons(&tracker([
                (glfw::MouseButtonLeft, glfw::Action::Release),
                (glfw::MouseButtonRight, glfw::Action::Release),
            ])),
            PointerButtons::NONE
        );
        assert_eq!(
            move_or_hover(PointerButtons::NONE),
            FlutterPointerPhase::Hover
        );
    }

    #[test]
    fn two_buttons_held_at_once_are_both_reported() {
        let held = pressed_buttons(&tracker([
            (glfw::MouseButtonLeft, glfw::Action::Press),
            (glfw::MouseButtonMiddle, glfw::Action::Press),
            (glfw::MouseButtonRight, glfw::Action::Release),
        ]));
        assert_eq!(held, PointerButtons::PRIMARY | PointerButtons::MIDDLE);
        assert_eq!(move_or_hover(held), FlutterPointerPhase::Move);
    }

    /// Dragging with any button is a `kMove`; before the fix only the left
    /// button counted, so a right-button drag arrived as a hover.
    #[test]
    fn any_held_button_makes_a_move() {
        for button in [
            glfw::MouseButtonLeft,
            glfw::MouseButtonRight,
            glfw::MouseButtonMiddle,
            glfw::MouseButton::Button5,
        ] {
            let held = pressed_buttons(&tracker([(button, glfw::Action::Press)]));
            assert!(!held.is_empty(), "{button:?}");
            assert_eq!(move_or_hover(held), FlutterPointerPhase::Move, "{button:?}");
        }
    }

    /// Button 4 is the "back" gesture: it pops a route and must never reach
    /// the engine as a pointer button, or the engine sees a `kUp` for a
    /// pointer that was never `kDown`.
    #[test]
    fn back_button_is_never_a_pointer_button() {
        for action in [glfw::Action::Press, glfw::Action::Release] {
            assert_eq!(
                pressed_buttons(&tracker([(glfw::MouseButton::Button4, action)])),
                PointerButtons::NONE,
                "{action:?}"
            );
        }
    }

    /// The header reserves `1 << 5` upwards for the sixth button onwards.
    #[test]
    fn buttons_map_onto_the_embedder_bits() {
        for (button, expected) in [
            (glfw::MouseButtonLeft, PointerButtons::PRIMARY),
            (glfw::MouseButtonRight, PointerButtons::SECONDARY),
            (glfw::MouseButtonMiddle, PointerButtons::MIDDLE),
            (glfw::MouseButton::Button5, PointerButtons::FORWARD),
            (
                glfw::MouseButton::Button6,
                PointerButtons::from_bits(1 << 5),
            ),
            (
                glfw::MouseButton::Button7,
                PointerButtons::from_bits(1 << 6),
            ),
            (
                glfw::MouseButton::Button8,
                PointerButtons::from_bits(1 << 7),
            ),
        ] {
            assert_eq!(
                pressed_buttons(&tracker([(button, glfw::Action::Press)])),
                expected,
                "{button:?}"
            );
        }
    }

    #[test]
    fn pixel_ratio_is_the_framebuffer_over_the_window() {
        assert_eq!(pixels_per_screen_coordinate(800, 1600), 2.0);
        assert_eq!(pixels_per_screen_coordinate(1280, 1280), 1.0);
    }

    /// A single zero-width report used to poison the ratio with a NaN for the
    /// rest of the session, collapsing every later pointer coordinate.
    #[test]
    fn degenerate_window_size_does_not_poison_the_pixel_ratio() {
        for (window, framebuffer) in [(0, 0), (0, 1600), (800, 0), (-1, 1600), (800, -1)] {
            let ratio = pixels_per_screen_coordinate(window, framebuffer);
            assert!(ratio.is_finite(), "{window}x{framebuffer} -> {ratio}");
            assert_eq!(ratio, 1.0, "{window}x{framebuffer}");
        }
    }

    /// `paused` means "the platform has stopped asking for frames", which a
    /// GLFW window never does, and `detached` is Android/iOS-only. A minimised
    /// window is `hidden` whether or not it still holds the focus.
    #[test]
    fn window_state_maps_onto_the_three_desktop_lifecycle_states() {
        assert_eq!(lifecycle_state(true, true), AppLifecycleState::Resumed);
        assert_eq!(lifecycle_state(true, false), AppLifecycleState::Inactive);
        assert_eq!(lifecycle_state(false, false), AppLifecycleState::Hidden);
        assert_eq!(lifecycle_state(false, true), AppLifecycleState::Hidden);
    }

    /// The same five transitions the official GTK embedder's
    /// `fl_window_state_monitor` tests pin, driven through GLFW's separate
    /// focus and iconify events.
    #[test]
    fn focus_and_iconify_produce_the_official_embedders_transitions() {
        let mut tracker = LifecycleTracker::new();
        assert_eq!(
            tracker.update(true, false),
            Some(AppLifecycleState::Inactive),
            "a window that starts unfocused"
        );
        assert_eq!(
            tracker.set_focused(true),
            Some(AppLifecycleState::Resumed),
            "gaining focus"
        );
        assert_eq!(
            tracker.set_focused(false),
            Some(AppLifecycleState::Inactive),
            "losing focus"
        );
        assert_eq!(
            tracker.set_visible(false),
            Some(AppLifecycleState::Hidden),
            "being iconified"
        );
        assert_eq!(
            tracker.set_visible(true),
            Some(AppLifecycleState::Inactive),
            "leaving iconified without focus"
        );

        // And the focused variant of the last one: GLFW reports the restore
        // and the focus as two events, so the tracker passes through
        // `inactive` on the way to `resumed`.
        assert_eq!(tracker.set_visible(false), Some(AppLifecycleState::Hidden));
        assert_eq!(tracker.set_focused(true), None, "still iconified");
        assert_eq!(
            tracker.set_visible(true),
            Some(AppLifecycleState::Resumed),
            "leaving iconified with focus"
        );
    }

    /// Two different window states can mean the same lifecycle state — an
    /// iconified window gaining focus is still `hidden` — and the framework
    /// ignores a repeat anyway, so it is not worth a channel message.
    #[test]
    fn an_unchanged_lifecycle_state_is_not_resent() {
        let mut tracker = LifecycleTracker::new();
        assert_eq!(tracker.update(true, true), Some(AppLifecycleState::Resumed));
        assert_eq!(tracker.update(true, true), None);
        assert_eq!(tracker.set_focused(true), None);
        assert_eq!(tracker.set_visible(false), Some(AppLifecycleState::Hidden));
        assert_eq!(tracker.set_focused(false), None, "hidden either way");
        assert_eq!(tracker.set_focused(true), None, "hidden either way");
    }

    /// `(physical, logical)` for `A` and for left Shift, from the keymap.
    const KEY_A: (u64, u64) = (0x0007_0004, 0x0000_0061);
    const LEFT_SHIFT: (u64, u64) = (0x0007_00e1, 0x0002_0000_0102);

    fn sorted(mut keys: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
        keys.sort_unstable();
        keys
    }

    #[test]
    fn pressed_keys_tracks_what_the_framework_believes_is_held() {
        let mut keys = PressedKeys::default();
        assert_eq!(keys.pressed_keys(), Vec::new());

        keys.press(KEY_A.0, KEY_A.1);
        keys.press(LEFT_SHIFT.0, LEFT_SHIFT.1);
        assert_eq!(sorted(keys.pressed_keys()), sorted(vec![KEY_A, LEFT_SHIFT]));

        keys.release(KEY_A.0);
        assert_eq!(keys.pressed_keys(), vec![LEFT_SHIFT]);
    }

    /// A repeat arrives as another press of a key already held; it must not
    /// turn into a second entry or a second logical id.
    #[test]
    fn repeating_a_held_key_does_not_duplicate_it() {
        let mut keys = PressedKeys::default();
        keys.press(KEY_A.0, KEY_A.1);
        keys.press(KEY_A.0, KEY_A.1);
        assert_eq!(keys.pressed_keys(), vec![KEY_A]);
        keys.release(KEY_A.0);
        assert_eq!(keys.pressed_keys(), Vec::new());
    }

    /// Releasing a key GLFW never reported down — which happens after the
    /// table was cleared on a focus loss — must not resurrect it.
    #[test]
    fn releasing_an_unheld_key_is_a_no_op() {
        let mut keys = PressedKeys::default();
        keys.release(KEY_A.0);
        assert_eq!(keys.pressed_keys(), Vec::new());
    }

    /// Losing the keyboard hands back every held key exactly once, so the
    /// embedder can synthesise the up events GLFW will never deliver, and
    /// leaves the table empty so they are not sent twice.
    #[test]
    fn losing_the_keyboard_hands_back_every_held_key_once() {
        let mut keys = PressedKeys::default();
        keys.press(KEY_A.0, KEY_A.1);
        keys.press(LEFT_SHIFT.0, LEFT_SHIFT.1);

        assert_eq!(sorted(keys.release_all()), sorted(vec![KEY_A, LEFT_SHIFT]));
        assert_eq!(keys.release_all(), Vec::new());
        assert_eq!(keys.pressed_keys(), Vec::new());
    }

    fn staged(kind: KeyEventType, (physical, logical): (u64, u64)) -> StagedKey {
        StagedKey {
            kind,
            physical,
            logical,
            character: None,
        }
    }

    /// The whole point of staging: GLFW reports the text after the key, so a
    /// key event sent immediately could never carry a `character`.
    #[test]
    fn a_staged_key_picks_up_the_character_that_follows_it() {
        let mut stager = KeyStager::default();
        assert_eq!(stager.stage(staged(KeyEventType::Down, KEY_A)), None);

        let key = stager
            .take_with_character('a')
            .expect("the staged key is released by the char");
        assert_eq!(key.character.as_deref(), Some("a"));
        assert_eq!((key.physical, key.logical), KEY_A);
        assert_eq!(key.kind, KeyEventType::Down);
    }

    /// A key that produces no text — a modifier, an arrow, a function key —
    /// is followed by the next event instead of a `Char`, and goes out with
    /// `character: None` rather than being dropped.
    #[test]
    fn a_key_with_no_text_is_released_without_a_character() {
        let mut stager = KeyStager::default();
        stager.stage(staged(KeyEventType::Down, LEFT_SHIFT));

        let key = stager.take().expect("flushed by the next event");
        assert_eq!(key.character, None);
        assert_eq!((key.physical, key.logical), LEFT_SHIFT);
    }

    /// Nothing is staged after a release, so the next `Char` — the second one
    /// of a composed sequence, say — must not attach itself to an event that
    /// has already gone out.
    #[test]
    fn a_character_with_nothing_staged_is_dropped() {
        let mut stager = KeyStager::default();
        assert_eq!(stager.take_with_character('a'), None);
        assert_eq!(stager.take(), None);

        stager.stage(staged(KeyEventType::Down, KEY_A));
        assert!(stager.take_with_character('a').is_some());
        assert_eq!(stager.take_with_character('b'), None, "already released");
    }

    /// Staging over a key that is still waiting hands the old one back so it
    /// can be sent first; dropping it would lose a key press entirely.
    #[test]
    fn staging_over_a_waiting_key_hands_it_back() {
        let mut stager = KeyStager::default();
        stager.stage(staged(KeyEventType::Down, LEFT_SHIFT));

        let displaced = stager
            .stage(staged(KeyEventType::Down, KEY_A))
            .expect("the first key is handed back");
        assert_eq!((displaced.physical, displaced.logical), LEFT_SHIFT);
        assert_eq!(displaced.character, None);
        assert_eq!(
            stager.take().map(|key| (key.physical, key.logical)),
            Some(KEY_A)
        );
    }

    /// A host with no light/dark preference gets the framework's default,
    /// which is what it would have used had the embedder sent nothing.
    #[cfg(feature = "brightness")]
    #[test]
    fn an_unspecified_host_preference_falls_back_to_the_frameworks_default() {
        assert_eq!(
            brightness_from(dark_light::Mode::Dark),
            PlatformBrightness::Dark
        );
        assert_eq!(
            brightness_from(dark_light::Mode::Light),
            PlatformBrightness::Light
        );
        assert_eq!(
            brightness_from(dark_light::Mode::Unspecified),
            PlatformBrightness::default()
        );
    }
}
