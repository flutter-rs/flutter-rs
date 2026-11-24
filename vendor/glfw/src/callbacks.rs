// Copyright 2013 The GLFW-RS Developers. For a full listing of the authors,
// refer to the AUTHORS file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Private callback support functions.

macro_rules! callback (
    (
        args -> ($($args:ty),*),
        glfw -> $glfw:ident ($($glfw_arg_names:ident: $glfw_args:ty),*),
        convert_args -> ($($convert_args:expr),*)
    ) => (
        thread_local!(static CALLBACK_KEY: RefCell<Option<Box<dyn FnMut($($args),*)>>> = RefCell::new(None));

        pub fn set<T>(f: T)
        where T: FnMut($($args),*) + 'static
        {
            let boxed_cb = Some(Box::new(f) as Box<dyn FnMut($($args),*)>);
            CALLBACK_KEY.with(|cb| {
                *cb.borrow_mut() = boxed_cb;
            });
            unsafe {
                crate::ffi::$glfw(Some(callback));
            }
        }

        pub fn unset() {
            CALLBACK_KEY.with(|cb| {
                *cb.borrow_mut() = None;
            });
             unsafe {
                crate::ffi::$glfw(None);
            }
        }

        extern "C" fn callback($($glfw_arg_names: $glfw_args),*) {
            CALLBACK_KEY.with(|cb| {
                match *cb.borrow_mut() {
                    Some(ref mut cb) => unsafe { cb($($convert_args),*) },
                    _ => {}
                }
            })
        }
    )
);

pub mod error {
    use std::cell::RefCell;
    use std::os::raw::{c_char, c_int};

    callback!(
        args -> (crate::Error, String),
        glfw -> glfwSetErrorCallback(error: c_int, description: *const c_char),
        convert_args -> (crate::Error::from_raw(error), crate::string_from_c_str(description))
    );
}

pub mod monitor {
    use std::cell::RefCell;
    use std::mem;
    use std::os::raw::c_int;

    callback!(
        args -> (crate::Monitor, crate::MonitorEvent),
        glfw -> glfwSetMonitorCallback(monitor: *mut crate::ffi::GLFWmonitor, event: c_int),
        convert_args -> (
            crate::Monitor { ptr: monitor },
            mem::transmute(event)
        )
    );
}

pub mod joystick {
    use std::cell::RefCell;
    use std::mem;
    use std::os::raw::c_int;

    callback!(
        args -> (crate::JoystickId, crate::JoystickEvent),
        glfw -> glfwSetJoystickCallback(joystick_id: c_int, event: c_int),
        convert_args -> (mem::transmute(joystick_id), mem::transmute(event))
    );
}

pub mod unbuffered {
    use crate::{WindowEvent, WindowId};
    use std::cell::RefCell;

    type CallbackPtr = *mut std::ffi::c_void;
    type HandlerFn = fn(
        window_id: WindowId,
        event: (f64, WindowEvent),
        callback_ptr: CallbackPtr,
    ) -> Option<(f64, WindowEvent)>;

    thread_local! {
        static HANDLER: RefCell<Option<(HandlerFn, CallbackPtr)>> = RefCell::new(None);
    }

    pub struct UnsetHandlerGuard {
        _private: (),
    }

    impl Drop for UnsetHandlerGuard {
        fn drop(&mut self) {
            HANDLER.with(|ref_cell| {
                *ref_cell.borrow_mut() = None;
            })
        }
    }

    pub unsafe fn handle(
        window_id: WindowId,
        event: (f64, WindowEvent),
    ) -> Option<(f64, WindowEvent)> {
        HANDLER.with(|ref_cell| {
            if let Some((handler, callback_ptr)) = *ref_cell.borrow() {
                handler(window_id, event, callback_ptr)
            } else {
                Some(event)
            }
        })
    }

    pub unsafe fn set_handler<F>(callback: &mut F) -> UnsetHandlerGuard
    where
        F: FnMut(WindowId, (f64, WindowEvent)) -> Option<(f64, WindowEvent)>,
    {
        fn handler<F>(
            window_id: WindowId,
            event: (f64, WindowEvent),
            callback_ptr: CallbackPtr,
        ) -> Option<(f64, WindowEvent)>
        where
            F: FnMut(WindowId, (f64, WindowEvent)) -> Option<(f64, WindowEvent)>,
        {
            unsafe {
                let callback: &mut F = &mut *(callback_ptr as *mut F);
                callback(window_id, event)
            }
        }
        HANDLER.with(|ref_cell| {
            let callback_ptr = callback as *mut F as CallbackPtr;
            *ref_cell.borrow_mut() = Some((handler::<F>, callback_ptr));
        });
        UnsetHandlerGuard { _private: () }
    }
}
