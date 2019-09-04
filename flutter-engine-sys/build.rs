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

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("Cannot get manifest dir");
    let out_dir = std::env::var("OUT_DIR").expect("Cannot get out dir");
    let mut project_path = Path::new(&manifest_dir);

    let mut is_dev = false;
    if let Some(p) = project_path.parent() {
        // This project is in a workspace
        if p.join("Cargo.toml").is_file() {
            is_dev = true;
            project_path = p;
        }
    }

    if !is_dev {
        project_path = Path::new(&out_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap();
    }

    let version = get_flutter_version().expect("Cannot get flutter engine version");

    println!("Checking flutter engine status");
    let (libs_dir, rx) = flutter_download::download(&version);
    if let Ok(rx) = rx {
        // THis is /bin/internal/engine.version file in your flutter sdk
        println!("Engine will be downloaded to {:?}", libs_dir);
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
        let src = libs_dir.join("flutter_engine.dll");
        let tar = Path::new(&std::env::var("OUT_DIR").unwrap())
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("flutter_engine.dll");

        let _ = fs::copy(src, tar);
        format!(r#""#)
    } else {
        format!(r#""#)
    };

    fs::write(config_dir.join("config"), s).expect("Cannot write linker config in .cargo/config");
}
