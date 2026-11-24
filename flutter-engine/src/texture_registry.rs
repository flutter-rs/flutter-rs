//! External OpenGL textures the embedder can push frames into.

use crate::{FlutterEngine, log_engine_result};
use flutter_engine_sys::FlutterOpenGLTexture;
#[cfg(feature = "gl-helpers")]
use image::RgbaImage;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

pub(crate) struct TextureRegistry {
    last_id: AtomicI64,
    frames: Arc<Mutex<HashMap<TextureId, TextureFrame>>>,
}

impl TextureRegistry {
    pub fn new() -> Self {
        Self {
            last_id: AtomicI64::new(1),
            frames: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn create_texture(&self, engine: FlutterEngine) -> Texture {
        let texture_id = self.last_id.fetch_add(1, Ordering::Relaxed);

        engine.run_on_platform_thread(move |engine| {
            log::trace!("texture {}: register", texture_id);
            if engine.is_shut_down() {
                return;
            }
            // SAFETY: the engine handle is live and this runs on the platform
            // thread, as `FlutterEngineRegisterExternalTexture` requires.
            let result = unsafe {
                flutter_engine_sys::FlutterEngineRegisterExternalTexture(
                    engine.engine_ptr(),
                    texture_id,
                )
            };
            log_engine_result(result, "FlutterEngineRegisterExternalTexture");
        });

        Texture {
            engine,
            texture_id,
            frames: self.frames.clone(),
        }
    }

    pub fn get_texture_frame(
        &self,
        texture_id: TextureId,
        _size: (usize, usize),
    ) -> Option<TextureFrame> {
        self.frames.lock().remove(&texture_id)
    }
}

/// The engine-wide identifier of an external texture.
pub type TextureId = i64;

/// A registered external texture. Unregisters itself when dropped.
pub struct Texture {
    engine: FlutterEngine,
    texture_id: TextureId,
    frames: Arc<Mutex<HashMap<TextureId, TextureFrame>>>,
}

impl core::fmt::Debug for Texture {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Texture")
            .field("texture_id", &self.texture_id)
            .finish_non_exhaustive()
    }
}

impl Texture {
    /// The id to hand to Dart's `Texture` widget.
    #[must_use]
    pub fn id(&self) -> TextureId {
        self.texture_id
    }

    /// Make `frame` the texture's current frame, replacing any frame the raster
    /// thread has not picked up yet.
    pub fn post_frame(&self, frame: TextureFrame) {
        post_frame_internal(&self.engine, self.texture_id, &self.frames, frame);
    }

    /// Upload `img` into a fresh GL texture and post it as the current frame.
    #[cfg(feature = "gl-helpers")]
    pub fn post_frame_rgba(&self, img: RgbaImage) {
        let texture_id = self.texture_id;
        let frames = self.frames.clone();
        self.engine.run_on_render_thread(move |engine| {
            let (width, height) = img.dimensions();

            // SAFETY: this runs on the render thread with the engine's GL
            // context current, `glid` is a live `u32` out-parameter and the
            // pixel pointer is valid for `width * height * 4` bytes, which is
            // what the `RGBA`/`UNSIGNED_BYTE` format pair describes.
            let glid = unsafe {
                let mut glid: u32 = 0;
                gl::GenTextures(1, &mut glid as *mut _);
                gl::BindTexture(gl::TEXTURE_2D, glid);
                gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,             // mipmap level
                    gl::RGBA as _, // internal format of the texture
                    width as _,
                    height as _,
                    0,                        // border, must be 0
                    gl::RGBA,                 // format of the pixel data
                    gl::UNSIGNED_BYTE,        // data type of the pixel data
                    img.as_ptr() as *const _, // pixel data
                );
                gl::BindTexture(gl::TEXTURE_2D, 0);

                glid
            };

            let engine_weak = engine.downgrade();
            let frame = TextureFrame::new(gl::TEXTURE_2D, glid, gl::RGBA8, move || {
                if let Some(engine) = engine_weak.upgrade() {
                    engine.run_on_render_thread(move |_| {
                        // SAFETY: runs on the render thread with the GL context
                        // current, and `glid` is the texture created above,
                        // deleted exactly once.
                        unsafe {
                            gl::DeleteTextures(1, &glid as *const _);
                        }
                    });
                }
            });

            post_frame_internal(engine, texture_id, &frames, frame);
        });
    }
}

fn post_frame_internal(
    engine: &FlutterEngine,
    texture_id: TextureId,
    frames: &Arc<Mutex<HashMap<TextureId, TextureFrame>>>,
    frame: TextureFrame,
) {
    if let Some(old_frame) = frames.lock().insert(texture_id, frame) {
        engine.run_on_render_thread(move |_| {
            (old_frame.destruction_callback)();
        });
    }

    engine.run_on_platform_thread(move |engine| {
        log::trace!("texture {}: marking frame available", texture_id);
        if engine.is_shut_down() {
            return;
        }
        // SAFETY: the engine handle is live and this runs on the platform
        // thread, as the embedder API requires.
        let result = unsafe {
            flutter_engine_sys::FlutterEngineMarkExternalTextureFrameAvailable(
                engine.engine_ptr(),
                texture_id,
            )
        };
        log_engine_result(result, "FlutterEngineMarkExternalTextureFrameAvailable");
    });
}

impl Drop for Texture {
    fn drop(&mut self) {
        let texture_id = self.texture_id;

        // A frame that was posted but never picked up by the raster thread owns
        // a GL texture; without this its destruction callback would never run
        // and the map entry would outlive the texture.
        if let Some(frame) = self.frames.lock().remove(&texture_id) {
            self.engine.run_on_render_thread(move |_| {
                (frame.destruction_callback)();
            });
        }

        self.engine.run_on_platform_thread(move |engine| {
            log::trace!("texture {}: unregister", texture_id);
            if engine.is_shut_down() {
                return;
            }
            // SAFETY: the engine handle is live and this runs on the platform
            // thread, as the embedder API requires.
            let result = unsafe {
                flutter_engine_sys::FlutterEngineUnregisterExternalTexture(
                    engine.engine_ptr(),
                    texture_id,
                )
            };
            log_engine_result(result, "FlutterEngineUnregisterExternalTexture");
        });
    }
}

type DestructorType = Box<dyn FnOnce() + Send>;

/// One frame of an external texture: a GL texture plus the callback that frees
/// it once the engine is done with it.
pub struct TextureFrame {
    /// GL texture target, e.g. `GL_TEXTURE_2D`.
    pub target: u32,
    /// GL texture name.
    pub name: u32,
    /// GL internal format, e.g. `GL_RGBA8`.
    pub format: u32,
    /// Called on the render thread once the engine has finished with the frame.
    pub destruction_callback: Box<DestructorType>,
}

impl core::fmt::Debug for TextureFrame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextureFrame")
            .field("target", &self.target)
            .field("name", &self.name)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl TextureFrame {
    /// Describe a frame the embedder has already uploaded to GL.
    ///
    /// `destruction_callback` runs on the engine's render thread, with its GL
    /// context current.
    #[must_use]
    pub fn new<F>(target: u32, name: u32, format: u32, destruction_callback: F) -> TextureFrame
    where
        F: FnOnce() + 'static + Send,
    {
        Self {
            target,
            name,
            format,
            destruction_callback: Box::new(Box::new(destruction_callback)),
        }
    }

    pub(crate) fn into_ffi(self, target: &mut FlutterOpenGLTexture) {
        target.target = self.target;
        target.name = self.name;
        target.format = self.format;
        target.destruction_callback = Some(texture_destruction_callback);
        target.user_data = Box::into_raw(self.destruction_callback) as _;
    }
}

/// # Safety
///
/// `user_data` must be the `Box<DestructorType>` pointer leaked by
/// [`TextureFrame::into_ffi`], delivered back by the engine exactly once.
unsafe extern "C" fn texture_destruction_callback(user_data: *mut c_void) {
    log::trace!("texture_destruction_callback");
    // SAFETY: the caller guarantees `user_data` is the leaked
    // `Box<DestructorType>` and that this runs once for it.
    let user_data = unsafe { Box::from_raw(user_data as *mut DestructorType) };
    // A panic must not reach the `extern "C"` boundary: it would abort the
    // process rather than unwind.
    if catch_unwind(AssertUnwindSafe(user_data)).is_err() {
        log::error!("panic in a texture destruction callback");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// `TextureFrame::new` must hand the callback through untouched, and the
    /// frame must not run it on drop (the engine owns that decision).
    #[test]
    fn a_frame_keeps_its_destruction_callback() {
        static RAN: AtomicBool = AtomicBool::new(false);
        RAN.store(false, Ordering::SeqCst);

        let frame = TextureFrame::new(0x0DE1, 7, 0x8058, || {
            RAN.store(true, Ordering::SeqCst);
        });
        assert_eq!(frame.target, 0x0DE1);
        assert_eq!(frame.name, 7);
        assert_eq!(frame.format, 0x8058);

        let callback = frame.destruction_callback;
        assert!(!RAN.load(Ordering::SeqCst));
        callback();
        assert!(RAN.load(Ordering::SeqCst));
    }

    /// Texture ids are handed out in order and never reused, because the engine
    /// keys its external textures on them.
    #[test]
    fn texture_ids_are_unique_and_increasing() {
        let registry = TextureRegistry::new();
        let first = registry.last_id.fetch_add(1, Ordering::Relaxed);
        let second = registry.last_id.fetch_add(1, Ordering::Relaxed);
        assert_eq!(first, 1);
        assert_eq!(second, 2);
    }

    /// A frame posted but never consumed must be reclaimable by id, which is
    /// what both the raster thread and `Drop for Texture` rely on.
    #[test]
    fn a_pending_frame_is_taken_exactly_once() {
        let registry = TextureRegistry::new();
        registry
            .frames
            .lock()
            .insert(42, TextureFrame::new(0, 0, 0, || {}));

        assert!(registry.get_texture_frame(42, (1, 1)).is_some());
        assert!(registry.get_texture_frame(42, (1, 1)).is_none());
    }
}
