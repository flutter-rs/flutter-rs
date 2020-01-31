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
#[link(name = "flutter_engine")]
extern "C" {}

#[cfg(test)]
mod tests {
    #[allow(unused)]
    use super::*;
    use libloading::Library;

    #[cfg(target_os = "linux")]
    const LIB: &str = "libflutter_engine.so";
    #[cfg(target_os = "macos")]
    const LIB: &str = "libflutter_engine.dylib";
    #[cfg(target_os = "windows")]
    const LIB: &str = "flutter_engine.lib";

    #[test]
    fn link() {
        let lib = Library::new(LIB).unwrap();
        unsafe {
            lib.get::<*const ()>(b"gIcudtlData\0").unwrap();
            lib.get::<*const ()>(b"gIcudtlEnd\0").unwrap();
            lib.get::<*const ()>(b"gIcudtlSize\0").unwrap();
        }
    }
}
