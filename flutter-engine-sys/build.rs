extern crate bindgen;
extern crate flutter_download;

use bindgen::EnumVariation;
use flutter_download::get_flutter_version;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn gen_bindings() {
    let bindings = bindgen::Builder::default()
        .header("flutter-engine.h")
        .header("flutter_export.h")
        .header("flutter_glfw.h")
        .header("flutter_messenger.h")
        .header("flutter_plugin_registrar.h")
        .default_enum_style(EnumVariation::Rust)
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("flutter-engine-sys.rs"))
        .expect("Couldn't write bindings!");
}

fn main() {
    gen_bindings();

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("Cannot get project dir");
    let mut project_path = Path::new(&manifest_dir);

    // This project is in a workspace
    if let Some(p) = project_path.parent() {
        if p.join("Cargo.toml").is_file() {
            project_path = p;
        }
    }

    let version = get_flutter_version().expect("Cannot get flutter engine version");

    let libs_dir = project_path.join("target").join("flutter-engine");

    println!("Checking flutter engine status");
    if let Ok(rx) = flutter_download::download_to(&version, &libs_dir) {
        // THis is /bin/internal/engine.version file in your flutter sdk
        for (total, done) in rx.iter() {
            println!("Downloading flutter engine {} of {}", done, total);
        }
    }

    // config library search path
    let libs_dir = libs_dir.join(&version);

    #[cfg(target_os = "linux")]
    {
        println!(
            "cargo:rustc-link-search=native={}",
            libs_dir.to_str().expect("libs_dir invalid")
        );
    }

    #[cfg(target_os = "macos")]
    {
        println!(
            "cargo:rustc-link-search=framework={}",
            libs_dir.to_str().expect("libs_dir invalid")
        );
    }

    #[cfg(target_os = "windows")]
    {
        println!(
            "cargo:rustc-link-search=native={}",
            libs_dir.to_str().expect("libs_dir invalid")
        );
    }

    // use RUSTFLAGS to config linker
    write_cargo_config(project_path, &libs_dir);
}

fn write_cargo_config(project_dir: &Path, libs_dir: &Path) {
    println!("Generating .cargo/config file");

    let config_dir = project_dir.join(".cargo");
    std::fs::create_dir(&config_dir).unwrap_or(());

    let s = if cfg!(target_os = "linux") {
        format!(
            r#"[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-args=-Wl,-rpath,{libs}"]"#,
            libs = libs_dir.to_string_lossy()
        )
    } else if cfg!(target_os = "macos") {
        format!(
            r#"[target.x86_64-apple-darwin]
rustflags = ["-C", "link-args=-Wl,-rpath,{libs},-rpath,@executable_path/../Frameworks/"]"#,
            libs = libs_dir.to_string_lossy()
        )
    } else if cfg!(target_os = "windows") {
        // windows does not use rpath, we have to copy dll to OUT_DIR
        let src = libs_dir.join("flutter_windows.dll");
        let tar = Path::new(&std::env::var("OUT_DIR").unwrap())
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("flutter_windows.dll");

        let _ = fs::copy(src, tar);
        format!(r#""#)
    } else {
        format!(r#""#)
    };

    fs::write(config_dir.join("config"), s).expect("Cannot write linker config in .cargo/config");
}
