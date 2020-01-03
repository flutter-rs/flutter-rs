use crate::context::Context;
use crate::handler::{WinitFlutterEngineHandler, WinitPlatformHandler, WinitWindowHandler};
use flutter_engine::channel::Channel;
use flutter_engine::ffi::{
    FlutterPointerDeviceKind, FlutterPointerMouseButtons, FlutterPointerPhase,
    FlutterPointerSignalKind,
};
use flutter_engine::plugins::Plugin;
use flutter_engine::texture_registry::{ExternalTexture, TextureRegistry};
use flutter_engine::{FlutterEngine, FlutterEngineHandler};
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
use glutin::event::{
    ElementState, Event, KeyboardInput, MouseButton, MouseScrollDelta, Touch, TouchPhase,
    VirtualKeyCode, WindowEvent,
};
use glutin::event_loop::{ControlFlow, EventLoop};
use glutin::window::WindowBuilder;
use glutin::ContextBuilder;
use parking_lot::Mutex;
use std::error::Error;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub enum FlutterEvent {
    WakePlatformThread,
    IsolateCreated,
}

pub struct FlutterWindow {
    event_loop: EventLoop<FlutterEvent>,
    context: Arc<Mutex<Context>>,
    resource_context: Arc<Mutex<Context>>,
    engine: FlutterEngine,
    engine_handler: Arc<WinitFlutterEngineHandler>,
    texture_registry: Arc<Mutex<TextureRegistry>>,
    close: Arc<AtomicBool>,
}

impl FlutterWindow {
    pub fn new(window: WindowBuilder) -> Result<Self, Box<dyn Error>> {
        let event_loop = EventLoop::with_user_event();
        let proxy = event_loop.create_proxy();

        let context = ContextBuilder::new().build_windowed(window, &event_loop)?;
        let context = Arc::new(Mutex::new(Context::from_context(context)));
        let resource_context = Arc::new(Mutex::new(Context::empty()));

        let texture_registry = Arc::new(Mutex::new(TextureRegistry::new()));
        let engine_handler = Arc::new(WinitFlutterEngineHandler::new(
            proxy,
            context.clone(),
            resource_context.clone(),
            texture_registry.clone(),
        ));
        let engine = FlutterEngine::new(Arc::downgrade(&engine_handler) as _);

        let proxy = event_loop.create_proxy();
        let isolate_cb = move || {
            proxy.send_event(FlutterEvent::IsolateCreated).ok();
        };
        let platform_handler = Arc::new(Mutex::new(Box::new(WinitPlatformHandler::new(
            context.clone(),
        )?) as _));
        let close = Arc::new(AtomicBool::new(false));
        let window_handler = Arc::new(Mutex::new(WinitWindowHandler::new(
            context.clone(),
            close.clone(),
        )));

        engine.add_plugin(DialogPlugin::default());
        engine.add_plugin(IsolatePlugin::new(isolate_cb));
        engine.add_plugin(KeyEventPlugin::default());
        engine.add_plugin(LifecyclePlugin::default());
        engine.add_plugin(LocalizationPlugin::default());
        engine.add_plugin(NavigationPlugin::default());
        engine.add_plugin(PlatformPlugin::new(platform_handler));
        engine.add_plugin(SettingsPlugin::default());
        engine.add_plugin(SystemPlugin::default());
        engine.add_plugin(TextInputPlugin::default());
        engine.add_plugin(WindowPlugin::new(window_handler));

        Ok(Self {
            event_loop,
            context,
            resource_context,
            engine,
            engine_handler,
            texture_registry,
            close,
        })
    }

    pub fn with_resource_context(self) -> Result<Self, Box<dyn Error>> {
        {
            let context = self.context.lock();
            let resource_context = ContextBuilder::new()
                .with_shared_lists(context.context().unwrap())
                .build_windowed(WindowBuilder::new(), &self.event_loop)?;
            let mut guard = self.resource_context.lock();
            *guard = Context::from_context(resource_context);
        }
        Ok(self)
    }

    pub fn engine(&self) -> FlutterEngine {
        self.engine.clone()
    }

    pub fn context(&self) -> Arc<Mutex<Context>> {
        self.context.clone()
    }

    pub fn resource_context(&self) -> Arc<Mutex<Context>> {
        self.resource_context.clone()
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

    pub fn start_engine(
        &self,
        assets_path: &Path,
        arguments: &[String],
    ) -> Result<(), Box<dyn Error>> {
        self.engine.run(assets_path, arguments)?;
        Ok(())
    }

    pub fn run(self) -> ! {
        let engine = self.engine.clone();
        let context = self.context.clone();
        let close = self.close.clone();

        resize(&engine, &context);

        engine.with_plugin(|localization: &LocalizationPlugin| {
            localization.send_locale(locale_config::Locale::current());
        });

        let mut cursor = (0.0, 0.0);
        self.event_loop
            .run(move |event, _, control_flow| match event {
                Event::WindowEvent { event, .. } => {
                    match event {
                        WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                        WindowEvent::Resized(_) => resize(&engine, &context),
                        WindowEvent::HiDpiFactorChanged(_) => resize(&engine, &context),
                        WindowEvent::CursorEntered { .. } => {
                            engine.send_pointer_event(
                                FlutterPointerPhase::Add,
                                cursor,
                                FlutterPointerSignalKind::None,
                                (0.0, 0.0),
                                FlutterPointerDeviceKind::Mouse,
                                FlutterPointerMouseButtons::Primary,
                            );
                        }
                        WindowEvent::CursorLeft { .. } => {
                            engine.send_pointer_event(
                                FlutterPointerPhase::Remove,
                                cursor,
                                FlutterPointerSignalKind::None,
                                (0.0, 0.0),
                                FlutterPointerDeviceKind::Mouse,
                                FlutterPointerMouseButtons::Primary,
                            );
                        }
                        WindowEvent::CursorMoved { position, .. } => {
                            let dpi = { context.lock().hidpi_factor() };
                            cursor = position.to_physical(dpi).into();

                            engine.send_pointer_event(
                                // TODO Move
                                FlutterPointerPhase::Hover,
                                cursor,
                                FlutterPointerSignalKind::None,
                                (0.0, 0.0),
                                FlutterPointerDeviceKind::Mouse,
                                FlutterPointerMouseButtons::Primary,
                            );
                        }
                        WindowEvent::MouseInput { state, button, .. } => {
                            let phase = match state {
                                ElementState::Pressed => FlutterPointerPhase::Down,
                                ElementState::Released => FlutterPointerPhase::Up,
                            };
                            let button = match button {
                                MouseButton::Left => FlutterPointerMouseButtons::Primary,
                                MouseButton::Right => FlutterPointerMouseButtons::Secondary,
                                MouseButton::Middle => FlutterPointerMouseButtons::Middle,
                                MouseButton::Other(4) => FlutterPointerMouseButtons::Back,
                                MouseButton::Other(5) => FlutterPointerMouseButtons::Forward,
                                _ => FlutterPointerMouseButtons::Primary,
                            };
                            engine.send_pointer_event(
                                phase,
                                cursor,
                                FlutterPointerSignalKind::None,
                                (0.0, 0.0),
                                FlutterPointerDeviceKind::Mouse,
                                button,
                            );
                        }
                        WindowEvent::MouseWheel { delta, .. } => {
                            let delta = match delta {
                                MouseScrollDelta::LineDelta(_, _) => (0.0, 0.0), // TODO
                                MouseScrollDelta::PixelDelta(position) => {
                                    let dpi = { context.lock().hidpi_factor() };
                                    let (dx, dy): (f64, f64) = position.to_physical(dpi).into();
                                    (-dx, dy)
                                }
                            };

                            engine.send_pointer_event(
                                // TODO
                                FlutterPointerPhase::Hover,
                                cursor,
                                FlutterPointerSignalKind::Scroll,
                                delta,
                                FlutterPointerDeviceKind::Mouse,
                                FlutterPointerMouseButtons::Primary,
                            );
                        }
                        WindowEvent::Touch(Touch {
                            phase, location, ..
                        }) => {
                            let dpi = { context.lock().hidpi_factor() };
                            cursor = location.to_physical(dpi).into();
                            let phase = match phase {
                                TouchPhase::Started => FlutterPointerPhase::Down,
                                TouchPhase::Moved => FlutterPointerPhase::Move,
                                TouchPhase::Ended => FlutterPointerPhase::Up,
                                TouchPhase::Cancelled => FlutterPointerPhase::Cancel,
                            };

                            engine.send_pointer_event(
                                phase,
                                cursor,
                                FlutterPointerSignalKind::None,
                                (0.0, 0.0),
                                FlutterPointerDeviceKind::Touch,
                                // TODO
                                FlutterPointerMouseButtons::Primary,
                            );
                        }
                        WindowEvent::ReceivedCharacter(ch) => {
                            if !ch.is_control() {
                                engine.with_plugin_mut(|text_input: &mut TextInputPlugin| {
                                    text_input.with_state(|state| {
                                        state.add_characters(&ch.to_string());
                                    });
                                    text_input.notify_changes();
                                });
                            }
                        }
                        WindowEvent::KeyboardInput {
                            input:
                                KeyboardInput {
                                    state,
                                    virtual_keycode,
                                    modifiers,
                                    scancode,
                                },
                            ..
                        } => {
                            let raw_key = if let Some(raw_key) = raw_key(virtual_keycode) {
                                raw_key
                            } else {
                                return;
                            };

                            let shift = modifiers.shift as u32;
                            let ctrl = modifiers.ctrl as u32;
                            let alt = modifiers.alt as u32;
                            let logo = modifiers.logo as u32;
                            let raw_modifiers = shift | ctrl << 1 | alt << 2 | logo << 3;

                            match state {
                                ElementState::Pressed => {
                                    if let Some(key) = virtual_keycode {
                                        engine.with_plugin_mut(
                                            |text_input: &mut TextInputPlugin| match key {
                                                VirtualKeyCode::Return => {
                                                    text_input.with_state(|state| {
                                                        state.add_characters(&"\n");
                                                    });
                                                    text_input.notify_changes();
                                                }
                                                VirtualKeyCode::Back => {
                                                    text_input.with_state(|state| {
                                                        state.backspace();
                                                    });
                                                    text_input.notify_changes();
                                                }
                                                _ => {}
                                            },
                                        );
                                    }

                                    engine.with_plugin_mut(|keyevent: &mut KeyEventPlugin| {
                                        keyevent.key_action(KeyAction {
                                            toolkit: "glfw".to_string(),
                                            key_code: raw_key as _,
                                            scan_code: scancode as _,
                                            modifiers: raw_modifiers as _,
                                            keymap: "linux".to_string(),
                                            _type: KeyActionType::Keydown,
                                        });
                                    });
                                }
                                ElementState::Released => {
                                    engine.with_plugin_mut(|keyevent: &mut KeyEventPlugin| {
                                        keyevent.key_action(KeyAction {
                                            toolkit: "glfw".to_string(),
                                            key_code: raw_key as _,
                                            scan_code: scancode as _,
                                            modifiers: raw_modifiers as _,
                                            keymap: "linux".to_string(),
                                            _type: KeyActionType::Keyup,
                                        });
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Event::LoopDestroyed => {
                    engine.shutdown();
                }
                _ => {
                    if close.load(Ordering::Relaxed) {
                        *control_flow = ControlFlow::Exit;
                        return;
                    }

                    let next_task_time = engine.execute_platform_tasks();

                    if let Some(next_task_time) = next_task_time {
                        *control_flow = ControlFlow::WaitUntil(next_task_time)
                    } else {
                        *control_flow = ControlFlow::Wait
                    }
                }
            });
    }

    pub fn wake_platform_thread(&self) {
        self.engine_handler.wake_platform_thread();
    }
}

fn resize(engine: &FlutterEngine, context: &Arc<Mutex<Context>>) {
    let mut context = context.lock();
    let dpi = context.hidpi_factor();
    let size = context.size().to_physical(dpi);
    log::trace!(
        "resize width: {} height: {} scale {}",
        size.width,
        size.height,
        dpi
    );
    context.resize(size);
    engine.send_window_metrics_event(size.width as usize, size.height as usize, dpi);
}

// Emulates glfw key numbers
// https://github.com/flutter/flutter/blob/master/packages/flutter/lib/src/services/keyboard_maps.dart
fn raw_key(key: Option<VirtualKeyCode>) -> Option<u32> {
    let key = if let Some(key) = key {
        if key as u32 >= Key::A as u32 && key as u32 <= Key::Z as u32 {
            return Some(key as u32 - Key::A as u32 + 65);
        }

        if key as u32 >= Key::Key1 as u32 && key as u32 <= Key::Key9 as u32 {
            return Some(key as u32 - Key::Key1 as u32 + 49);
        }

        key
    } else {
        return None;
    };

    use VirtualKeyCode as Key;
    let code = match key {
        Key::Key0 => 48,
        Key::Return => 257,
        Key::Escape => 256,
        Key::Back => 259,
        Key::Tab => 258,
        Key::Space => 32,
        Key::LControl => 341,
        Key::LShift => 340,
        Key::LAlt => 342,
        Key::LWin => 343,
        Key::RControl => 345,
        Key::RShift => 344,
        Key::RAlt => 346,
        Key::RWin => 347,
        Key::Minus => 45,
        Key::Equals => 61,
        Key::LBracket => 91,
        Key::RBracket => 93,
        Key::Backslash => 92,
        Key::Semicolon => 59,
        Key::Apostrophe => 39,
        //Key::Backquote => 96,
        Key::Comma => 44,
        Key::Period => 46,
        Key::Slash => 47,
        //Key::CapsLock => 280,
        Key::Snapshot => 283,
        Key::Pause => 284,
        Key::Insert => 260,
        Key::Home => 268,
        Key::PageUp => 266,
        Key::Delete => 261,
        Key::End => 269,
        Key::PageDown => 267,
        Key::Right => 262,
        Key::Left => 263,
        Key::Down => 264,
        Key::Up => 265,
        Key::Numlock => 282,
        //Key::NumpadDivide => 331,
        //Key::NumpadMultiply => 332,
        //Key::NumpadAdd => 334,
        Key::NumpadEnter => 335,
        Key::Numpad0 => 320,
        Key::Numpad1 => 321,
        Key::Numpad2 => 322,
        Key::Numpad3 => 323,
        Key::Numpad4 => 324,
        Key::Numpad5 => 325,
        Key::Numpad6 => 326,
        Key::Numpad7 => 327,
        Key::Numpad8 => 328,
        Key::Numpad9 => 329,
        //Key::NumpadDecimal => 330,
        //Key::ContextMenu => 348,
        Key::NumpadEquals => 336,
        Key::F1 => 290,
        Key::F2 => 291,
        Key::F3 => 292,
        Key::F4 => 293,
        Key::F5 => 294,
        Key::F6 => 295,
        Key::F7 => 296,
        Key::F8 => 297,
        Key::F9 => 298,
        Key::F10 => 299,
        Key::F11 => 300,
        Key::F12 => 301,
        Key::F13 => 302,
        Key::F14 => 303,
        Key::F15 => 304,
        Key::F16 => 305,
        Key::F17 => 306,
        Key::F18 => 307,
        Key::F19 => 308,
        Key::F20 => 309,
        Key::F21 => 310,
        Key::F22 => 311,
        Key::F23 => 312,
        _ => return None,
    };
    Some(code)
}
