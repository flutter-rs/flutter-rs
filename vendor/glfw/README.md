<!--
    Copyright 2013-2014 The GLFW-RS Developers. For a full listing of the authors,
    refer to the AUTHORS file at the top-level directory of this distribution.

    Licensed under the Apache License, Version 2.0 (the "License");
    you may not use this file except in compliance with the License.
    You may obtain a copy of the License at

        http://www.apache.org/licenses/LICENSE-2.0

    Unless required by applicable law or agreed to in writing, software
    distributed under the License is distributed on an "AS IS" BASIS,
    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
    See the License for the specific language governing permissions and
    limitations under the License.
-->

# glfw-rs

[![Crates.io](https://img.shields.io/crates/v/glfw)](https://crates.io/crates/glfw)
[![Docs.rs](https://docs.rs/glfw/badge.svg)](https://docs.rs/glfw)
[![Build Status](https://travis-ci.org/PistonDevelopers/glfw-rs.svg?branch=master)](https://travis-ci.org/PistonDevelopers/glfw-rs)

GLFW bindings and wrapper for The Rust Programming Language.

## Example

~~~rust
extern crate glfw;

use glfw::{Action, Context, Key};

fn main() {
    let mut glfw = glfw::init(glfw::fail_on_errors).unwrap();

    let (mut window, events) = glfw.create_window(300, 300, "Hello this is window", glfw::WindowMode::Windowed)
        .expect("Failed to create GLFW window.");

    window.set_key_polling(true);
    window.make_current();

    while !window.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            handle_window_event(&mut window, event);
        }
    }
}

fn handle_window_event(window: &mut glfw::Window, event: glfw::WindowEvent) {
    match event {
        glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
            window.set_should_close(true)
        }
        _ => {}
    }
}
~~~

## Using glfw-rs

### Prerequisites

Make sure you have [compiled and installed GLFW 3.x](http://www.glfw.org/docs/latest/compile.html).
You might be able to find it on your package manager, for example on OS X:
`brew install glfw3` (you may need to run `brew tap homebrew/versions`).
If not you can download and build the library
[from the source](http://www.glfw.org/docs/latest/compile.html) supplied on the
GLFW website. Note that if you compile GLFW with CMake on Linux, you will have
to supply the `-DCMAKE_C_FLAGS=-fPIC` argument. You may install GLFW to your
`PATH`, otherwise you will have to specify the directory containing the library
binaries when you call `make` or `make lib`:

~~~
GLFW_LIB_DIR=path/to/glfw/lib/directory make
~~~

### Including glfw-rs in your project

Add this to your `Cargo.toml`:

~~~toml
[dependencies.glfw]
version = "*"
~~~

#### On Windows

By default, `glfw-rs` will try to compile the `glfw` library. If you want to link to your custom
build of `glfw` or if the build doesn't work (which is probably the case on Windows), you can
disable this:

~~~toml
[dependencies.glfw]
version = "*"
default-features = false
~~~

### Raw window handle 0.5.0 compatibility

By default, `glfw-rs` uses raw-window-handle at v0.6.0. If your project is depending on `glfw-rs`
with raw-window-handle v0.5.0, then use this line in your Cargo.toml:
~~~
glfw = { version = 0.56.0 , default-features = false, features = ["with-window-handle-v0-5"] }
~~~

## Support

Contact `bjz` on irc.mozilla.org [#rust](http://mibbit.com/?server=irc.mozilla.org&channel=%23rust)
and [#rust-gamedev](http://mibbit.com/?server=irc.mozilla.org&channel=%23rust-gamedev),
or [post an issue](https://github.com/bjz/glfw-rs/issues/new) on GitHub.

## glfw-rs in use

- [sebcrozet/kiss3d](https://github.com/sebcrozet/kiss3d)
- [Jeaye/q3](https://github.com/Jeaye/q3)
- [cyndis/rsmc](https://github.com/cyndis/rsmc/)
- [ozkriff/zoc](https://github.com/ozkriff/zoc)

## glfw-rs with other graphical APIs

In certain circumstances OpenGL which GLFW uses can conflict with the new handle that the 
graphical API is also trying to use, causing crashes, to fix this add
~~~rust
glfw.window_hint(WindowHint::ClientApi(ClientApiHint::NoApi));
~~~
before creating the window. If using this however you cannot use certain built-in functions, 
such as `window.swap_buffers()`, `window.make_current()`, and `glfw.set_swap_interval()`, but
these should have equivalents provided by the graphical API.