#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/flutter-engine-sys.rs"));

#[cfg(target_os = "linux")]
#[link(name = "flutter_linux")]
extern {}

#[cfg(target_os = "macos")]
#[link(name = "FlutterMacOS", kind = "framework")]
extern {}

#[cfg(target_os = "windows")]
#[link(name = "flutter_windows.dll")]
extern {}
