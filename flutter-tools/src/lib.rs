use curl::easy::Easy;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zip::result::ZipError;
use zip::ZipArchive;

#[derive(Debug)]
pub enum Error {
    FlutterNotFound,
    DownloadNotFound,
    DartNotFound,
    Io(std::io::Error),
    Which(which::Error),
    Curl(curl::Error),
    Zip(zip::result::ZipError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::FlutterNotFound => write!(f, "Couldn't find flutter sdk"),
            Error::DownloadNotFound => write!(
                f,
                r#"We couldn't find the requested engine version '{}'.
This means that your flutter version is too old or to new.

To update flutter run `flutter upgrade`. If the problem persists the engine
build has not completed yet. This means you need to manually supply the flutter
engine version through one of the following methods:

```bash
export FLUTTER_ENGINE_VERSION = "..."
```

`Cargo.toml`
```toml
[package.metadata.flutter]
engine_version = "..."
```

You'll find the available builds on our github releases page [0].

- [0] https://github.com/flutter-rs/engine-builds/releases"#,
                "missing",
            ),
            Error::DartNotFound => write!(f, "Could't find dart"),
            Error::Which(error) => error.fmt(f),
            Error::Io(error) => error.fmt(f),
            Error::Curl(error) => error.fmt(f),
            Error::Zip(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error)
    }
}

impl From<which::Error> for Error {
    fn from(error: which::Error) -> Self {
        Error::Which(error)
    }
}

impl From<curl::Error> for Error {
    fn from(error: curl::Error) -> Self {
        Error::Curl(error)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(error: ZipError) -> Self {
        Error::Zip(error)
    }
}

pub struct Flutter {
    root_path: PathBuf,
}

impl Flutter {
    pub fn new_from_path(path: PathBuf) -> Result<Self, Error> {
        if !path.exists() {
            return Err(Error::FlutterNotFound);
        }
        Ok(Self { root_path: path })
    }

    pub fn auto_detect() -> Result<Self, Error> {
        let root = if let Ok(root) = std::env::var("FLUTTER_ROOT") {
            PathBuf::from(root)
        } else {
            let flutter = which::which("flutter").or(Err(Error::FlutterNotFound))?;
            let flutter = std::fs::canonicalize(flutter)?;
            flutter
                .parent()
                .ok_or(Error::FlutterNotFound)?
                .parent()
                .ok_or(Error::FlutterNotFound)?
                .to_owned()
        };
        Self::new_from_path(root)
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn engine_version(&self) -> Result<String, Error> {
        let path = self
            .root_path
            .join("bin")
            .join("internal")
            .join("engine.version");
        Ok(std::fs::read_to_string(path).map(|v| v.trim().to_owned())?)
    }

    /// Locate `icudtl.dat` inside the SDK's host engine cache.
    ///
    /// Official engine builds no longer link the ICU data into
    /// `libflutter_engine`, so the embedder must point `icu_data_path` at this
    /// file (see `FlutterEngineBuilder::with_icu_data_path`).
    pub fn icudtl_path(&self) -> Result<PathBuf, Error> {
        let engine = self
            .root_path
            .join("bin")
            .join("cache")
            .join("artifacts")
            .join("engine");
        std::fs::read_dir(&engine)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path().join("icudtl.dat"))
            .find(|path| path.exists())
            .ok_or(Error::DownloadNotFound)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Build {
    Debug,
    Release,
    Profile,
}

impl Build {
    pub fn build(&self) -> &str {
        match self {
            Self::Debug => "debug_unopt",
            Self::Release => "release",
            Self::Profile => "profile",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Engine {
    version: String,
    target: String,
    build: Build,
}

impl Engine {
    pub fn new(version: String, target: String, build: Build) -> Self {
        Self {
            version,
            target,
            build,
        }
    }

    /// The platform directory used by Flutter's official artifact storage,
    /// e.g. `linux-x64`. Desktop embedder builds are published per engine
    /// version under these directories.
    fn artifact_platform(&self) -> &'static str {
        match self.target.as_str() {
            "x86_64-unknown-linux-gnu" => "linux-x64",
            "aarch64-unknown-linux-gnu" => "linux-arm64",
            "x86_64-apple-darwin" | "aarch64-apple-darwin" => "darwin-x64",
            "x86_64-pc-windows-msvc" | "x86_64-pc-windows-gnu" => "windows-x64",
            _ => panic!("unsupported platform: {}", self.target),
        }
    }

    pub fn download_url(&self) -> String {
        // The flutter-rs/engine-builds repository is no longer maintained, so we
        // download the embedder library straight from Flutter's official engine
        // artifact storage. The same library is used for every build mode; the
        // debug/release distinction lives in the app's Dart snapshot, not here.
        let platform = self.artifact_platform();
        let archive = if platform == "darwin-x64" {
            "FlutterEmbedder.framework.zip".to_owned()
        } else {
            format!("{}-embedder.zip", platform)
        };
        format!(
            "https://storage.googleapis.com/flutter_infra_release/flutter/{version}/{platform}/{archive}",
            version = self.version,
        )
    }

    pub fn library_name(&self) -> &'static str {
        match self.target.as_str() {
            "x86_64-unknown-linux-gnu" | "aarch64-unknown-linux-gnu" => "libflutter_engine.so",
            "x86_64-apple-darwin" | "aarch64-apple-darwin" => "FlutterEmbedder.framework",
            "x86_64-pc-windows-msvc" | "x86_64-pc-windows-gnu" => "flutter_engine.dll",
            _ => panic!("unsupported platform: {}", self.target),
        }
    }

    pub fn engine_dir(&self) -> PathBuf {
        dirs::cache_dir()
            .expect("Cannot get cache dir")
            .join("flutter-engine")
            .join(&self.version)
            .join(&self.target)
            .join(self.build.build())
    }

    pub fn library_path(&self) -> PathBuf {
        self.engine_dir().join(self.library_name())
    }

    pub fn download(&self) -> Result<(), Error> {
        let url = self.download_url();
        let path = self.library_path();
        let dir = path.parent().unwrap().to_owned();

        if path.exists() {
            return Ok(());
        }

        std::fs::create_dir_all(&dir)?;

        let download_file = dir.join("engine.zip");
        download(&url, &download_file)?;
        unzip(&download_file, &dir)?;

        Ok(())
    }

    pub fn dart(&self) -> Result<PathBuf, Error> {
        let host_engine_dir = self.engine_dir();
        ["dart", "dart.exe"]
            .iter()
            .map(|bin| host_engine_dir.join(bin))
            .find(|path| path.exists())
            .ok_or(Error::DartNotFound)
    }
}

fn download(url: &str, target: &Path) -> Result<(), Error> {
    println!("Starting download from {}", url);
    let mut file = File::create(target)?;
    let mut last_done = 0.0;

    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    let pb = Arc::new(pb);

    let mut easy = Easy::new();
    easy.fail_on_error(true)?;
    easy.url(&url)?;
    easy.follow_location(true)?;
    easy.progress(true)?;
    let pb2 = pb.clone();
    easy.progress_function(move |total, done, _, _| {
        if done > last_done {
            last_done = done;

            pb2.set_length(total as u64);
            pb2.set_position(done as u64);
        }
        true
    })?;
    easy.write_function(move |data| Ok(file.write(data).unwrap()))?;

    easy.perform().or_else(|_| Err(Error::DownloadNotFound))?;

    pb.finish_with_message("Downloaded");

    println!("Download finished");
    Ok(())
}

fn unzip(archive: &Path, dir: &Path) -> Result<(), Error> {
    println!("Extracting {:?}...", archive.file_name().unwrap());

    let file = File::open(archive)?;
    let mut archive = ZipArchive::new(file)?;

    let pb = ProgressBar::new(archive.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {wide_msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => dir.join(path),
            None => continue,
        };

        pb.inc(1);

        if file.name().ends_with('/') {
            pb.set_message(format!("File {} extracted to \"{}\"", i, outpath.display()));
            std::fs::create_dir_all(&outpath)?;
        } else {
            pb.set_message(format!(
                "File {} extracted to \"{}\" ({} bytes)",
                i,
                outpath.display(),
                file.size()
            ));
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    std::fs::create_dir_all(&p)?;
                }
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile).unwrap();

            #[cfg(unix)]
            {
                use std::fs::Permissions;
                use std::os::unix::fs::PermissionsExt;

                if let Some(mode) = file.unix_mode() {
                    std::fs::set_permissions(&outpath, Permissions::from_mode(mode))?;
                }
            }
        }
    }

    pb.finish_with_message("Extracted");

    Ok(())
}
