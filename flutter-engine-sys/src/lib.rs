#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::unreadable_literal)]

include!(concat!(env!("OUT_DIR"), "/flutter-engine-sys.rs"));

#[cfg(target_os = "android")]
#[link(name = "flutter_engine")]
extern "C" {}

#[cfg(target_os = "ios")]
#[link(name = "flutter_engine")]
extern "C" {}

#[cfg(target_os = "linux")]
#[link(name = "flutter_engine")]
extern "C" {}

#[cfg(target_os = "macos")]
#[link(name = "flutter_engine")]
extern "C" {}

#[cfg(target_os = "windows")]
#[link(name = "flutter_engine.dll")]
extern "C" {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link() {
        println!("linking works");
    }
}
