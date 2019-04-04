extern crate bindgen;

use std::env;
use std::path::PathBuf;
use bindgen::EnumVariation;

fn main() {
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
