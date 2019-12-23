use crate::draw;
use crate::handler::{GlfwFlutterEngineHandler, GlfwPlatformHandler, GlfwWindowHandler};
use flutter_engine::channel::Channel;
use flutter_engine::ffi::{
    FlutterPointerDeviceKind, FlutterPointerMouseButtons, FlutterPointerPhase,
    FlutterPointerSignalKind,
};
use flutter_engine::plugins::Plugin;
use flutter_engine::texture_registry::{ExternalTexture, TextureRegistry};
use flutter_engine::{FlutterEngine, FlutterEngineHandler, RunError};
use flutter_plugins::dialog::DialogPlugin;
use flutter_plugins::isolate::IsolatePlugin;
use flutter_plugins::keyevent::{KeyAction, KeyActionType, KeyEventPlugin};
use flutter_plugins::lifecycle::LifecyclePlugin;
use flutter_plugins::localization::LocalizationPlugin;
use flutter_plugins::navigation::NavigationPlugin;
use flutter_plugins::platform::PlatformPlugin;
use flutter_plugins::settings::SettingsPlugin;
use flutter_plugins::system::SystemPlugin;
use flutter_plugins::textinput::TextInputPlugin;
use flutter_plugins::window::WindowPlugin;
use glfw::Context;
use lazy_static::lazy_static;
use log::{debug, info};
use parking_lot::{Mutex, MutexGuard};
use std::collections::{HashMap, VecDeque};
use std::ops::DerefMut;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::sync::{mpsc, Arc};
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

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum CreateError {
    WindowAlreadyCreated,
    WindowCreationFailed,
    MonitorNotFound,
}

impl std::fmt::Display for CreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use std::error::Error;
        f.write_str(self.description())
    }
}

impl std::error::Error for CreateError {
    fn description(&self) -> &str {
        match *self {
            CreateError::WindowCreationFailed => "Failed to create a window",
            CreateError::WindowAlreadyCreated => "Window was already created",
            CreateError::MonitorNotFound => "No monitor with the specified index found",
        }
    }
}

pub enum WindowMode {
    Fullscreen(usize),
    Windowed,
    Borderless,
}

pub struct WindowArgs<'a> {
    pub width: i32,
    pub height: i32,
    pub title: &'a str,
    pub mode: WindowMode,
    pub bg_color: (u8, u8, u8),
}

/// Wrap glfw::Window, so that it could be used in a lazy_static HashMap
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct WindowSafe(*mut glfw::ffi::GLFWwindow);

unsafe impl Send for WindowSafe {}

unsafe impl Sync for WindowSafe {}

// This HashMap is used to look up FlutterEngine using glfw Window
lazy_static! {
    static ref ENGINES: Mutex<HashMap<WindowSafe, FlutterEngine>> = Mutex::new(HashMap::new());
}

pub fn get_engine(window: *mut glfw::ffi::GLFWwindow) -> Option<FlutterEngine> {
    ENGINES.lock().get(&WindowSafe(window)).map(|v| v.clone())
}

pub(crate) type MainTheadFn = Box<dyn FnMut(&FlutterWindow) + Send>;
pub type WindowEventHandler = dyn FnMut(&FlutterWindow, glfw::WindowEvent) -> bool;
pub type PerFrameCallback = dyn FnMut(&FlutterWindow);

pub struct FlutterWindow {
    glfw: glfw::Glfw,
    window: Arc<Mutex<glfw::Window>>,
    window_receiver: Receiver<(f64, glfw::WindowEvent)>,
    resource_window: Arc<Mutex<glfw::Window>>,
    _resource_window_receiver: Receiver<(f64, glfw::WindowEvent)>,
    engine_handler: Arc<GlfwFlutterEngineHandler>,
    engine: FlutterEngine,
    pointer_currently_added: AtomicBool,
    window_pixels_per_screen_coordinate: AtomicU64,
    main_thread_receiver: Receiver<MainTheadFn>,
    main_thread_sender: Sender<MainTheadFn>,
    isolate_created: AtomicBool,
    defered_events: Mutex<VecDeque<glfw::WindowEvent>>,
    mouse_tracker: Mutex<HashMap<glfw::MouseButton, glfw::Action>>,
    texture_registry: Arc<Mutex<TextureRegistry>>,
    window_handler: Arc<Mutex<GlfwWindowHandler>>,
}

impl FlutterWindow {
    pub(crate) fn create(
        glfw: &mut glfw::Glfw,
        window_args: &WindowArgs,
    ) -> Result<Self, CreateError> {
        // Create window
        let (window, receiver) = match window_args.mode {
            WindowMode::Windowed => glfw
                .create_window(
                    window_args.width as u32,
                    window_args.height as u32,
                    window_args.title,
                    glfw::WindowMode::Windowed,
                )
                .ok_or(CreateError::WindowCreationFailed)?,
            WindowMode::Borderless => {
                glfw.window_hint(glfw::WindowHint::Decorated(false));
                glfw.create_window(
                    window_args.width as u32,
                    window_args.height as u32,
                    window_args.title,
                    glfw::WindowMode::Windowed,
                )
                .ok_or(CreateError::WindowCreationFailed)?
            }
            WindowMode::Fullscreen(index) => {
                glfw.with_connected_monitors(|glfw, monitors| -> Result<_, CreateError> {
                    let monitor = monitors.get(index).ok_or(CreateError::MonitorNotFound)?;
                    glfw.create_window(
                        window_args.width as u32,
                        window_args.height as u32,
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
        let (res_window, res_window_recv) = window
            .create_shared(1, 1, "", glfw::WindowMode::Windowed)
            .ok_or(CreateError::WindowCreationFailed)?;
        glfw.default_window_hints();

        // Wrap
        let window = Arc::new(Mutex::new(window));
        let res_window = Arc::new(Mutex::new(res_window));

        // draw initial screen to avoid blinking
        {
            let mut window = window.lock();
            window.make_current();
            let mut window = MutexGuard::deref_mut(&mut window);
            draw::init_gl(&mut window);
            draw::draw_bg(&mut window, window_args.bg_color);
            glfw::make_context_current(None);
        }

        // Texture registry
        let texture_registry = Arc::new(Mutex::new(TextureRegistry::new()));

        // Create engine
        let handler = Arc::new(GlfwFlutterEngineHandler {
            glfw: glfw.clone(),
            window: window.clone(),
            resource_window: res_window.clone(),
            texture_registry: texture_registry.clone(),
        });
        let engine = FlutterEngine::new(Arc::downgrade(&handler) as _);

        // register window and engine globally
        {
            ENGINES
                .lock()
                .insert(WindowSafe(window.lock().window_ptr()), engine.clone());
        }

        // Main thread callbacks
        let (main_tx, main_rx) = mpsc::channel();

        // Register plugins
        let isolate_tx: Sender<MainTheadFn> = main_tx.clone();
        engine.add_plugin(IsolatePlugin::new(move || {
            isolate_tx
                .send(Box::new(move |window| {
                    window.set_isolate_created();
                }))
                .unwrap();
        }));

        engine.add_plugin(KeyEventPlugin::default());
        engine.add_plugin(LifecyclePlugin::default());
        engine.add_plugin(LocalizationPlugin::default());
        engine.add_plugin(NavigationPlugin::default());
        engine.add_plugin(PlatformPlugin::new(Arc::new(Mutex::new(Box::new(
            GlfwPlatformHandler {
                window: window.clone(),
            },
        )))));
        engine.add_plugin(SettingsPlugin::default());
        engine.add_plugin(SystemPlugin::default());
        engine.add_plugin(TextInputPlugin::default());
        engine.add_plugin(DialogPlugin::default());

        let window_handler: Arc<Mutex<GlfwWindowHandler>> =
            Arc::new(Mutex::new(GlfwWindowHandler::new(window.clone())));
        engine.add_plugin(WindowPlugin::new(window_handler.clone() as _));

        Ok(Self {
            glfw: glfw.clone(),
            window,
            window_receiver: receiver,
            resource_window: res_window,
            _resource_window_receiver: res_window_recv,
            engine_handler: handler,
            engine,
            pointer_currently_added: AtomicBool::new(false),
            window_pixels_per_screen_coordinate: AtomicU64::new(0.0_f64.to_bits()),
            main_thread_receiver: main_rx,
            main_thread_sender: main_tx,
            isolate_created: AtomicBool::new(false),
            defered_events: Mutex::new(Default::default()),
            mouse_tracker: Mutex::new(Default::default()),
            texture_registry,
            window_handler,
        })
    }

    pub fn engine(&self) -> FlutterEngine {
        self.engine.clone()
    }

    pub fn window(&self) -> Arc<Mutex<glfw::Window>> {
        self.window.clone()
    }

    pub fn resource_window(&self) -> Arc<Mutex<glfw::Window>> {
        self.resource_window.clone()
    }

    pub fn create_texture(&self) -> Arc<ExternalTexture> {
        self.texture_registry.lock().create_texture(&self.engine)
    }

    pub fn add_plugin<P>(&self, plugin: P) -> &Self
    where
        P: Plugin + 'static,
    {
        self.engine.add_plugin(plugin);
        self
    }

    pub fn with_plugin<F, P>(&self, f: F)
    where
        F: FnOnce(&P),
        P: Plugin + 'static,
    {
        self.engine.with_plugin(f)
    }

    pub fn with_plugin_mut<F, P>(&self, f: F)
    where
        F: FnOnce(&mut P),
        P: Plugin + 'static,
    {
        self.engine.with_plugin_mut(f)
    }

    pub fn remove_channel(&self, channel_name: &str) -> Option<Arc<dyn Channel>> {
        self.engine.remove_channel(channel_name)
    }

    pub fn with_channel<F>(&self, channel_name: &str, f: F)
    where
        F: FnMut(&dyn Channel),
    {
        self.engine.with_channel(channel_name, f)
    }

    pub fn run(
        &self,
        assets_path: &Path,
        icu_data_path: &Path,
        arguments: &[&str],
        mut custom_handler: Option<&mut WindowEventHandler>,
        mut frame_callback: Option<&mut PerFrameCallback>,
    ) -> Result<(), RunError> {
        // Start engine
        self.engine.run(assets_path, icu_data_path, arguments)?;

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

            unsafe {
                glfw::ffi::glfwSetWindowRefreshCallback(
                    window.window_ptr(),
                    Some(window_refreshed),
                );
            }
        }

        self.with_plugin(
            |localization: &flutter_plugins::localization::LocalizationPlugin| {
                localization.send_locale(locale_config::Locale::current());
            },
        );

        let mut glfw = self.glfw.clone();
        while !self.window.lock().should_close() {
            // Execute tasks and callbacks
            let next_task_time = self.engine.execute_platform_tasks();

            let callbacks: Vec<MainTheadFn> = self.main_thread_receiver.try_iter().collect();
            for mut cb in callbacks {
                cb(&self);
            }

            // Sleep for events/till next task
            if let Some(next_task_time) = next_task_time {
                let now = Instant::now();
                if now < next_task_time {
                    let duration = next_task_time.duration_since(now);
                    let secs = duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9;
                    glfw.wait_events_timeout(secs);
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
                    custom_handler(&self, event.clone())
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
                callback(&self);
            }
        }

        Ok(())
    }

    pub fn post_main_thread_callback<F>(&self, f: F) -> Result<(), SendError<MainTheadFn>>
    where
        F: FnMut(&FlutterWindow) + Send + 'static,
    {
        self.main_thread_sender.send(Box::new(f))?;
        self.engine_handler.wake_platform_thread();
        Ok(())
    }

    pub fn set_isolate_created(&self) {
        self.isolate_created.store(true, Ordering::Relaxed);

        while let Some(evt) = self.defered_events.lock().pop_front() {
            self.handle_glfw_event(evt);
        }
    }

    pub fn shutdown(self) {
        ENGINES
            .lock()
            .remove(&WindowSafe(self.window.lock().window_ptr()));

        self.engine.shutdown();
    }

    fn send_scale_or_size_change(&self) {
        let window = self.window.lock();
        let window_size = window.get_size();
        let framebuffer_size = window.get_framebuffer_size();
        let scale = window.get_content_scale();
        self.window_pixels_per_screen_coordinate.store(
            (f64::from(framebuffer_size.0) / f64::from(window_size.0)).to_bits(),
            Ordering::Relaxed,
        );
        debug!(
            "Setting framebuffer size to {:?}, scale to {}",
            framebuffer_size, scale.0
        );
        self.engine.send_window_metrics_event(
            framebuffer_size.0 as _,
            framebuffer_size.1 as _,
            f64::from(scale.0),
        );
    }

    fn send_pointer_event(
        &self,
        phase: FlutterPointerPhase,
        (x, y): (f64, f64),
        signal_kind: FlutterPointerSignalKind,
        (scroll_delta_x, scroll_delta_y): (f64, f64),
        buttons: FlutterPointerMouseButtons,
    ) {
        if !self.pointer_currently_added.load(Ordering::Relaxed)
            && phase != FlutterPointerPhase::Add
            && phase != FlutterPointerPhase::Remove
        {
            self.send_pointer_event(
                FlutterPointerPhase::Add,
                (x, y),
                FlutterPointerSignalKind::None,
                (0.0, 0.0),
                buttons,
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
        );

        match phase {
            FlutterPointerPhase::Add => self.pointer_currently_added.store(true, Ordering::Relaxed),
            FlutterPointerPhase::Remove => {
                self.pointer_currently_added.store(false, Ordering::Relaxed)
            }
            _ => {}
        }
    }

    pub fn handle_glfw_event(&self, event: glfw::WindowEvent) {
        if !self.isolate_created.load(Ordering::Relaxed) {
            self.defered_events.lock().push_back(event);
            return;
        }

        match event {
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
                    FlutterPointerMouseButtons::Primary,
                );
            }
            glfw::WindowEvent::CursorPos(x, y) => {
                // fix error when dragging cursor out of a window
                if !self.pointer_currently_added.load(Ordering::Relaxed) {
                    return;
                }
                let phase = if self
                    .mouse_tracker
                    .lock()
                    .get(&glfw::MouseButtonLeft)
                    .unwrap_or(&glfw::Action::Release)
                    == &glfw::Action::Press
                {
                    FlutterPointerPhase::Move
                } else {
                    FlutterPointerPhase::Hover
                };
                self.send_pointer_event(
                    phase,
                    (x, y),
                    FlutterPointerSignalKind::None,
                    (0.0, 0.0),
                    FlutterPointerMouseButtons::Primary,
                );
            }
            glfw::WindowEvent::MouseButton(
                glfw::MouseButton::Button4,
                glfw::Action::Press,
                _modifiers,
            ) => {
                self.mouse_tracker
                    .lock()
                    .insert(glfw::MouseButton::Button4, glfw::Action::Press);
                self.with_plugin(
                    |navigation: &flutter_plugins::navigation::NavigationPlugin| {
                        navigation.pop_route();
                    },
                );
            }
            glfw::WindowEvent::MouseButton(buttons, action, _modifiers) => {
                // Since Events are delayed by wait_events_timeout,
                // it's not accurate to use get_mouse_button API to fetch current mouse state
                // Here we save mouse states, and query it in this HashMap
                self.mouse_tracker.lock().insert(buttons, action);

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
                let buttons = match buttons {
                    glfw::MouseButtonLeft => FlutterPointerMouseButtons::Primary,
                    glfw::MouseButtonRight => FlutterPointerMouseButtons::Secondary,
                    glfw::MouseButtonMiddle => FlutterPointerMouseButtons::Middle,
                    glfw::MouseButton::Button4 => FlutterPointerMouseButtons::Back,
                    glfw::MouseButton::Button5 => FlutterPointerMouseButtons::Forward,
                    _ => FlutterPointerMouseButtons::Primary,
                };
                self.send_pointer_event(
                    phase,
                    (x, y),
                    FlutterPointerSignalKind::None,
                    (0.0, 0.0),
                    buttons,
                );
            }
            glfw::WindowEvent::Scroll(scroll_delta_x, scroll_delta_y) => {
                let (x, y) = self.window.lock().get_cursor_pos();
                let phase = if self
                    .mouse_tracker
                    .lock()
                    .get(&glfw::MouseButtonLeft)
                    .unwrap_or(&glfw::Action::Release)
                    == &glfw::Action::Press
                {
                    FlutterPointerPhase::Move
                } else {
                    FlutterPointerPhase::Hover
                };
                self.send_pointer_event(
                    phase,
                    (x, y),
                    FlutterPointerSignalKind::Scroll,
                    (
                        scroll_delta_x * SCROLL_SPEED,
                        -scroll_delta_y * SCROLL_SPEED,
                    ),
                    FlutterPointerMouseButtons::Primary,
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
            glfw::WindowEvent::Key(key, scancode, glfw::Action::Press, modifiers)
            | glfw::WindowEvent::Key(key, scancode, glfw::Action::Repeat, modifiers) => {
                // TODO: move this to TextInputPlugin
                match key {
                    glfw::Key::Enter => self.with_plugin_mut(
                        |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                            text_input.with_state(|state| {
                                state.add_characters(&"\n");
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
                    glfw::Key::A => {
                        if modifiers.contains(FUNCTION_MODIFIER_KEY) {
                            self.with_plugin_mut(
                                |text_input: &mut flutter_plugins::textinput::TextInputPlugin| {
                                    text_input.with_state(|state| {
                                        state.select_all();
                                    });
                                    text_input.notify_changes();
                                },
                            )
                        }
                    }
                    glfw::Key::X => {
                        if modifiers.contains(FUNCTION_MODIFIER_KEY) {
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
                    }
                    glfw::Key::C => {
                        if modifiers.contains(FUNCTION_MODIFIER_KEY) {
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
                    }
                    glfw::Key::V => {
                        if modifiers.contains(FUNCTION_MODIFIER_KEY) {
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
                    }
                    _ => {}
                }

                self.with_plugin_mut(|keyevent: &mut flutter_plugins::keyevent::KeyEventPlugin| {
                    keyevent.key_action(KeyAction {
                        toolkit: "glfw".to_string(),
                        key_code: key as i32,
                        scan_code: scancode as i32,
                        modifiers: modifiers.bits() as i32,
                        keymap: "linux".to_string(),
                        _type: KeyActionType::Keydown,
                    });
                });
            }
            glfw::WindowEvent::Key(key, scancode, glfw::Action::Release, modifiers) => {
                self.with_plugin_mut(|keyevent: &mut flutter_plugins::keyevent::KeyEventPlugin| {
                    keyevent.key_action(KeyAction {
                        toolkit: "glfw".to_string(),
                        key_code: key as i32,
                        scan_code: scancode as i32,
                        modifiers: modifiers.bits() as i32,
                        keymap: "linux".to_string(),
                        _type: KeyActionType::Keyup,
                    });
                });
            }
            _ => {}
        }
    }
}

extern "C" fn window_refreshed(window: *mut glfw::ffi::GLFWwindow) {
    if let Some(engine) = get_engine(window) {
        let mut window_size: (i32, i32) = (0, 0);
        let mut framebuffer_size: (i32, i32) = (0, 0);
        let mut scale: (f32, f32) = (0.0, 0.0);

        unsafe {
            glfw::ffi::glfwGetWindowSize(window, &mut window_size.0, &mut window_size.1);
            glfw::ffi::glfwGetFramebufferSize(
                window,
                &mut framebuffer_size.0,
                &mut framebuffer_size.1,
            );
            glfw::ffi::glfwGetWindowContentScale(window, &mut scale.0, &mut scale.1);
        }

        // probably dont need this, since after resize a framebuffer size
        // change event is sent and set this regardless
        // self.window_pixels_per_screen_coordinate =
        //     f64::from(framebuffer_size.0) / f64::from(window_size.0);

        log::debug!(
            "Setting framebuffer size to {:?}, scale to {}",
            framebuffer_size,
            scale.0
        );

        engine.send_window_metrics_event(
            framebuffer_size.0 as _,
            framebuffer_size.1 as _,
            f64::from(scale.0),
        );
    }
}
