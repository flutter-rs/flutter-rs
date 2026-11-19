//! Generates the raw embedder bindings from the vendored `flutter-engine.h`
//! and tells Cargo where the engine library can be found.

use bindgen::EnumVariation;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Directory holding the engine library (`libflutter_engine.so`,
/// `flutter_engine.dll`, `FlutterEmbedder.framework`).
const ENGINE_LIB_DIR: &str = "FLUTTER_ENGINE_LIB_DIR";

/// Environment variables that may point at an Android NDK, most specific first.
const NDK_VARS: [&str; 3] = ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "NDK_HOME"];

fn main() {
    // Cargo only tracks the inputs a build script names, so this list has to be
    // complete: `build.rs` and the vendored header are the only files bindgen
    // reads today, and any header added later must be listed here or the
    // bindings will silently go stale.
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=flutter-engine.h");
    println!("cargo::rerun-if-env-changed={ENGINE_LIB_DIR}");
    println!("cargo::rerun-if-env-changed=SDKROOT");
    println!("cargo::rustc-check-cfg=cfg(flutter_embedder_framework)");
    for var in NDK_VARS {
        println!("cargo::rerun-if-env-changed={var}");
    }

    emit_link_search();

    let target = std::env::var("TARGET").expect("TARGET is set by cargo");
    let mut clang_args: Vec<String> = Vec::new();

    // This adds the sysroot specific to the android NDK
    if target.contains("android") {
        clang_args.push("--sysroot".into());
        clang_args.push(android_sysroot());
    }

    // This adds the sysroot specific to the apple SDK for clang.
    if let Some(sdk_path) = sdk_path(&target) {
        clang_args.push("-isysroot".into());
        clang_args.push(sdk_path);
    }

    // https://github.com/rust-lang/rust-bindgen/issues/1211
    let target = if target == "aarch64-apple-ios" {
        String::from("arm64-apple-ios")
    } else {
        target
    };
    clang_args.push(format!("--target={target}"));

    let bindings = bindgen::Builder::default()
        .header("flutter-engine.h")
        .default_enum_style(EnumVariation::Rust {
            non_exhaustive: false,
        })
        // The embedder header ships extensive doc comments containing fenced
        // code and backticks; carrying them over makes rustdoc try to compile
        // them as doctests and fail, so drop comments from the bindings.
        .generate_comments(false)
        .clang_args(&clang_args)
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
    bindings
        .write_to_file(out_path.join("flutter-engine-sys.rs"))
        .expect("Couldn't write bindings!");
}

/// Forward `FLUTTER_ENGINE_LIB_DIR` to the linker, if it is set.
///
/// The engine library is neither shipped nor built by this crate, so the search
/// path has to come from the environment. Setting `FLUTTER_ENGINE_LIB_DIR`
/// composes with (and is equivalent to) passing `-L <dir>` through `RUSTFLAGS`:
/// both end up as `-L` on the final link, and neither is required if the
/// library is already on the platform's default search path.
fn emit_link_search() {
    let Ok(dir) = std::env::var(ENGINE_LIB_DIR) else {
        return;
    };
    if dir.is_empty() {
        return;
    }

    let Ok(dir) = Path::new(&dir).canonicalize() else {
        println!("cargo::warning={ENGINE_LIB_DIR} is set to `{dir}`, which does not exist");
        return;
    };
    if !dir.is_dir() {
        println!(
            "cargo::warning={ENGINE_LIB_DIR} is set to `{}`, which is not a directory",
            dir.display()
        );
        return;
    }

    // Apple's official artifact is `FlutterEmbedder.framework`, not a
    // `libflutter_engine.dylib`. Link it as a framework when it is what the
    // caller pointed us at; the `#[link]` attribute in `src/lib.rs` is gated on
    // this cfg.
    let apple = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "macos" || os == "ios");
    if apple && dir.join("FlutterEmbedder.framework").is_dir() {
        println!("cargo::rustc-cfg=flutter_embedder_framework");
        println!("cargo::rustc-link-search=framework={}", dir.display());
        let dir = dir.display().to_string();
        if !dir.contains(',') {
            println!("cargo::rustc-link-arg=-Wl,-rpath,{dir}");
        }
        return;
    }

    let dir = dir.display().to_string();
    println!("cargo::rustc-link-search=native={dir}");

    // `dlopen` and the dynamic loader both consult the calling binary's rpath,
    // so recording it here lets this crate's tests (and any example or binary
    // of this package) find the engine without `LD_LIBRARY_PATH`. MSVC and
    // MinGW have no equivalent and keep relying on `PATH`. A comma would be
    // parsed as another `-Wl` argument, so a path containing one is skipped.
    let windows = std::env::var("TARGET").is_ok_and(|target| target.contains("windows"));
    if !windows && !dir.contains(',') {
        println!("cargo::rustc-link-arg=-Wl,-rpath,{dir}");
    }
}

/// Locate the NDK sysroot clang needs to parse the embedder header for Android.
///
/// NDK r19 moved the sysroot into the bundled LLVM toolchain; the pre-r19
/// top-level `$NDK/sysroot` is still accepted so an old NDK keeps working.
fn android_sysroot() -> String {
    let ndk = NDK_VARS
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| {
            panic!("set one of {NDK_VARS:?} to the Android NDK directory to build for android")
        });
    let ndk = PathBuf::from(ndk);

    let candidates = [
        ndk.join("toolchains")
            .join("llvm")
            .join("prebuilt")
            .join(ndk_host_tag())
            .join("sysroot"),
        ndk.join("sysroot"),
    ];
    let sysroot = candidates
        .iter()
        .find(|path| path.is_dir())
        .unwrap_or_else(|| {
            panic!(
                "no NDK sysroot found; looked for {}",
                candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" and ")
            )
        });
    sysroot.display().to_string()
}

/// The NDK's name for the host the toolchain binaries were built for.
fn ndk_host_tag() -> &'static str {
    let host = std::env::var("HOST").unwrap_or_default();
    if host.contains("apple-darwin") {
        // The NDK only ships an x86_64 macOS toolchain; Apple silicon hosts run
        // it under Rosetta.
        "darwin-x86_64"
    } else if host.contains("windows") {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    }
}

/// Path to the Apple SDK `target` needs, or `None` if it is not an Apple target
/// or no SDK could be located.
fn sdk_path(target: &str) -> Option<String> {
    let sdk = if target.contains("apple-darwin") {
        "macosx"
    } else if target == "x86_64-apple-ios" || target == "i386-apple-ios" {
        "iphonesimulator"
    } else if target == "aarch64-apple-ios" || target == "armv7-apple-ios" {
        "iphoneos"
    } else {
        return None;
    };

    // Cross-compiling to an Apple target from a non-Apple host has no `xcrun`,
    // so honour `SDKROOT` (what Xcode itself exports) and otherwise warn and
    // leave `-isysroot` off rather than aborting the build.
    if let Ok(sdkroot) = std::env::var("SDKROOT")
        && !sdkroot.is_empty()
    {
        return Some(sdkroot);
    }

    let output = match Command::new("xcrun")
        .args(["--sdk", sdk, "--show-sdk-path"])
        .output()
    {
        Ok(output) if output.status.success() => output.stdout,
        Ok(output) => {
            println!(
                "cargo::warning=`xcrun --sdk {sdk} --show-sdk-path` failed ({}); \
                 set SDKROOT to the {sdk} sdk path",
                output.status
            );
            return None;
        }
        Err(error) => {
            println!(
                "cargo::warning=could not run `xcrun` ({error}); \
                 set SDKROOT to the {sdk} sdk path"
            );
            return None;
        }
    };

    let path = std::str::from_utf8(&output).ok()?.trim_end();
    if path.is_empty() {
        None
    } else {
        Some(path.to_owned())
    }
}
