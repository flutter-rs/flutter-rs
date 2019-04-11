use std::{
    error,
    fmt,
    io,
    fs,
    process::Command,
    path::{ Path, PathBuf },
};

#[derive(Debug)]
pub enum Error {
    AlreadyDownloaded,
    MissingEnv,
    InvalidFlutterRoot,
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
            Error::AlreadyDownloaded => "Flutter engine already downloaded",
            Error::MissingEnv => "Cannot find flutter engine version. flutter cli not in PATH. You may need to set either FLUTTER_ROOT or FLUTTER_ENGINE_VERSION",
            Error::InvalidFlutterRoot => "Cannot read from FLUTTER_ROOT",
        }
    }
}

fn guess_sdk_path() -> Option<PathBuf> {
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
                .args(&["/C", "where.exe flutter"])
                .output()
    } else {
        Command::new("sh")
                .arg("-c")
                .arg("which flutter")
                .output()
    };
    if let Ok(o) = output {
        if let Ok(s) = std::str::from_utf8(o.stdout.as_slice()) {
            if let Some(line) = s.trim().lines().next() {
                let p = Path::new(line);
                if let Some(p) = p.parent() {
                    if let Some(p) = p.parent() {
                        return Some(p.to_owned());
                    }
                }
            }
        }
    }
    None
}

fn read_ver_from_sdk(p: &Path) -> io::Result<String> {
    let p = p.join("bin").join("internal").join("engine.version");
    fs::read_to_string(p).map(|v| v.trim().to_owned())
}

pub fn get_flutter_version() -> Result<String, Error> {
    if let Ok(v) = std::env::var("FLUTTER_ENGINE_VERSION") {
        Ok(v)
    } else if let Ok(v) = std::env::var("FLUTTER_ROOT") {
        let p = Path::new(&v);
        read_ver_from_sdk(p).map_err(|_| Error::InvalidFlutterRoot)
    } else if let Some(p) = guess_sdk_path() {
        read_ver_from_sdk(&p).map_err(|_| Error::InvalidFlutterRoot)
    } else {
        Err(Error::MissingEnv)
    }
}