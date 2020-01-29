use crate::ffi::{ExternalTexture, ExternalTextureFrame, TextureId};
use crate::FlutterEngine;
#[cfg(feature = "image")]
use image::RgbaImage;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::{Arc, Barrier};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Default)]
pub struct TextureRegistry {
    textures: Arc<RwLock<HashMap<TextureId, u32>>>,
}

impl TextureRegistry {
    pub fn register(&self, texture_id: TextureId, glid: u32) {
        self.textures.write().insert(texture_id, glid);
    }

    pub fn get_texture_frame(&self, texture_id: TextureId, _size: (usize, usize)) -> Option<ExternalTextureFrame> {
        let textures = self.textures.read();
        if let Some(glid) = textures.get(&texture_id) {
            log::trace!("returning external texture frame with glid {}", glid);
            return Some(ExternalTextureFrame::new(gl::TEXTURE_2D, *glid, gl::RGBA8, || {}))
        }
        None
    }
}

pub struct Texture {
    engine: FlutterEngine,
    texture: ExternalTexture,
    glid: Arc<AtomicU32>,
}

impl Texture {
    pub(crate) fn new(engine: FlutterEngine) -> Self {
        let texture = ExternalTexture::new(engine.engine_ptr());
        let texture2 = texture.clone();
        let glid = Arc::new(AtomicU32::new(0));
        let glid2 = glid.clone();
        let barrier = Arc::new(Barrier::new(2));
        let barrier2 = barrier.clone();
        engine.run_on_render_thread(move |engine| {
            let mut id: u32 = 0;
            unsafe {
                gl::GenTextures(1, &mut id as *mut _);
            }
            glid2.store(id, Ordering::SeqCst);
            engine.inner.texture_registry.register(texture2.id(), id);
            barrier2.wait();

            engine.run_on_platform_thread(move |_engine| {
                texture2.register();
            });
        });
        barrier.wait();
        Self {
            engine, texture, glid
        }
    }

    pub fn id(&self) -> TextureId {
        self.texture.id()
    }

    pub fn post_frame<F: FnOnce() + Send + 'static>(&self, render: F) {
        let glid = self.glid.load(Ordering::SeqCst);
        let texture = self.texture.clone();
        self.engine.run_on_render_thread(move |engine| {
            log::trace!("bound texture with glid {}", glid);
            unsafe { gl::BindTexture(gl::TEXTURE_2D, glid) };
            render();
            engine.run_on_platform_thread(move |_engine| {
                texture.mark_frame_available();
            });
        });
    }

    #[cfg(feature = "image")]
    pub fn post_frame_rgba(&self, img: RgbaImage) {
        self.post_frame(move || {
            let (width, height) = img.dimensions();
            unsafe {
                gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,             // mipmap level
                    gl::RGBA as _, // internal format of the texture
                    width as _,
                    height as _,
                    0,                           // border, must be 0
                    gl::RGBA,                    // format of the pixel data
                    gl::UNSIGNED_BYTE,           // data type of the pixel data
                    (&img).as_ptr() as *const _, // pixel data
                );
            }
        });
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        let id = self.glid.load(Ordering::SeqCst);
        log::trace!("dropping Texture with id {}", id);
        let texture = self.texture.clone();
        self.engine.run_on_platform_thread(move |_engine| {
            texture.unregister();
        });
        self.engine.run_on_render_thread(move |_engine| {
             unsafe {
                gl::DeleteTextures(1, &id as *const _);
            }
        });
    }
}
