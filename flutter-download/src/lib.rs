extern crate curl;
extern crate dirs;
extern crate unzip;

use std::process::Command;
use std::sync::mpsc;
use std::io::{Write};
use curl::easy::Easy;
use std::{
    thread,
    io::BufReader,
    fs::{ self, File }
};
use std::sync::Mutex;
use std::path::{ Path, PathBuf };
use std::error;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    AlreadyDownloaded,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use std::error::Error;

        f.write_str(self.description())
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::AlreadyDownloaded => "AlreadyDownloaded",
            _ => "",
        }
    }
}

pub fn download(version: &str) -> Result<mpsc::Receiver<(f64, f64)>, Error> {
    let dir = home_download_path();
    download_to(version, &dir)
}

pub fn download_to(version: &str, dir: &Path) -> Result<mpsc::Receiver<(f64, f64)>, Error> {
    let url = download_url(version);
    let dir = dir.to_path_buf().join(version);

    if !should_download(&dir) {
        println!("Flutter engine already exist. Download not necessary");
        return Err(Error::AlreadyDownloaded);
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
        easy.progress(true).unwrap();
        easy.progress_function(move |total, done, _, _| {
            tx.lock().unwrap().send((total, done)).unwrap();
            true
        }).unwrap();
        easy.write_function(move |data| {
            Ok(file.write(data).unwrap())
        }).unwrap();
        easy.perform().unwrap();

        println!("Download finished");

        println!("Extracting...");
        let zip_file = File::open(&download_file).unwrap();
        let reader = BufReader::new(zip_file);
        let unzipper = unzip::Unzipper::new(reader, &dir);
        unzipper.unzip().unwrap();

        // mac framework file is a double zip file
        if cfg!(target_os = "macos") {
            Command::new("unzip").args(&["FlutterEmbedder.framework.zip", "-d", "FlutterEmbedder.framework"]).current_dir(&dir).status().unwrap();

            // TODO: fixme
            // unzip bug! Extracted file corrupted!
            // let zip_file = File::open(dir.join("FlutterEmbedder.framework.zip")).unwrap();
            // let reader = BufReader::new(zip_file);
            // let unzipper = unzip::Unzipper::new(reader, dir.join("FlutterEmbedder.framework"));
            // unzipper.unzip().unwrap();
        }
    });

    Ok(rx)
}

pub fn home_download_path() -> PathBuf {
    let mut dir = dirs::home_dir().unwrap();    
    dir.push(".flutter-rs");
    dir
}

#[cfg(target_os = "macos")]
pub fn download_url(version: &str) -> String {
    format!("https://storage.googleapis.com/flutter_infra/flutter/{}/darwin-x64/FlutterEmbedder.framework.zip", version)
}

#[cfg(target_os = "macos")]
fn should_download(path: &Path) -> bool {
    !path.join("FlutterEmbedder.framework").exists()
}

#[cfg(target_os = "linux")]
pub fn download_url(version: &str) -> String {
    format!("https://storage.googleapis.com/flutter_infra/flutter/{}/linux-x64/linux-x64-embedder", version)
}

#[cfg(target_os = "linux")]
fn should_download(path: &Path) -> bool {
    !path.join("libflutter_engine.so").exists()
}

#[cfg(target_os = "windows")]
pub fn download_url(version: &str) -> String {
    format!("https://storage.googleapis.com/flutter_infra/flutter/{}/windows-x64/windows-x64-embedder.zip", version)
}

#[cfg(target_os = "windows")]
fn should_download(path: &Path) -> bool {
    !path.join("flutter_engine.dll").exists()
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
