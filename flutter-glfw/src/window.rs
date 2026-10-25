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
use flutter_plugins::keyboard::KeyboardPlugin;
use flutter_plugins::lifecycle::LifecyclePlugin;
use flutter_plugins::localization::LocalizationPlugin;
use flutter_plugins::mousecursor::MouseCursorPlugin;
use flutter_plugins::navigation::NavigationPlugin;
use flutter_plugins::platform::PlatformPlugin;
use flutter_plugins::settings::SettingsPlugin;
use flutter_plugins::system::SystemPlugin;
use flutter_plugins::textinput::TextInputPlugin;
use flutter_plugins::window::WindowPlugin;
use log::{debug, info};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::sync::{Arc, Weak, mpsc};
use std::time::Instant;

// seems to be about 2.5 lines of text
const SCROLL_SPEED: f64 = 50.0;
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
    window_handler: Arc<Mutex<GlfwWindowHandler>>,
    platform_task_handler: Arc<GlfwPlatformTaskHandler>,
    plugins: RwLock<PluginRegistrar>,
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

        let mut plugins = PluginRegistrar::new();
        plugins.add_plugin(&engine, AccessibilityPlugin::default());
        plugins.add_plugin(&engine, DialogPlugin::default());
        plugins.add_plugin(&engine, IsolatePlugin::new_stub());
        plugins.add_plugin(&engine, KeyboardPlugin::default());
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
        }

        self.with_plugin(
            |localization: &flutter_plugins::localization::LocalizationPlugin| {
                localization.send_locales(sys_locale::get_locales());
            },
        );

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

    /// Translate a GLFW key event into the modern Flutter key protocol
    /// (FlutterKeyEvent), which drives HardwareKeyboard, KeyboardListener and
    /// shortcut/Actions handling. Keys without a Flutter mapping are dropped.
    fn send_key_event(&self, key: glfw::Key, action: glfw::Action) {
        let kind = match action {
            glfw::Action::Press => KeyEventType::Down,
            glfw::Action::Repeat => KeyEventType::Repeat,
            glfw::Action::Release => KeyEventType::Up,
        };
        let key = key as i32;
        if let Some((physical, logical)) = crate::keymap::glfw_key_to_flutter(key)
            .or_else(|| crate::keymap::glfw_key_to_flutter_extra(key))
        {
            self.engine.send_key_event(kind, physical, logical, None);
        }
    }

    /// Apply this crate's default translation of a GLFW event into engine
    /// events. [`Self::run`] calls this unless a [`WindowEventHandler`] has
    /// claimed the event.
    ///
    /// # Panics
    ///
    /// Panics if called from a thread other than the platform thread.
    pub fn handle_glfw_event(&self, event: glfw::WindowEvent) {
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
            glfw::WindowEvent::Char(char) => self.with_plugin_mut(
                |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                    text_input.with_state(|state| {
                        state.add_characters(&char.to_string());
                    });
                    text_input.notify_changes();
                },
            ),
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
}
