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
    InvalidFlutterRoot(&'static str),
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
            Error::InvalidFlutterRoot(_) => "Cannot read from FLUTTER_ROOT",
        }
    }
}

fn guess_sdk_path() -> Result<PathBuf, &'static str> {
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
                .args(&["/C", "where.exe flutter"])
                .output()
    } else {
        Command::new("sh")
                .arg("-c")
                .arg("which flutter")
                .output()
    }.map_err(|_| "cannot find flutter executable")?;
    let s = std::str::from_utf8(output.stdout.as_slice()).map_err(|_| "parse result of `which flutter`")?;
    let line = s.trim().lines().next().ok_or("output empty")?;
    let p = Path::new(line).canonicalize().map_err(|_| "follow link")?;
    let p = p.parent().ok_or("parent of flutter")?.parent().ok_or("parent of parent of flutter")?;
    Ok(p.to_owned())
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
        read_ver_from_sdk(p).map_err(|_| Error::InvalidFlutterRoot("read engine version from FLUTTER_ROOT failed"))
    } else {
        match guess_sdk_path() {
            Ok(p) => read_ver_from_sdk(&p).map_err(|_| Error::InvalidFlutterRoot("read engine version from flutter executable failed")),
            Err("cannot find flutter executable") => Err(Error::MissingEnv),
            Err(reason) => Err(Error::InvalidFlutterRoot(reason)),
        }
    }
}
