use crate::context::Context;
use crate::handler::{WinitFlutterEngineHandler, WinitPlatformHandler, WinitWindowHandler};
use failure::Error;
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
use flutter_plugins::keyevent::KeyEventPlugin;
use flutter_plugins::lifecycle::LifecyclePlugin;
use flutter_plugins::localization::LocalizationPlugin;
use flutter_plugins::navigation::NavigationPlugin;
use flutter_plugins::platform::PlatformPlugin;
use flutter_plugins::settings::SettingsPlugin;
use flutter_plugins::system::SystemPlugin;
use flutter_plugins::textinput::TextInputPlugin;
use flutter_plugins::window::WindowPlugin;
use glutin::event::{ElementState, Event, MouseButton, Touch, TouchPhase, WindowEvent};
use glutin::event_loop::{ControlFlow, EventLoop};
use glutin::window::WindowBuilder;
use glutin::ContextBuilder;
use parking_lot::Mutex;
use std::path::Path;
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
}

impl FlutterWindow {
    pub fn new(window: WindowBuilder) -> Result<Self, Error> {
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
        let platform_handler = Arc::new(Mutex::new(Box::new(WinitPlatformHandler::new()) as _));
        let window_handler = Arc::new(Mutex::new(WinitWindowHandler::new()));

        engine.add_plugin(DialogPlugin::default());
        engine.add_plugin(IsolatePlugin::new(isolate_cb));
        engine.add_plugin(KeyEventPlugin::default());
        engine.add_plugin(LifecyclePlugin::default());
        engine.add_plugin(LocalizationPlugin::default());
        engine.add_plugin(NavigationPlugin::default());
        engine.add_plugin(PlatformPlugin::new(platform_handler.clone()));
        engine.add_plugin(SettingsPlugin::default());
        engine.add_plugin(SystemPlugin::default());
        engine.add_plugin(TextInputPlugin::default());
        engine.add_plugin(WindowPlugin::new(window_handler.clone()));

        Ok(Self {
            event_loop,
            context,
            resource_context,
            engine,
            engine_handler,
            texture_registry,
        })
    }

    pub fn with_resource_context(self) -> Result<Self, Error> {
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

    pub fn run(
        self,
        assets_path: &Path,
        icu_data_path: &Path,
        arguments: &[&str],
    ) -> Result<(), Error> {
        self.engine.run(assets_path, icu_data_path, arguments)?;
        let engine = self.engine.clone();
        let context = self.context.clone();
        resize(&engine, &context);

        self.with_plugin(|localization: &LocalizationPlugin| {
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
                        WindowEvent::MouseWheel { .. } => {
                            engine.send_pointer_event(
                                // TODO
                                FlutterPointerPhase::Hover,
                                cursor,
                                FlutterPointerSignalKind::Scroll,
                                (0.0, 0.0), // TODO
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
                        _ => {}
                    }
                }
                Event::LoopDestroyed => {
                    engine.shutdown();
                }
                _ => {
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
    let context = context.lock();
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
