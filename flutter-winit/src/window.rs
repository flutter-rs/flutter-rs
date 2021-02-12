use crate::context::Context;
use crate::handler::{
    WinitOpenGLHandler, WinitPlatformHandler, WinitPlatformTaskHandler, WinitTextInputHandler,
    WinitWindowHandler,
};
use crate::keyboard::raw_key;
use crate::pointer::Pointers;
use flutter_engine::builder::FlutterEngineBuilder;
use flutter_engine::channel::Channel;
use flutter_engine::plugins::Plugin;
use flutter_engine::plugins::PluginRegistrar;
use flutter_engine::texture_registry::Texture;
use flutter_engine::FlutterEngine;
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
    ElementState, Event, KeyboardInput, MouseScrollDelta, Touch, VirtualKeyCode, WindowEvent,
};
use glutin::event_loop::{ControlFlow, EventLoop};
use glutin::window::WindowBuilder;
use glutin::ContextBuilder;
use parking_lot::Mutex;
use std::error::Error;
use std::path::PathBuf;
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
    close: Arc<AtomicBool>,
    plugin_registrar: PluginRegistrar,
}

impl FlutterWindow {
    pub fn new(
        window: WindowBuilder,
        assets_path: PathBuf,
        arguments: Vec<String>,
    ) -> Result<Self, Box<dyn Error>> {
        let event_loop = EventLoop::with_user_event();
        let proxy = event_loop.create_proxy();

        let context = ContextBuilder::new().build_windowed(window, &event_loop)?;
        let context = Arc::new(Mutex::new(Context::from_context(context)));
        let resource_context = Arc::new(Mutex::new(Context::empty()));

        let platform_task_handler = Arc::new(WinitPlatformTaskHandler::new(proxy));

        let opengl_handler = WinitOpenGLHandler::new(context.clone(), resource_context.clone());

        let engine = FlutterEngineBuilder::new()
            .with_platform_handler(platform_task_handler)
            .with_opengl(opengl_handler)
            .with_asset_path(assets_path)
            .with_args(arguments)
            .build()
            .expect("Failed to create engine");

        let proxy = event_loop.create_proxy();
        let isolate_cb = move || {
            proxy.send_event(FlutterEvent::IsolateCreated).ok();
        };
        let platform_handler = Arc::new(Mutex::new(WinitPlatformHandler::new(context.clone())?));
        let close = Arc::new(AtomicBool::new(false));
        let window_handler = Arc::new(Mutex::new(WinitWindowHandler::new(
            context.clone(),
            close.clone(),
        )));
        let textinput_handler = Arc::new(Mutex::new(WinitTextInputHandler::default()));

        let mut plugin_registrar = PluginRegistrar::new();

        plugin_registrar.add_plugin(&engine, DialogPlugin::default());
        plugin_registrar.add_plugin(&engine, IsolatePlugin::new(isolate_cb));
        plugin_registrar.add_plugin(&engine, KeyEventPlugin::default());
        plugin_registrar.add_plugin(&engine, LifecyclePlugin::default());
        plugin_registrar.add_plugin(&engine, LocalizationPlugin::default());
        plugin_registrar.add_plugin(&engine, NavigationPlugin::default());
        plugin_registrar.add_plugin(&engine, PlatformPlugin::new(platform_handler));
        plugin_registrar.add_plugin(&engine, SettingsPlugin::default());
        plugin_registrar.add_plugin(&engine, SystemPlugin::default());
        plugin_registrar.add_plugin(&engine, TextInputPlugin::new(textinput_handler));
        plugin_registrar.add_plugin(&engine, WindowPlugin::new(window_handler));

        Ok(Self {
            event_loop,
            context,
            resource_context,
            engine,
            close,
            plugin_registrar,
        })
    }

    pub fn with_resource_context(self) -> Result<Self, Box<dyn Error>> {
        {
            let window = WindowBuilder::new().with_visible(false);
            let context = self.context.lock();
            let resource_context = ContextBuilder::new()
                .with_shared_lists(context.context().unwrap())
                .build_windowed(window, &self.event_loop)?;

            let resource_context = unsafe { resource_context.make_current().unwrap() };
            gl::load_with(|s| resource_context.get_proc_address(s));
            let resource_context = unsafe { resource_context.make_not_current().unwrap() };

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

    pub fn create_texture(&self) -> Texture {
        self.engine.create_texture()
    }

    pub fn add_plugin<P>(&mut self, plugin: P) -> &Self
    where
        P: Plugin + 'static,
    {
        self.plugin_registrar.add_plugin(&self.engine, plugin);
        self
    }

    pub fn with_plugin<F, P>(&self, f: F)
    where
        F: FnOnce(&P),
        P: Plugin + 'static,
    {
        self.plugin_registrar.with_plugin(f)
    }

    pub fn with_plugin_mut<F, P>(&mut self, f: F)
    where
        F: FnOnce(&mut P),
        P: Plugin + 'static,
    {
        self.plugin_registrar.with_plugin_mut(f)
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

    pub fn start_engine(&self) -> Result<(), ()> {
        self.engine.run()
    }

    pub fn run(self) -> ! {
        let engine = self.engine.clone();
        let mut plugin_registrar = self.plugin_registrar;
        let context = self.context.clone();
        let close = self.close.clone();

        resize(&engine, &context);

        plugin_registrar.with_plugin(|localization: &LocalizationPlugin| {
            localization.send_locale(locale_config::Locale::current());
        });

        let mut pointers = Pointers::new(engine.clone());
        self.event_loop
            .run(move |event, _, control_flow| match event {
                Event::WindowEvent { event, .. } => {
                    match event {
                        WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                        WindowEvent::Resized(_) => resize(&engine, &context),
                        WindowEvent::ScaleFactorChanged { .. } => resize(&engine, &context),
                        WindowEvent::CursorEntered { device_id } => pointers.enter(device_id),
                        WindowEvent::CursorLeft { device_id } => pointers.leave(device_id),
                        WindowEvent::CursorMoved {
                            device_id,
                            position,
                            ..
                        } => {
                            let dpi = { context.lock().hidpi_factor() };
                            let position: glutin::dpi::LogicalPosition<f64> =
                                position.to_logical(dpi);
                            pointers.moved(device_id, position.into());
                        }
                        WindowEvent::MouseInput {
                            device_id,
                            state,
                            button,
                            ..
                        } => {
                            pointers.input(device_id, state, button);
                        }
                        WindowEvent::MouseWheel {
                            device_id, delta, ..
                        } => {
                            let delta = match delta {
                                MouseScrollDelta::LineDelta(_, _) => (0.0, 0.0), // TODO
                                MouseScrollDelta::PixelDelta(position) => {
                                    let dpi: f64 = { context.lock().hidpi_factor() };
                                    let (dx, dy): (f64, f64) = (position.to_logical(dpi)
                                        as glutin::dpi::LogicalPosition<f64>)
                                        .into();
                                    (-dx, dy)
                                }
                            };
                            pointers.wheel(device_id, delta);
                        }
                        WindowEvent::Touch(Touch {
                            device_id,
                            phase,
                            location,
                            ..
                        }) => {
                            let dpi = { context.lock().hidpi_factor() };
                            let position: glutin::dpi::LogicalPosition<f64> =
                                location.to_logical(dpi);
                            pointers.touch(device_id, phase, position.into());
                        }
                        WindowEvent::ReceivedCharacter(ch) => {
                            if !ch.is_control() {
                                plugin_registrar.with_plugin_mut(
                                    |text_input: &mut TextInputPlugin| {
                                        text_input.with_state(|state| {
                                            state.add_characters(&ch.to_string());
                                        });
                                        text_input.notify_changes();
                                    },
                                );
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

                            let shift = modifiers.shift() as u32;
                            let ctrl = modifiers.ctrl() as u32;
                            let alt = modifiers.alt() as u32;
                            let logo = modifiers.logo() as u32;
                            let raw_modifiers = shift | ctrl << 1 | alt << 2 | logo << 3;

                            match state {
                                ElementState::Pressed => {
                                    if let Some(key) = virtual_keycode {
                                        plugin_registrar.with_plugin_mut(
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

                                    plugin_registrar.with_plugin_mut(
                                        |keyevent: &mut KeyEventPlugin| {
                                            keyevent.key_action(KeyAction {
                                                toolkit: "glfw".to_string(),
                                                key_code: raw_key as _,
                                                scan_code: scancode as _,
                                                modifiers: raw_modifiers as _,
                                                keymap: "linux".to_string(),
                                                _type: KeyActionType::Keydown,
                                            });
                                        },
                                    );
                                }
                                ElementState::Released => {
                                    plugin_registrar.with_plugin_mut(
                                        |keyevent: &mut KeyEventPlugin| {
                                            keyevent.key_action(KeyAction {
                                                toolkit: "glfw".to_string(),
                                                key_code: raw_key as _,
                                                scan_code: scancode as _,
                                                modifiers: raw_modifiers as _,
                                                keymap: "linux".to_string(),
                                                _type: KeyActionType::Keyup,
                                            });
                                        },
                                    );
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
