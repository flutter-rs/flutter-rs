//! Locate a Flutter SDK and fetch the matching engine embedder library.
//!
//! [`Flutter`] discovers the SDK (`FLUTTER_ROOT`, or `flutter` on `PATH`) and
//! reports the engine revision and storage realm it is pinned to. [`Engine`]
//! maps a (revision, target, build) triple onto Flutter's artifact storage and
//! caches the download under [`dirs::cache_dir`]:
//!
//! ```no_run
//! # fn main() -> Result<(), flutter_tools::Error> {
//! use flutter_tools::{Build, Engine, Flutter};
//!
//! let flutter = Flutter::auto_detect()?;
//! let engine = Engine::new(
//!     flutter.engine_version()?,
//!     "x86_64-unknown-linux-gnu".to_owned(),
//!     Build::Debug,
//! )
//! .with_realm(flutter.engine_realm());
//!
//! engine.download()?;
//! println!("{}", engine.library_path()?.display());
//! # Ok(())
//! # }
//! ```
//!
//! Only the JIT (debug) embedder is published, so [`Engine::download`] rejects
//! [`Build::Release`] and [`Build::Profile`]; an AOT engine has to be built
//! from source with `scripts/build-engine.sh`.
#![warn(missing_docs)]

use curl::easy::Easy;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zip::ZipArchive;

/// Base URL of Flutter's artifact storage, overridable with
/// `FLUTTER_STORAGE_BASE_URL` exactly as the `flutter` tool allows.
const DEFAULT_STORAGE_BASE_URL: &str = "https://storage.googleapis.com";

/// Everything that can go wrong while locating an SDK or fetching an engine.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// No Flutter SDK could be located.
    #[error("couldn't find a flutter sdk; set FLUTTER_ROOT or put `flutter` on PATH")]
    FlutterNotFound,
    /// Flutter's storage has no engine artifact for this revision.
    #[error(
        "couldn't find the flutter engine '{version}' at {url}\n\
         the engine has to match your sdk exactly: run `flutter upgrade`, pin a \
         revision with FLUTTER_ENGINE_VERSION or `[package.metadata.flutter] \
         engine_version`, or build one from source with scripts/build-engine.sh"
    )]
    DownloadNotFound {
        /// Engine revision that was requested.
        version: String,
        /// URL that reported the artifact as missing.
        url: String,
    },
    /// `icudtl.dat` is not in the SDK's engine cache.
    #[error("couldn't find icudtl.dat in the sdk's engine cache; run `flutter precache`")]
    IcuDataNotFound,
    /// The Dart SDK bundled with Flutter could not be located.
    #[error("couldn't find the dart sdk; run `flutter precache`")]
    DartNotFound,
    /// Flutter publishes no embedder artifact for this target triple.
    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),
    /// Only the JIT engine is published, and an AOT build was requested.
    #[error(
        "only the jit (debug) embedder is published; build an aot engine from \
         source with `scripts/build-engine.sh --mode release`"
    )]
    AotEngineNotPublished,
    /// The user's cache directory could not be determined.
    #[error("cannot determine the user cache directory")]
    NoCacheDir,
    /// An I/O error, usually while writing to the engine cache.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// An executable could not be located on `PATH`.
    #[error("could not locate an executable: {0}")]
    Which(#[from] which::Error),
    /// The HTTP transfer failed.
    #[error("http transfer failed: {0}")]
    Curl(#[from] curl::Error),
    /// The downloaded engine archive could not be read.
    #[error("could not read the engine archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// The archive unpacked without producing the engine library, so the
    /// artifact's layout is not the one this crate knows.
    #[error("the engine archive from {url} contains no {library}")]
    ArchiveMissingLibrary {
        /// Library (or framework bundle) that was expected at the top of the
        /// extracted tree.
        library: String,
        /// URL the archive came from.
        url: String,
    },
}

/// A located Flutter SDK.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Flutter {
    root_path: PathBuf,
}

impl Flutter {
    /// Use the SDK rooted at `path`.
    ///
    /// Returns [`Error::FlutterNotFound`] if `path` does not exist.
    pub fn new_from_path(path: PathBuf) -> Result<Self, Error> {
        if !path.exists() {
            return Err(Error::FlutterNotFound);
        }
        Ok(Self { root_path: path })
    }

    /// Locate the SDK from `FLUTTER_ROOT`, falling back to resolving `flutter`
    /// on `PATH`.
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

    /// Root directory of the SDK.
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// The engine revision this SDK is pinned to.
    ///
    /// `bin/cache/engine.stamp` is the file the `flutter` tool itself reads and
    /// is written on every channel; `bin/internal/engine.version` only ships in
    /// tagged stable/beta releases (it is gitignored on `main`), so it is only
    /// a fallback for a freshly cloned SDK that has never been run.
    pub fn engine_version(&self) -> Result<String, Error> {
        let stamp = self
            .root_path
            .join("bin")
            .join("cache")
            .join("engine.stamp");
        if let Ok(version) = std::fs::read_to_string(&stamp) {
            let version = version.trim();
            if !version.is_empty() {
                return Ok(version.to_owned());
            }
        }

        let pinned = self
            .root_path
            .join("bin")
            .join("internal")
            .join("engine.version");
        Ok(std::fs::read_to_string(pinned)?.trim().to_owned())
    }

    /// The artifact storage realm this SDK is pinned to, or an empty string for
    /// an SDK using the default realm.
    ///
    /// SDKs built against presubmit artifacts record a realm in
    /// `bin/cache/engine.realm`, which has to be spliced into the artifact URL.
    /// Pass the result to [`Engine::with_realm`].
    pub fn engine_realm(&self) -> String {
        let path = self
            .root_path
            .join("bin")
            .join("cache")
            .join("engine.realm");
        std::fs::read_to_string(path)
            .map(|realm| realm.trim().to_owned())
            .unwrap_or_default()
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
            .ok_or(Error::IcuDataNotFound)
    }

    /// Locate the `dart` binary inside the SDK's Dart SDK cache.
    ///
    /// This is the only Dart binary a Flutter install ships; the published
    /// embedder archives contain none (see [`Engine::dart`]).
    pub fn dart(&self) -> Result<PathBuf, Error> {
        let bin = self
            .root_path
            .join("bin")
            .join("cache")
            .join("dart-sdk")
            .join("bin");
        ["dart", "dart.exe"]
            .iter()
            .map(|name| bin.join(name))
            .find(|path| path.exists())
            .ok_or(Error::DartNotFound)
    }
}

/// Build mode of an engine artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Build {
    /// JIT engine, the only one Flutter publishes for the generic embedder.
    Debug,
    /// AOT engine with optimisations; has to be built from source.
    Release,
    /// AOT engine with tracing enabled; has to be built from source.
    Profile,
}

impl Build {
    /// The directory name this build mode is cached under.
    pub fn build(&self) -> &str {
        match self {
            Self::Debug => "debug_unopt",
            Self::Release => "release",
            Self::Profile => "profile",
        }
    }
}

/// An engine artifact: an engine revision, a Rust target triple and a build
/// mode, plus the storage realm to fetch it from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Engine {
    version: String,
    target: String,
    build: Build,
    realm: String,
}

impl Engine {
    /// Describe the engine artifact for an engine revision (as reported by
    /// [`Flutter::engine_version`]), a Rust target triple and a build mode.
    pub fn new(version: String, target: String, build: Build) -> Self {
        Self {
            version,
            target,
            build,
            realm: String::new(),
        }
    }

    /// Fetch this artifact from a non-default storage realm, as reported by
    /// [`Flutter::engine_realm`]. An empty realm means the default.
    #[must_use]
    pub fn with_realm(mut self, realm: String) -> Self {
        self.realm = realm;
        self
    }

    /// The platform directory used by Flutter's official artifact storage,
    /// e.g. `linux-x64`.
    fn artifact_platform(&self) -> Result<&'static str, Error> {
        Ok(match self.target.as_str() {
            "x86_64-unknown-linux-gnu" => "linux-x64",
            "aarch64-unknown-linux-gnu" => "linux-arm64",
            // There is no darwin-arm64 embedder: the darwin-x64 framework is a
            // universal binary (verified: x86_64 + arm64 slices), so both Apple
            // targets take it.
            "x86_64-apple-darwin" | "aarch64-apple-darwin" => "darwin-x64",
            "x86_64-pc-windows-msvc" | "x86_64-pc-windows-gnu" => "windows-x64",
            "aarch64-pc-windows-msvc" => "windows-arm64",
            _ => return Err(Error::UnsupportedTarget(self.target.clone())),
        })
    }

    /// URL of the published artifact for this engine.
    pub fn download_url(&self) -> Result<String, Error> {
        let platform = self.artifact_platform()?;
        // Apple's embedder ships as a framework bundle; every other platform
        // gets a flat `<platform>-embedder.zip`.
        let archive = if platform.starts_with("darwin") {
            "FlutterEmbedder.framework.zip".to_owned()
        } else {
            format!("{platform}-embedder.zip")
        };
        let base = std::env::var("FLUTTER_STORAGE_BASE_URL")
            .ok()
            .filter(|base| !base.is_empty())
            .unwrap_or_else(|| DEFAULT_STORAGE_BASE_URL.to_owned());

        Ok(compose_download_url(
            &base,
            &self.realm,
            &self.version,
            platform,
            &archive,
        ))
    }

    /// Name of the library — or, on Apple targets, the framework bundle — that
    /// the artifact installs into [`Engine::engine_dir`].
    pub fn library_name(&self) -> Result<&'static str, Error> {
        Ok(match self.target.as_str() {
            "x86_64-unknown-linux-gnu" | "aarch64-unknown-linux-gnu" => "libflutter_engine.so",
            "x86_64-apple-darwin" | "aarch64-apple-darwin" => "FlutterEmbedder.framework",
            "x86_64-pc-windows-msvc" | "x86_64-pc-windows-gnu" | "aarch64-pc-windows-msvc" => {
                "flutter_engine.dll"
            }
            _ => return Err(Error::UnsupportedTarget(self.target.clone())),
        })
    }

    /// Directory this artifact is cached in, under [`dirs::cache_dir`].
    pub fn engine_dir(&self) -> Result<PathBuf, Error> {
        Ok(dirs::cache_dir()
            .ok_or(Error::NoCacheDir)?
            .join("flutter-engine")
            .join(&self.version)
            .join(&self.target)
            .join(self.build.build()))
    }

    /// Path the library (or framework bundle) ends up at once downloaded.
    ///
    /// Only meaningful once [`Engine::download`] has returned `Ok`: nothing is
    /// published under this path until the whole artifact is in place.
    pub fn library_path(&self) -> Result<PathBuf, Error> {
        Ok(self.engine_dir()?.join(self.library_name()?))
    }

    /// Download and extract the artifact into [`Engine::engine_dir`], unless it
    /// is already there.
    ///
    /// The artifact is assembled in a staging directory beside
    /// [`Engine::engine_dir`] and renamed into place only once the archive has
    /// unpacked and the library is where [`Engine::library_path`] promises it.
    /// An interrupted run therefore leaves nothing behind that the next one
    /// would mistake for a usable engine, and the archive itself is thrown away
    /// with the staging directory rather than installed next to the library.
    /// Two processes downloading the same artifact are safe: each stages
    /// separately, and whoever loses the rename keeps the winner's copy.
    ///
    /// # Errors
    ///
    /// - [`Error::AotEngineNotPublished`] for [`Build::Release`] and
    ///   [`Build::Profile`]: Flutter only publishes the JIT embedder, and
    ///   installing it under a `release` directory would hand the caller an
    ///   engine that cannot run an AOT snapshot.
    /// - [`Error::UnsupportedTarget`] if Flutter publishes no embedder for this
    ///   target, or [`Error::NoCacheDir`] if the cache directory is unknown.
    /// - [`Error::DownloadNotFound`] if storage holds no artifact for this
    ///   engine revision, and [`Error::Curl`] for any other transfer failure.
    /// - [`Error::ArchiveMissingLibrary`] if the archive unpacked without
    ///   producing [`Engine::library_name`], [`Error::Zip`] if it could not be
    ///   read, and [`Error::Io`] for any failure writing the engine cache.
    pub fn download(&self) -> Result<(), Error> {
        if self.build != Build::Debug {
            return Err(Error::AotEngineNotPublished);
        }

        let dir = self.engine_dir()?;
        let library = self.library_name()?;
        let installed = dir.join(library);

        if installed.exists() {
            return Ok(());
        }

        let url = self.download_url()?;
        let staging = Staging::create(&staging_path(&dir))?;
        // What `engine_dir` is going to be: everything the artifact installs
        // and nothing else, so that installing it is a single rename.
        let payload = staging.path().join("payload");

        // The darwin artifact is a *bare* framework: its entries are rooted at
        // `Versions/`, not at `FlutterEmbedder.framework/`, so it has to be
        // unpacked into the bundle directory itself. Everything else ships a
        // flat archive that belongs directly in `engine_dir`.
        let extract_dir = if library.ends_with(".framework") {
            payload.join(library)
        } else {
            payload.clone()
        };
        std::fs::create_dir_all(&extract_dir)?;

        // Outside `payload`, so it is discarded with the staging directory.
        let archive = staging.path().join("engine.zip");
        download(&url, &archive, &self.version)?;
        unzip(&archive, &extract_dir)?;

        if !payload.join(library).exists() {
            return Err(Error::ArchiveMissingLibrary {
                library: library.to_owned(),
                url,
            });
        }

        install(&payload, &dir, &installed)
    }

    /// Previously looked for a `dart` binary next to the engine library.
    ///
    /// The published embedder archives contain no Dart binary, so this can only
    /// ever return [`Error::DartNotFound`]. Use [`Flutter::dart`] instead.
    #[deprecated(note = "use Flutter::dart; the embedder archive ships no dart binary")]
    pub fn dart(&self) -> Result<PathBuf, Error> {
        let host_engine_dir = self.engine_dir()?;
        ["dart", "dart.exe"]
            .iter()
            .map(|bin| host_engine_dir.join(bin))
            .find(|path| path.exists())
            .ok_or(Error::DartNotFound)
    }
}

/// Join the parts of an artifact URL.
///
/// A non-empty realm goes between the storage base and the bucket path, which
/// is where the `flutter` tool puts it.
fn compose_download_url(
    base: &str,
    realm: &str,
    version: &str,
    platform: &str,
    archive: &str,
) -> String {
    let base = base.trim_end_matches('/');
    let realm = realm.trim_matches('/');
    let tail = format!("flutter_infra_release/flutter/{version}/{platform}/{archive}");

    if realm.is_empty() {
        format!("{base}/{tail}")
    } else {
        format!("{base}/{realm}/{tail}")
    }
}

/// Path of the staging directory an engine cache entry is assembled in.
///
/// A sibling of `dir`, so installing it is a rename inside one directory (and
/// therefore one filesystem), under a name no artifact and no concurrent
/// download can collide with: [`Build::build`] never starts with a dot.
fn staging_path(dir: &Path) -> PathBuf {
    /// Tells apart two staging directories made by one process.
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    // Relaxed: the counter only has to hand out distinct values, and nothing is
    // published through it.
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let leaf = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("engine");

    dir.with_file_name(format!(".{leaf}.staging-{}-{sequence}", std::process::id()))
}

/// A scratch directory that is removed again when it goes out of scope.
///
/// [`Engine::download`] assembles the cache entry in here, so every way out of
/// that function — including the `?` on a failed transfer and a panic — has to
/// take the half-written tree with it.
struct Staging {
    path: PathBuf,
}

impl Staging {
    /// Create `path`, discarding anything already there.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the directory cannot be created.
    fn create(path: &Path) -> Result<Self, Error> {
        // Only a crashed run of this very process id can have left a directory
        // here, and its contents are worthless either way.
        if path.exists() {
            std::fs::remove_dir_all(path)?;
        }
        std::fs::create_dir_all(path)?;

        Ok(Self {
            path: path.to_owned(),
        })
    }

    /// The directory being staged in.
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        // `NotFound` is not a failure: something else, such as another run's
        // cleanup, got there first. Anything else leaves a stale copy of a
        // 15 MB download behind, which is worth a word even though the caller
        // is usually already on its way out with an error of its own.
        if let Err(error) = std::fs::remove_dir_all(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("warning: could not remove {}: {error}", self.path.display());
        }
    }
}

/// Move the fully assembled `staged` directory onto `dest`.
///
/// `installed` is the file (or bundle) inside `dest` that marks the entry as
/// complete; see [`Engine::library_path`].
///
/// # Errors
///
/// Returns [`Error::Io`] if `dest` is occupied by something that is neither a
/// complete engine nor removable, or if the rename fails for any other reason.
fn install(staged: &Path, dest: &Path, installed: &Path) -> Result<(), Error> {
    let blocked = match std::fs::rename(staged, dest) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };

    // Something is in the way. Either another process installed the same
    // artifact while this one was downloading — its copy is as good as ours —
    // or an interrupted run left a partial directory, which `rename` will not
    // replace: it refuses a non-empty destination, and on Windows any existing
    // destination at all.
    if installed.exists() {
        return Ok(());
    }
    if !dest.exists() {
        return Err(Error::Io(blocked));
    }

    std::fs::remove_dir_all(dest)?;
    match std::fs::rename(staged, dest) {
        Ok(()) => Ok(()),
        // Lost the same race, one step later.
        Err(_) if installed.exists() => Ok(()),
        Err(error) => Err(Error::Io(error)),
    }
}

/// Download `url` into `target`, drawing a progress bar.
fn download(url: &str, target: &Path, version: &str) -> Result<(), Error> {
    println!("Starting download from {url}");

    // curl hands us whatever arrives, which on a slow link is a lot of small
    // chunks, so buffer. The explicit flush at the end is what makes that safe:
    // `BufWriter`'s drop-time flush discards its error.
    let mut writer = BufWriter::with_capacity(64 * 1024, File::create(target)?);
    let mut write_error: Option<std::io::Error> = None;
    let mut last_done = 0.0;

    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .expect("the download progress template is a constant")
        .progress_chars("#>-"));

    let mut easy = Easy::new();
    easy.fail_on_error(true)?;
    easy.url(url)?;
    easy.follow_location(true)?;
    easy.progress(true)?;

    // A borrowing transfer rather than `Easy::write_function`, whose closure
    // would have to own the writer and could then never be flushed.
    let result = {
        let mut transfer = easy.transfer();
        transfer.progress_function(|total, done, _, _| {
            if done > last_done {
                last_done = done;

                pb.set_length(total as u64);
                pb.set_position(done as u64);
            }
            true
        })?;
        transfer.write_function(|data| match writer.write_all(data) {
            Ok(()) => Ok(data.len()),
            Err(error) => {
                // The callback cannot report an `io::Error`; a short count is
                // the only way to abort, so carry the cause out by hand.
                write_error = Some(error);
                Ok(0)
            }
        })?;
        transfer.perform()
    };

    if let Some(error) = write_error {
        return Err(Error::Io(error));
    }
    if let Err(error) = result {
        // `fail_on_error` turns an HTTP error into a transfer error, so the
        // "wrong engine version" case has to be read back off the handle.
        if easy.response_code().unwrap_or(0) == 404 {
            return Err(Error::DownloadNotFound {
                version: version.to_owned(),
                url: url.to_owned(),
            });
        }
        return Err(Error::Curl(error));
    }
    writer.flush()?;

    pb.finish_with_message("Downloaded");

    println!("Download finished");
    Ok(())
}

/// Extract `archive` into `dir`, preserving symlinks and the executable bit.
fn unzip(archive: &Path, dir: &Path) -> Result<(), Error> {
    println!("Extracting {}...", archive.display());

    // The deflate decoder reads the file in small pieces, so the archive needs
    // to be buffered even though the entries are copied out in bulk.
    let file = BufReader::new(File::open(archive)?);
    let mut archive = ZipArchive::new(file)?;

    let pb = ProgressBar::new(archive.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {wide_msg}")
            .expect("the extraction progress template is a constant")
            .progress_chars("#>-"),
    );

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        // `enclosed_name` rejects absolute and `..`-escaping entry paths.
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let outpath = dir.join(name);

        pb.inc(1);

        if file.is_dir() {
            pb.set_message(format!("File {i} extracted to \"{}\"", outpath.display()));
            std::fs::create_dir_all(&outpath)?;
            continue;
        }

        if let Some(parent) = outpath.parent() {
            std::fs::create_dir_all(parent)?;
        }

        #[cfg(unix)]
        if file.is_symlink() {
            use std::io::Read as _;

            // A symlink entry stores its target as the entry's contents. The
            // macOS framework is built almost entirely out of them, so writing
            // them as regular files yields an unusable bundle.
            let mut target = String::new();
            file.read_to_string(&mut target)?;
            if !is_contained_link_target(&target) {
                pb.set_message(format!(
                    "Skipped escaping symlink \"{}\"",
                    outpath.display()
                ));
                continue;
            }
            pb.set_message(format!(
                "File {i} linked to \"{}\" -> {target}",
                outpath.display()
            ));
            // A previous extraction may have left a regular file here.
            let _ = std::fs::remove_file(&outpath);
            std::os::unix::fs::symlink(&target, &outpath)?;
            continue;
        }

        pb.set_message(format!(
            "File {i} extracted to \"{}\" ({} bytes)",
            outpath.display(),
            file.size()
        ));
        let mut outfile = BufWriter::new(File::create(&outpath)?);
        std::io::copy(&mut file, &mut outfile)?;
        outfile.flush()?;

        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;

            if let Some(mode) = file.unix_mode() {
                // Mask off setuid/setgid/sticky (RUSTSEC-2026-0367): a downloaded
                // archive must not be able to create setuid-root files.
                std::fs::set_permissions(&outpath, Permissions::from_mode(mode & 0o777))?;
            }
        }
    }

    pb.finish_with_message("Extracted");

    Ok(())
}

/// Whether a symlink target stays inside the directory being extracted.
///
/// `ZipFile::enclosed_name` vets an entry's own path but says nothing about
/// where a symlink entry points, so without this an archive could drop a link
/// to `/etc/shadow` into the engine cache.
#[cfg(unix)]
fn is_contained_link_target(target: &str) -> bool {
    use std::path::Component;

    let path = Path::new(target);
    !target.is_empty()
        && !path.is_absolute()
        && !path
            .components()
            .any(|component| component == Component::ParentDir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(target: &str) -> Engine {
        Engine::new("deadbeef".to_owned(), target.to_owned(), Build::Debug)
    }

    /// A unique directory under the system temp directory, removed on drop so a
    /// failing assertion cannot leak it into the next run.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);

            let path = std::env::temp_dir().join(format!(
                "flutter-tools-{name}-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("the temp directory is writable");

            Self { path }
        }

        fn join(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn published_targets_map_to_an_artifact_and_a_library() {
        for (target, platform, library) in [
            (
                "x86_64-unknown-linux-gnu",
                "linux-x64",
                "libflutter_engine.so",
            ),
            (
                "aarch64-unknown-linux-gnu",
                "linux-arm64",
                "libflutter_engine.so",
            ),
            (
                "x86_64-apple-darwin",
                "darwin-x64",
                "FlutterEmbedder.framework",
            ),
            (
                "aarch64-apple-darwin",
                "darwin-x64",
                "FlutterEmbedder.framework",
            ),
            (
                "x86_64-pc-windows-msvc",
                "windows-x64",
                "flutter_engine.dll",
            ),
            ("x86_64-pc-windows-gnu", "windows-x64", "flutter_engine.dll"),
            (
                "aarch64-pc-windows-msvc",
                "windows-arm64",
                "flutter_engine.dll",
            ),
        ] {
            let engine = engine(target);
            assert_eq!(engine.artifact_platform().unwrap(), platform, "{target}");
            assert_eq!(engine.library_name().unwrap(), library, "{target}");
        }
    }

    #[test]
    fn unsupported_target_is_an_error_not_a_panic() {
        let engine = engine("mips64-unknown-linux-gnuabi64");
        assert!(matches!(
            engine.artifact_platform(),
            Err(Error::UnsupportedTarget(target)) if target == engine.target
        ));
        assert!(matches!(
            engine.library_name(),
            Err(Error::UnsupportedTarget(_))
        ));
        assert!(matches!(
            engine.download_url(),
            Err(Error::UnsupportedTarget(_))
        ));
    }

    #[test]
    fn download_url_matches_flutter_storage_layout() {
        assert_eq!(
            compose_download_url(
                DEFAULT_STORAGE_BASE_URL,
                "",
                "abc123",
                "linux-x64",
                "linux-x64-embedder.zip"
            ),
            "https://storage.googleapis.com/flutter_infra_release/flutter/abc123/linux-x64/linux-x64-embedder.zip"
        );
    }

    #[test]
    fn download_url_splices_the_engine_realm() {
        // Stray slashes on either side must not turn into an empty path
        // segment, which the storage bucket answers with a 404.
        assert_eq!(
            compose_download_url(
                "https://storage.googleapis.com/",
                "/flutter_archives_v2/",
                "abc123",
                "darwin-x64",
                "FlutterEmbedder.framework.zip"
            ),
            "https://storage.googleapis.com/flutter_archives_v2/flutter_infra_release/flutter/abc123/darwin-x64/FlutterEmbedder.framework.zip"
        );
    }

    #[test]
    fn apple_targets_take_the_universal_framework_archive() {
        let url = engine("aarch64-apple-darwin").download_url().unwrap();
        assert!(
            url.ends_with("/darwin-x64/FlutterEmbedder.framework.zip"),
            "{url}"
        );
    }

    #[test]
    fn download_not_found_names_the_requested_version() {
        let message = Error::DownloadNotFound {
            version: "abc123".to_owned(),
            url: "https://example.invalid/x.zip".to_owned(),
        }
        .to_string();

        assert!(message.contains("abc123"), "{message}");
        assert!(
            message.contains("https://example.invalid/x.zip"),
            "{message}"
        );
        // The old message formatted the literal "missing" into the version
        // placeholder and pointed at the dead engine-builds repository.
        assert!(!message.contains("missing"), "{message}");
        assert!(!message.contains("engine-builds"), "{message}");
    }

    #[test]
    fn wrapped_errors_keep_their_source() {
        use std::error::Error as _;

        let error = Error::from(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ));

        assert!(error.source().is_some(), "wrapped error lost its source");
        assert!(error.to_string().contains("denied"), "{error}");
    }

    #[test]
    fn aot_builds_are_refused_before_any_io() {
        for build in [Build::Release, Build::Profile] {
            let engine = Engine::new(
                "abc123".to_owned(),
                "x86_64-unknown-linux-gnu".to_owned(),
                build,
            );
            assert!(matches!(
                engine.download(),
                Err(Error::AotEngineNotPublished)
            ));
        }
    }

    #[test]
    fn staging_path_is_a_distinct_sibling_of_the_engine_dir() {
        let dir = PathBuf::from("cache")
            .join("flutter-engine")
            .join("abc123")
            .join("x86_64-unknown-linux-gnu")
            .join("debug_unopt");

        let staged = staging_path(&dir);

        // Installing is a rename, so the staging directory has to share the
        // engine directory's parent.
        assert_eq!(staged.parent(), dir.parent());
        assert_ne!(staged, dir);
        let name = staged
            .file_name()
            .and_then(|name| name.to_str())
            .expect("the staging path is built from a utf-8 format string");
        assert!(name.starts_with(".debug_unopt.staging-"), "{name}");
        // Two downloads in one process must not share — and so delete — a
        // staging directory.
        assert_ne!(staging_path(&dir), staged);
    }

    #[test]
    fn staging_removes_itself_and_whatever_it_held() {
        let scratch = Scratch::new("staging-drop");
        let path = scratch.join("work");

        {
            let staging = Staging::create(&path).unwrap();
            std::fs::write(staging.path().join("engine.zip"), "half a download").unwrap();
            assert!(path.is_dir());
        }

        assert!(!path.exists(), "{} outlived its guard", path.display());
    }

    #[test]
    fn staging_discards_what_an_interrupted_run_left() {
        let scratch = Scratch::new("staging-stale");
        let path = scratch.join("work");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("engine.zip"), "stale").unwrap();

        let staging = Staging::create(&path).unwrap();

        assert!(!staging.path().join("engine.zip").exists());
    }

    #[test]
    fn install_into_a_free_path_moves_the_payload() {
        let scratch = Scratch::new("install-fresh");
        let staged = scratch.join("payload");
        let dest = scratch.join("debug_unopt");
        let installed = dest.join("libflutter_engine.so");

        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("libflutter_engine.so"), "engine").unwrap();

        install(&staged, &dest, &installed).unwrap();

        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "engine");
        assert!(!staged.exists(), "the payload was copied instead of moved");
    }

    #[test]
    fn install_replaces_an_incomplete_cache_entry() {
        let scratch = Scratch::new("install-partial");
        let staged = scratch.join("payload");
        let dest = scratch.join("debug_unopt");
        let installed = dest.join("libflutter_engine.so");

        // What an interrupted run leaves behind: an engine directory with no
        // library in it, which `rename` on its own refuses to replace.
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("engine.zip"), "interrupted").unwrap();
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("libflutter_engine.so"), "engine").unwrap();

        install(&staged, &dest, &installed).unwrap();

        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "engine");
        assert!(
            !dest.join("engine.zip").exists(),
            "the interrupted run's archive survived into the engine directory"
        );
    }

    #[test]
    fn install_keeps_the_copy_a_concurrent_download_installed() {
        let scratch = Scratch::new("install-race");
        let staged = scratch.join("payload");
        let dest = scratch.join("debug_unopt");
        let installed = dest.join("libflutter_engine.so");

        // Another process finished first, so `dest` is already complete and may
        // be in use.
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(&installed, "theirs").unwrap();
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("libflutter_engine.so"), "ours").unwrap();

        install(&staged, &dest, &installed).unwrap();

        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "theirs");
    }

    #[test]
    fn archive_missing_library_names_the_library_and_the_url() {
        let message = Error::ArchiveMissingLibrary {
            library: "libflutter_engine.so".to_owned(),
            url: "https://example.invalid/linux-x64-embedder.zip".to_owned(),
        }
        .to_string();

        assert!(message.contains("libflutter_engine.so"), "{message}");
        assert!(message.contains("https://example.invalid"), "{message}");
    }

    #[cfg(unix)]
    #[test]
    fn escaping_symlink_targets_are_rejected() {
        // Targets taken from the real FlutterEmbedder.framework archive.
        assert!(is_contained_link_target("A"));
        assert!(is_contained_link_target("Versions/Current/Resources"));

        assert!(!is_contained_link_target(""));
        assert!(!is_contained_link_target("/etc/shadow"));
        assert!(!is_contained_link_target("../../../etc/shadow"));
        assert!(!is_contained_link_target("Versions/../../etc/shadow"));
    }
}
