use crate::{
    ffi::{FlutterEngine, FlutterPointerPhase, FlutterPointerSignalKind},
    plugins::PluginRegistrar,
};

use std::sync::{mpsc::Receiver, Arc};

use log::info;

const DP_PER_INCH: f64 = 160.0;
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

pub struct DesktopWindowState {
    pub runtime_data: Arc<RuntimeData>,
    pointer_currently_added: bool,
    monitor_screen_coordinates_per_inch: f64,
    window_pixels_per_screen_coordinate: f64,
    pub plugin_registrar: PluginRegistrar,
}

pub struct RuntimeData {
    window: *mut glfw::Window,
    pub window_event_receiver: Receiver<(f64, glfw::WindowEvent)>,
    pub engine: Arc<FlutterEngine>,
}

impl RuntimeData {
    #[allow(clippy::mut_from_ref)]
    pub fn window(&self) -> &mut glfw::Window {
        unsafe { &mut *self.window }
    }
}

impl DesktopWindowState {
    pub fn new(
        window_ref: *mut glfw::Window,
        window_event_receiver: Receiver<(f64, glfw::WindowEvent)>,
        engine: FlutterEngine,
    ) -> Self {
        let runtime_data = Arc::new(RuntimeData {
            window: window_ref,
            window_event_receiver,
            engine: Arc::new(engine),
        });
        let monitor_screen_coordinates_per_inch =
            Self::get_screen_coordinates_per_inch(&mut runtime_data.window().glfw);
        Self {
            pointer_currently_added: false,
            monitor_screen_coordinates_per_inch,
            window_pixels_per_screen_coordinate: 0.0,
            plugin_registrar: PluginRegistrar::new(Arc::downgrade(&runtime_data)),
            runtime_data,
        }
    }

    pub fn send_framebuffer_size_change(&mut self, framebuffer_size: (i32, i32)) {
        let window_size = self.runtime_data.window().get_size();
        self.window_pixels_per_screen_coordinate = framebuffer_size.0 as f64 / window_size.0 as f64;
        let dpi =
            self.window_pixels_per_screen_coordinate * self.monitor_screen_coordinates_per_inch;
        let pixel_ratio = (dpi / DP_PER_INCH).max(1.0);
        self.runtime_data.engine.send_window_metrics_event(
            framebuffer_size.0,
            framebuffer_size.1,
            pixel_ratio,
        );
    }

    fn get_screen_coordinates_per_inch(glfw: &mut glfw::Glfw) -> f64 {
        glfw.with_primary_monitor(|glfw, monitor| match monitor {
            None => DP_PER_INCH,
            Some(monitor) => match monitor.get_video_mode() {
                None => DP_PER_INCH,
                Some(video_mode) => {
                    let (width, _) = monitor.get_physical_size();
                    video_mode.width as f64 / (width as f64 / 25.4)
                }
            },
        })
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

        self.runtime_data.engine.send_pointer_event(
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
                let cursor_pos = self.runtime_data.window().get_cursor_pos();
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
                let phase = if self
                    .runtime_data
                    .window()
                    .get_mouse_button(glfw::MouseButtonLeft)
                    == glfw::Action::Press
                {
                    FlutterPointerPhase::Move
                } else {
                    FlutterPointerPhase::Hover
                };
                self.send_pointer_event(phase, x, y, FlutterPointerSignalKind::None, 0.0, 0.0);
            }
            glfw::WindowEvent::MouseButton(glfw::MouseButtonLeft, action, _modifiers) => {
                let (x, y) = self.runtime_data.window().get_cursor_pos();
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
                let (x, y) = self.runtime_data.window().get_cursor_pos();
                let phase = if self
                    .runtime_data
                    .window()
                    .get_mouse_button(glfw::MouseButtonLeft)
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
            glfw::WindowEvent::FramebufferSize(width, height) => {
                self.send_framebuffer_size_change((width, height));
            }
            glfw::WindowEvent::Char(char) => {
                self.plugin_registrar
                    .with_plugin(|text_input: &crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.add_characters(&char.to_string());
                        });
                        text_input.notify_changes();
                    })
            }
            glfw::WindowEvent::Key(key, _, glfw::Action::Press, modifiers)
            | glfw::WindowEvent::Key(key, _, glfw::Action::Repeat, modifiers) => match key {
                glfw::Key::Enter => self.plugin_registrar.with_plugin(
                    |text_input: &crate::plugins::TextInputPlugin| {
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
                glfw::Key::Backspace => self.plugin_registrar.with_plugin(
                    |text_input: &crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.backspace();
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::Delete => self.plugin_registrar.with_plugin(
                    |text_input: &crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.delete();
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::Left => self.plugin_registrar.with_plugin(
                    |text_input: &crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.move_left(
                                modifiers.contains(BY_WORD_MODIFIER_KEY),
                                modifiers.contains(SELECT_MODIFIER_KEY),
                            );
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::Right => self.plugin_registrar.with_plugin(
                    |text_input: &crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.move_right(
                                modifiers.contains(BY_WORD_MODIFIER_KEY),
                                modifiers.contains(SELECT_MODIFIER_KEY),
                            );
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::Home => self.plugin_registrar.with_plugin(
                    |text_input: &crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.move_to_beginning(modifiers.contains(SELECT_MODIFIER_KEY));
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::End => self.plugin_registrar.with_plugin(
                    |text_input: &crate::plugins::TextInputPlugin| {
                        text_input.with_state(|state| {
                            state.move_to_end(modifiers.contains(SELECT_MODIFIER_KEY));
                        });
                        text_input.notify_changes();
                    },
                ),
                glfw::Key::A => {
                    if modifiers.contains(FUNCTION_MODIFIER_KEY) {
                        self.plugin_registrar.with_plugin(
                            |text_input: &crate::plugins::TextInputPlugin| {
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
                        self.plugin_registrar.with_plugin(
                            |text_input: &crate::plugins::TextInputPlugin| {
                                text_input.with_state(|state| {
                                    self.runtime_data
                                        .window()
                                        .set_clipboard_string(state.get_selected_text());
                                    state.delete_selected();
                                });
                                text_input.notify_changes();
                            },
                        )
                    }
                }
                glfw::Key::C => {
                    if modifiers.contains(FUNCTION_MODIFIER_KEY) {
                        self.plugin_registrar.with_plugin(
                            |text_input: &crate::plugins::TextInputPlugin| {
                                text_input.with_state(|state| {
                                    self.runtime_data
                                        .window()
                                        .set_clipboard_string(state.get_selected_text());
                                });
                                text_input.notify_changes();
                            },
                        )
                    }
                }
                glfw::Key::V => {
                    if modifiers.contains(FUNCTION_MODIFIER_KEY) {
                        self.plugin_registrar.with_plugin(
                            |text_input: &crate::plugins::TextInputPlugin| {
                                text_input.with_state(|state| {
                                    if let Some(text) =
                                        self.runtime_data.window().get_clipboard_string()
                                    {
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
}
