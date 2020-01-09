use bindgen::EnumVariation;
use std::path::PathBuf;

fn main() {
    let target = std::env::var("TARGET").unwrap();
    let mut clang_args: Vec<String> = Vec::new();

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
    clang_args.push(format!("--target={}", target));

    let bindings = bindgen::Builder::default()
        .header("flutter-engine.h")
        .default_enum_style(EnumVariation::Rust {
            non_exhaustive: false,
        })
        .clang_args(&clang_args)
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("flutter-engine-sys.rs"))
        .expect("Couldn't write bindings!");
}

fn sdk_path(target: &str) -> Option<String> {
    use std::process::Command;

    let sdk = if target.contains("apple-darwin") {
        "macosx"
    } else if target == "x86_64-apple-ios" || target == "i386-apple-ios" {
        "iphonesimulator"
    } else if target == "aarch64-apple-ios" {
        "iphoneos"
    } else {
        return None;
    };

    let output = Command::new("xcrun")
        .args(&["--sdk", sdk, "--show-sdk-path"])
        .output()
        .expect("xcrun command failed")
        .stdout;
    let prefix_str = std::str::from_utf8(&output).expect("invalid output from `xcrun`");
    Some(prefix_str.trim_end().to_string())
}
