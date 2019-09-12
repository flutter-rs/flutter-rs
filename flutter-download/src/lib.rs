extern crate curl;
extern crate dirs;
extern crate unzip;

use curl::easy::Easy;
use std::{
    fs::{self, File},
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{mpsc, Mutex},
    thread,
};

pub mod util;

pub use util::{get_flutter_version, Error};

#[derive(PartialEq, Copy, Clone)]
enum Target {
    Linux,
    Windows,
    MacOS,
}

pub fn download(version: &str) -> (PathBuf, Result<mpsc::Receiver<(f64, f64)>, Error>) {
    let libs_dir = dirs::cache_dir()
        .expect("Cannot get cache dir")
        .join("flutter-engine");

    let url = download_url(version);
    let dir = libs_dir.to_path_buf().join(version);

    if !should_download(&dir) {
        println!("Flutter engine already exist. Download not necessary");
        return (libs_dir, Err(Error::AlreadyDownloaded));
    }

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        // TODO: less unwrap, more error handling

        // Write the contents of rust-lang.org to stdout
        tx.send((0.0, 0.0)).unwrap();
        // create target dir

        fs::create_dir_all(&dir).unwrap();

        let download_file = dir.join("engine.zip");

        let mut file = File::create(&download_file).unwrap();

        let tx = Mutex::new(tx);

        let mut easy = Easy::new();

        println!("Starting download from {}", url);
        easy.url(&url).unwrap();
        easy.follow_location(true).unwrap();
        easy.progress(true).unwrap();
        easy.progress_function(move |total, done, _, _| {
            tx.lock().unwrap().send((total, done)).unwrap();
            true
        })
        .unwrap();
        easy.write_function(move |data| Ok(file.write(data).unwrap()))
            .unwrap();
        easy.perform().unwrap();

        println!("Download finished");

        println!("Extracting...");
        let zip_file = File::open(&download_file).unwrap();
        let reader = BufReader::new(zip_file);
        let unzipper = unzip::Unzipper::new(reader, &dir);
        unzipper.unzip().unwrap();

        // mac framework file is a double zip file
        if target() == Target::MacOS {
            Command::new("unzip")
                .args(&[
                    "FlutterEmbedder.framework.zip",
                    "-d",
                    "FlutterEmbedder.framework",
                ])
                .current_dir(&dir)
                .status()
                .unwrap();

            // TODO: fixme
            // unzip bug! Extracted file corrupted!
            // let zip_file = File::open(dir.join("FlutterEmbedder.framework.zip")).unwrap();
            // let reader = BufReader::new(zip_file);
            // let unzipper = unzip::Unzipper::new(reader, dir.join("FlutterEmbedder.framework"));
            // unzipper.unzip().unwrap();
        }
    });

    (libs_dir, Ok(rx))
}

pub fn download_url(version: &str) -> String {
    let url = match target() {
        Target::Linux => "{base_url}/flutter_infra/flutter/{version}/linux-x64/linux-x64-embedder",
        Target::MacOS => {
            "{base_url}/flutter_infra/flutter/{version}/darwin-x64/FlutterEmbedder.framework.zip"
        }
        Target::Windows => {
            "{base_url}/flutter_infra/flutter/{version}/windows-x64/windows-x64-embedder.zip"
        }
    };
    let base_url = std::env::var("FLUTTER_STORAGE_BASE_URL");
    let base_url = base_url
        .as_ref()
        .map(String::as_str)
        .unwrap_or("https://storage.googleapis.com");
    url.replace("{base_url}", base_url)
        .replace("{version}", version)
}

fn should_download(path: &Path) -> bool {
    match target() {
        Target::Linux => !path.join("libflutter_engine.so").exists(),
        Target::MacOS => !path.join("FlutterEmbedder.framework").exists(),
        Target::Windows => !path.join("flutter_engine.dll").exists(),
    }
}

fn target() -> Target {
    let target = std::env::var("TARGET").expect("Cannot determine target");
    if target.contains("linux") {
        Target::Linux
    } else if target.contains("apple") {
        Target::MacOS
    } else if target.contains("windows") {
        Target::Windows
    } else {
        panic!("Unknown target {}", target)
    }
}
