use crate::{
    channel::Channel,
    ffi::{FlutterEngine, FlutterPointerPhase, FlutterPointerSignalKind},
    plugins::PluginRegistrar,
    utils::WindowUnwrap,
};

use log::{debug, info};
use std::sync::{
    mpsc,
    mpsc::{Receiver, Sender},
    Arc,
};
use tokio::runtime::{Runtime, TaskExecutor};

const SCROLL_SPEED: f64 = 50.0; // seems to be about 2.5 lines of text
#[cfg(not(target_os = "macos"))]
const BY_WORD_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Control;
#[cfg(target_os = "macos")]
const BY_WORD_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Alt;
const SELECT_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Shift;
#[cfg(not(target_os = "macos"))]
const FUNCTION_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Control;
#[cfg(target_os = "macos")]
const FUNCTION_MODIFIER_KEY: glfw::Modifiers = glfw::Modifiers::Super;

pub type MainThreadFn = Box<FnMut(&mut glfw::Window) + Send>;
pub type ChannelFn = (&'static str, Box<FnMut(&Channel) + Send>);

pub struct DesktopWindowState {
    window_ref: *mut glfw::Window,
    pub window_event_receiver: Receiver<(f64, glfw::WindowEvent)>,
    pub runtime: Runtime,
    pub main_thread_receiver: Receiver<MainThreadFn>,
    pub channel_receiver: Receiver<ChannelFn>,
    pub init_data: Arc<InitData>,
    pointer_currently_added: bool,
    window_pixels_per_screen_coordinate: f64,
    pub plugin_registrar: PluginRegistrar,
}

/// Data accessible during initialization and on the main thread.
pub struct InitData {
    pub engine: Arc<FlutterEngine>,
    pub runtime_data: Arc<RuntimeData>,
}

/// Data accessible during runtime. Implements Send to be used in message handling.
#[derive(Clone)]
pub struct RuntimeData {
    main_thread_sender: Sender<MainThreadFn>,
    pub(crate) channel_sender: Sender<ChannelFn>,
    pub task_executor: TaskExecutor,
}

impl RuntimeData {
    pub fn with_window_result<F, R>(&self, mut f: F) -> Result<R, crate::error::MethodCallError>
    where
        F: FnMut(&mut glfw::Window) -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        self.main_thread_sender.send(Box::new(move |window| {
            let result = f(window);
            tx.send(result).unwrap();
        }))?;
        Ok(rx.recv()?)
    }

    pub fn with_window<F>(&self, mut f: F) -> Result<(), crate::error::MethodCallError>
    where
        F: FnMut(&mut glfw::Window) + Send + 'static,
    {
        self.main_thread_sender.send(Box::new(move |window| {
            f(window);
        }))?;
        Ok(())
    }

    pub fn with_channel<F>(
        &self,
        channel_name: &'static str,
        mut f: F,
    ) -> Result<(), crate::error::MethodCallError>
    where
        F: FnMut(&Channel) + Send + 'static,
    {
        self.channel_sender.send((
            channel_name,
            Box::new(move |channel| {
                f(channel);
            }),
        ))?;
        Ok(())
    }
}

impl DesktopWindowState {
    pub fn window(&mut self) -> &mut glfw::Window {
        self.window_ref.window()
    }

    pub fn new(
        window_ref: *mut glfw::Window,
        window_event_receiver: Receiver<(f64, glfw::WindowEvent)>,
        engine: FlutterEngine,
    ) -> Self {
        let runtime = Runtime::new().unwrap();
        let (main_tx, main_rx) = mpsc::channel();
        let (channel_tx, channel_rx) = mpsc::channel();
        let runtime_data = Arc::new(RuntimeData {
            main_thread_sender: main_tx,
            channel_sender: channel_tx,
            task_executor: runtime.executor(),
        });
        let init_data = Arc::new(InitData {
            engine: Arc::new(engine),
            runtime_data,
        });
        Self {
            window_ref,
            window_event_receiver,
            runtime,
            main_thread_receiver: main_rx,
            channel_receiver: channel_rx,
            pointer_currently_added: false,
            window_pixels_per_screen_coordinate: 0.0,
            plugin_registrar: PluginRegistrar::new(Arc::downgrade(&init_data)),
            init_data,
        }
    }

    pub fn send_scale_or_size_change(&mut self) {
        let window = self.window();
        let window_size = window.get_size();
        let framebuffer_size = window.get_framebuffer_size();
        let scale = window.get_content_scale();
        self.window_pixels_per_screen_coordinate = framebuffer_size.0 as f64 / window_size.0 as f64;
        debug!(
            "Setting framebuffer size to {:?}, scale to {}",
            framebuffer_size, scale.0
        );
        self.init_data.engine.send_window_metrics_event(
            framebuffer_size.0,
            framebuffer_size.1,
            scale.0 as f64,
        );
    }

    fn send_pointer_event(
        &mut self,
        phase: FlutterPointerPhase,
        x: f64,
        y: f64,
        signal_kind: FlutterPointerSignalKind,
        scroll_delta_x: f64,
        scroll_delta_y: f64,
    ) {
        if !self.pointer_currently_added && phase != FlutterPointerPhase::Add {
            self.send_pointer_event(
                FlutterPointerPhase::Add,
                x,
                y,
                FlutterPointerSignalKind::None,
                0.0,
                0.0,
            );
        }
        if self.pointer_currently_added && phase == FlutterPointerPhase::Add {
            return;
        }

        self.init_data.engine.send_pointer_event(
            phase,
            x * self.window_pixels_per_screen_coordinate,
            y * self.window_pixels_per_screen_coordinate,
            signal_kind,
            scroll_delta_x * self.window_pixels_per_screen_coordinate,
            scroll_delta_y * self.window_pixels_per_screen_coordinate,
        );

        match phase {
            FlutterPointerPhase::Add => self.pointer_currently_added = true,
            FlutterPointerPhase::Remove => self.pointer_currently_added = false,
            _ => {}
        }
    }

    pub fn handle_glfw_event(&mut self, event: glfw::WindowEvent) {
        match event {
            glfw::WindowEvent::CursorEnter(entered) => {
                let cursor_pos = self.window().get_cursor_pos();
                self.send_pointer_event(
                    if entered {
                        FlutterPointerPhase::Add
                    } else {
                        FlutterPointerPhase::Remove
                    },
                    cursor_pos.0,
                    cursor_pos.1,
                    FlutterPointerSignalKind::None,
                    0.0,
                    0.0,
                );
            }
            glfw::WindowEvent::CursorPos(x, y) => {
                let phase = if self.window().get_mouse_button(glfw::MouseButtonLeft)
                    == glfw::Action::Press
                {
                    FlutterPointerPhase::Move
                } else {
                    FlutterPointerPhase::Hover
                };
                self.send_pointer_event(phase, x, y, FlutterPointerSignalKind::None, 0.0, 0.0);
            }
            glfw::WindowEvent::MouseButton(glfw::MouseButtonLeft, action, _modifiers) => {
                let (x, y) = self.window().get_cursor_pos();
                let phase = if action == glfw::Action::Press {
                    FlutterPointerPhase::Down
                } else {
                    FlutterPointerPhase::Up
                };
                self.send_pointer_event(phase, x, y, FlutterPointerSignalKind::None, 0.0, 0.0);
            }
            glfw::WindowEvent::MouseButton(
                glfw::MouseButton::Button4,
                glfw::Action::Press,
                _modifiers,
            ) => {
                self.plugin_registrar.with_plugin(
                    |navigation: &crate::plugins::NavigationPlugin| {
                        navigation.pop_route();
                    },
                );
            }
            glfw::WindowEvent::Scroll(scroll_delta_x, scroll_delta_y) => {
                let (x, y) = self.window().get_cursor_pos();
                let phase = if self.window().get_mouse_button(glfw::MouseButtonLeft)
                    == glfw::Action::Press
                {
                    FlutterPointerPhase::Move
                } else {
                    FlutterPointerPhase::Hover
                };
                self.send_pointer_event(
                    phase,
                    x,
                    y,
                    FlutterPointerSignalKind::Scroll,
                    scroll_delta_x * SCROLL_SPEED,
                    -scroll_delta_y * SCROLL_SPEED,
                );
            }
            glfw::WindowEvent::FramebufferSize(_, _) => {
                self.send_scale_or_size_change();
            }
            glfw::WindowEvent::ContentScale(_, _) => {
                self.send_scale_or_size_change();
            }
            glfw::WindowEvent::Char(char) => self.plugin_registrar.with_plugin_mut(
                |text_input: &mut crate::plugins::TextInputPlugin| {
                    text_input.with_state(|state| {
                        state.add_characters(&char.to_string());
                    });
                    text_input.notify_changes();
                },
            ),
            glfw::WindowEvent::Key(key, _, glfw::Action::Press, modifiers)
            | glfw::WindowEvent::Key(key, _, glfw::Action::Repeat, modifiers) => match key {
                glfw::Key::Enter => self.plugin_registrar.with_plugin_mut(
                    |text_input: &mut crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.add_characters(&"\n");
                        });
                        text_input.notify_changes();
                    },
                ),
                //                Key::Enter => {
                //                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                //                        if modifiers.contains(Modifiers::Control) {
                //                            p.perform_action("done");
                //                        } else {
                //                            // TODO
                //                            // why add_char plus newline action?
                //                            p.add_chars("\n");
                //                            p.perform_action("newline");
                //                        }
                //                    });
                //                },
                //                Key::Up => {
                //                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                //                        p.move_cursor_up(modifiers);
                //                    });
                //                },
                //                Key::Down => {
                //                    FlutterEngine::with_plugin(window.window_ptr(), "flutter/textinput", |p: &Box<TextInputPlugin>| {
                //                        p.move_cursor_down(modifiers);
                //                    });
                //                },
                glfw::Key::Backspace => self.plugin_registrar.with_plugin_mut(
                    |text_input: &mut crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.backspace();
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::Delete => self.plugin_registrar.with_plugin_mut(
                    |text_input: &mut crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.delete();
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::Left => self.plugin_registrar.with_plugin_mut(
                    |text_input: &mut crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.move_left(
                                modifiers.contains(BY_WORD_MODIFIER_KEY),
                                modifiers.contains(SELECT_MODIFIER_KEY),
                            );
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::Right => self.plugin_registrar.with_plugin_mut(
                    |text_input: &mut crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.move_right(
                                modifiers.contains(BY_WORD_MODIFIER_KEY),
                                modifiers.contains(SELECT_MODIFIER_KEY),
                            );
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::Home => self.plugin_registrar.with_plugin_mut(
                    |text_input: &mut crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.move_to_beginning(modifiers.contains(SELECT_MODIFIER_KEY));
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::End => self.plugin_registrar.with_plugin_mut(
                    |text_input: &mut crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.move_to_end(modifiers.contains(SELECT_MODIFIER_KEY));
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::A => {
                    if modifiers.contains(FUNCTION_MODIFIER_KEY) {
                        self.plugin_registrar.with_plugin_mut(
                            |text_input: &mut crate::plugins::TextInputPlugin| {
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
                        let window = self.window_ref.window();
                        self.plugin_registrar.with_plugin_mut(
                            |text_input: &mut crate::plugins::TextInputPlugin| {
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
                        let window = self.window_ref.window();
                        self.plugin_registrar.with_plugin_mut(
                            |text_input: &mut crate::plugins::TextInputPlugin| {
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
                        let window = self.window_ref.window();
                        self.plugin_registrar.with_plugin_mut(
                            |text_input: &mut crate::plugins::TextInputPlugin| {
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
            },
            _ => {}
        }
    }

    pub fn with_window_and_plugin_mut_result<F, P, R>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&mut glfw::Window, &mut P) -> R,
        P: crate::plugins::Plugin + 'static,
    {
        let window = self.window_ref.window();
        let mut result = None;
        self.plugin_registrar.with_plugin_mut(|p: &mut P| {
            result = Some(f(window, p));
        });

        result
    }
}
