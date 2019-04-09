use std::{
    error,
    fmt,
    fs,
    path::{ Path },
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
            Error::MissingEnv => "Cannot get flutter engine version. Either FLUTTER_ROOT or FLUTTER_ENGINE_VERSION has to be set",
            Error::InvalidFlutterRoot => "Cannot read from FLUTTER_ROOT",
        }
    }
}

pub fn get_flutter_version() -> Result<String, Error> {
    if let Ok(v) = std::env::var("FLUTTER_ENGINE_VERSION") {
        Ok(v)
    } else if let Ok(v) = std::env::var("FLUTTER_ROOT") {
        let p = Path::new(&v);
        let p = p.join("bin").join("internal").join("engine.version");
        if let Ok(v) = fs::read_to_string(p) {
            Ok(String::from(v.trim()))
        } else {
            Err(Error::InvalidFlutterRoot)
        }
    } else {
        Err(Error::MissingEnv)
    }
}