//! Raw FFI bindings to the Flutter engine's embedder API.
//!
//! The bindings are generated at build time by `bindgen` from the vendored
//! `flutter-engine.h` (Flutter 3.47.8), so the declarations here always match
//! the header in this repository rather than whatever engine happens to be
//! installed. Nothing in this crate is safe to call on its own: the embedder
//! API has hard threading rules and a panic unwinding out of one of its
//! callbacks aborts the process. Use the safe `flutter-engine` wrapper.
//!
//! # Linking
//!
//! This crate declares `-lflutter_engine`; it neither ships nor builds the
//! engine. Point the linker at a directory containing the engine library in
//! either of two interchangeable ways:
//!
//! * set `FLUTTER_ENGINE_LIB_DIR` for the build, which makes `build.rs` emit a
//!   `cargo::rustc-link-search` for it (and, on non-Windows targets, an rpath
//!   so this package's test binaries also find the library at run time), or
//! * pass `-L <dir>` yourself through `RUSTFLAGS`.
//!
//! Neither helps the dynamic loader of a *downstream* binary, which still needs
//! the directory on `LD_LIBRARY_PATH` (Linux), `DYLD_LIBRARY_PATH` (macOS) or
//! `PATH` (Windows).
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::unreadable_literal)]

include!(concat!(env!("OUT_DIR"), "/flutter-engine-sys.rs"));

// One declaration for every platform that has an engine build, instead of five
// identical blocks. The `cfg_attr` keeps targets with no engine at all (wasm32,
// say) type-checkable, since they never receive the link directive.
#[cfg_attr(
    any(
        target_os = "android",
        target_os = "ios",
        target_os = "linux",
        target_os = "macos",
        target_os = "windows"
    ),
    link(name = "flutter_engine")
)]
unsafe extern "C" {}

#[cfg(test)]
mod tests {
    use libloading::Library;

    /// The engine library's file name as the dynamic loader sees it.
    ///
    /// Written as one `cfg!`-selected constant so that every target defines it;
    /// a `#[cfg]` ladder left the android/ios test targets failing to compile.
    const LIB: &str = if cfg!(any(target_os = "macos", target_os = "ios")) {
        "libflutter_engine.dylib"
    } else if cfg!(target_os = "windows") {
        // Not `flutter_engine.lib`: that is the import library, which the
        // loader cannot open.
        "flutter_engine.dll"
    } else {
        "libflutter_engine.so"
    };

    /// Check that the engine we link against really exports the embedder entry
    /// points the safe wrapper calls.
    ///
    /// This is an environment probe rather than a unit test: it needs a real
    /// `libflutter_engine` on the loader path (see the crate docs).
    #[test]
    fn link() {
        // SAFETY: opening a shared library runs its initialisers. The engine is
        // a well-behaved library and this is the same load the linked-in
        // `-lflutter_engine` performs at process start, so it is already
        // mapped by the time this runs.
        let lib = unsafe { Library::new(LIB) }.unwrap_or_else(|error| {
            panic!(
                "`{LIB}` must be on the loader path \
                 (LD_LIBRARY_PATH / DYLD_LIBRARY_PATH / PATH): {error}"
            )
        });

        for symbol in [
            c"FlutterEngineInitialize",
            c"FlutterEngineRunInitialized",
            c"FlutterEngineGetCurrentTime",
        ] {
            // SAFETY: these are embedder entry points, so typing them as bare
            // `extern "C" fn()` is a lie we never act on — the pointer is only
            // checked for existence, never called.
            unsafe { lib.get::<unsafe extern "C" fn()>(symbol.to_bytes_with_nul()) }
                .unwrap_or_else(|error| panic!("`{LIB}` must export {symbol:?}: {error}"));
        }
    }
}
