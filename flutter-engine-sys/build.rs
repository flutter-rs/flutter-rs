use bindgen::EnumVariation;
use std::path::PathBuf;

fn main() {
    let bindings = bindgen::Builder::default()
        .header("flutter-engine.h")
        .default_enum_style(EnumVariation::Rust {
            non_exhaustive: false,
        })
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("flutter-engine-sys.rs"))
        .expect("Couldn't write bindings!");
}
