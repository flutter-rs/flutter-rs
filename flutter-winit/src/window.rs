use crate::handler::{WinitFlutterEngineHandler, WinitPlatformHandler, WinitWindowHandler};
use crate::keyboard::Keyboard;
use crate::pointer::Pointers;
use flutter_engine::channel::Channel;
use flutter_engine::plugins::Plugin;
use flutter_engine::texture_registry::Texture;
use flutter_engine::{FlutterEngine, FlutterEngineHandler};
use flutter_plugins::dialog::DialogPlugin;
use flutter_plugins::isolate::IsolatePlugin;
use flutter_plugins::keyevent::KeyEventPlugin;
use flutter_plugins::lifecycle::LifecyclePlugin;
use flutter_plugins::localization::LocalizationPlugin;
use flutter_plugins::navigation::NavigationPlugin;
use flutter_plugins::platform::PlatformPlugin;
use flutter_plugins::settings::SettingsPlugin;
use flutter_plugins::system::SystemPlugin;
use flutter_plugins::textinput::TextInputPlugin;
use flutter_plugins::window::WindowPlugin;
use glutin::config::{Config, ConfigsFinder};
use glutin::context::{Context, ContextBuilder};
use glutin::surface::{Surface, Window as WindowMarker};
use parking_lot::Mutex;
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use winit::event::{DeviceEvent, Event, MouseScrollDelta, Touch, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

#[derive(Debug)]
pub enum FlutterEvent {
    WakePlatformThread,
    IsolateCreated,
}

pub struct FlutterWindow {
    event_loop: Option<EventLoop<FlutterEvent>>,
    config: Config,
    context: Arc<Context>,
    _resource_context: Option<Arc<Context>>,
    window: Arc<Window>,
    surface: Arc<Mutex<Option<Surface<WindowMarker>>>>,
    engine: FlutterEngine,
    engine_handler: Arc<WinitFlutterEngineHandler>,
    close: Arc<AtomicBool>,
}

impl FlutterWindow {
    pub fn new(
        wb: WindowBuilder,
        assets_path: PathBuf,
        resource_context: bool,
    ) -> Result<Self, Box<dyn Error>> {
        let event_loop = EventLoop::with_user_event();
        let proxy = event_loop.create_proxy();

        let confs = unsafe { ConfigsFinder::new().find(&*event_loop)? };
        let config = confs.get(0).expect("found opengl conf").clone();
        let context = unsafe { ContextBuilder::new().build(&config)? };
        let window = unsafe { Surface::build_window(&config, &*event_loop, wb)? };
        let resource_context = if resource_context {
            let context = unsafe {
                ContextBuilder::new()
                    .with_sharing(Some(&context))
                    .build(&config)?
            };
            unsafe { context.make_current_surfaceless()? };
            gl::load_with(|s| context.get_proc_address(s).unwrap());
            unsafe { context.make_not_current()? };
            Some(Arc::new(context))
        } else {
            None
        };

        let context = Arc::new(context);
        let window = Arc::new(window);
        let surface = Arc::new(Mutex::new(None));

        let engine_handler = Arc::new(WinitFlutterEngineHandler::new(
            proxy,
            surface.clone(),
            context.clone(),
            resource_context.clone(),
        ));
        let engine = FlutterEngine::new(Arc::downgrade(&engine_handler) as _, assets_path);

        let proxy = event_loop.create_proxy();
        let isolate_cb = move || {
            proxy.send_event(FlutterEvent::IsolateCreated).ok();
        };
        let platform_handler = Arc::new(Mutex::new(Box::new(WinitPlatformHandler::new(
            window.clone(),
        )?) as _));
        let close = Arc::new(AtomicBool::new(false));
        let window_handler = Arc::new(Mutex::new(WinitWindowHandler::new(
            window.clone(),
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
            event_loop: Some(event_loop),
            config,
            context,
            _resource_context: resource_context,
            surface,
            window,
            engine,
            engine_handler,
            close,
        })
    }

    pub fn engine(&self) -> FlutterEngine {
        self.engine.clone()
    }

    pub fn create_texture(&self) -> Texture {
        self.engine.create_texture()
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

    fn resize(&self) {
        let dpi = self.window.scale_factor();
        let size = self.window.inner_size();
        log::trace!(
            "resize width: {} height: {} scale {}",
            size.width,
            size.height,
            dpi
        );
        self.context.update_after_resize();
        if let Some(surf) = self.surface.lock().as_ref() {
            surf.update_after_resize(size);
        }
        self.engine
            .send_window_metrics_event(size.width as usize, size.height as usize, dpi);
    }

    pub fn run(mut self, arguments: Vec<String>) -> ! {
        let event_loop = self.event_loop.take().unwrap();
        let mut keyboard = Keyboard::new(self.engine.clone());
        let mut pointers = Pointers::new(self.engine.clone());
        event_loop.run(move |event, _, control_flow| match event {
            Event::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    WindowEvent::Resized(_) => self.resize(),
                    WindowEvent::ScaleFactorChanged { .. } => self.resize(),
                    WindowEvent::CursorEntered { device_id } => pointers.enter(device_id),
                    WindowEvent::CursorLeft { device_id } => pointers.leave(device_id),
                    WindowEvent::CursorMoved {
                        device_id,
                        position,
                        ..
                    } => pointers.moved(device_id, position.into()),
                    WindowEvent::MouseInput {
                        device_id,
                        state,
                        button,
                        ..
                    } => pointers.input(device_id, state, button),
                    WindowEvent::MouseWheel {
                        device_id, delta, ..
                    } => {
                        let delta = match delta {
                            MouseScrollDelta::LineDelta(_, _) => (0.0, 0.0), // TODO
                            MouseScrollDelta::PixelDelta(position) => {
                                let (dx, dy): (f64, f64) = position.into();
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
                    }) => pointers.touch(device_id, phase, location.into()),
                    WindowEvent::ReceivedCharacter(ch) => keyboard.character(ch),
                    WindowEvent::KeyboardInput { input, .. } => keyboard.input(&input),
                    _ => {}
                }
            }
            Event::DeviceEvent { event, .. } => match event {
                DeviceEvent::ModifiersChanged(modifiers) => keyboard.set_modifiers(modifiers),
                _ => {}
            },
            Event::Resumed => {
                let surface = unsafe {
                    Surface::new_from_existing_window(&self.config, &*self.window).unwrap()
                };
                *self.surface.lock() = Some(surface);
                self.engine.run(&arguments).unwrap();
                self.engine
                    .with_plugin(|localization: &LocalizationPlugin| {
                        localization.send_locale(locale_config::Locale::current());
                    });
            }
            Event::Suspended => {
                self.engine.shutdown();
                self.surface.lock().take();
            }
            Event::RedrawRequested(_) => {
                self.resize();
            }
            Event::MainEventsCleared => {
                self.window.request_redraw();
            }
            _ => {
                if self.close.load(Ordering::Relaxed) {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                let next_task_time = self.engine.execute_platform_tasks();
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
