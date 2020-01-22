use crate::ffi::{ExternalTexture, ExternalTextureFrame, TextureId};
use crate::FlutterEngine;
use image::RgbaImage;
use std::collections::HashMap;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct TextureRegistry {
    textures: Arc<Mutex<HashMap<TextureId, ExternalTexture>>>,
    data: Arc<Mutex<HashMap<TextureId, RgbaImage>>>,
}

impl TextureRegistry {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn create_texture(&self, engine: &FlutterEngine, img: RgbaImage) -> TextureId {
        static TEXTURE_ID: AtomicI64 = AtomicI64::new(1);
        let texture_id = TEXTURE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        self.data.lock().unwrap().insert(texture_id, img);

        let textures = self.textures.clone();
        engine.run_on_platform_thread(move |engine| {
            let texture = engine.register_external_texture(texture_id);
            textures.lock().unwrap().insert(texture_id, texture);
        });
        texture_id
    }

    pub fn get_texture_frame(
        &self,
        texture_id: TextureId,
        _size: (usize, usize),
    ) -> Option<ExternalTextureFrame> {
        let data = self.data.lock().unwrap();
        let img = data.get(&texture_id).unwrap();
        Some(unsafe { create_gl_texture(texture_id, img) })
    }

    pub fn mark_frame_available(&self, texture_id: TextureId) {
        if let Some(texture) = self.textures.lock().unwrap().get(&texture_id) {
            texture.mark_frame_available();
        }
    }
}

unsafe fn create_gl_texture(texture_id: TextureId, img: &RgbaImage) -> ExternalTextureFrame {
    let (width, height) = img.dimensions();
    log::debug!(
        "creating external texture with id {}, size {}x{}",
        texture_id,
        width,
        height,
    );
    let mut texture_name: u32 = 0;
    gl::GenTextures(1, &mut texture_name as *mut _);
    gl::BindTexture(gl::TEXTURE_2D, texture_name);
    gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
    let data = &img;
    gl::TexImage2D(
        gl::TEXTURE_2D,
        0,             // mipmap level
        gl::RGBA as _, // internal format of the texture
        width as _,
        height as _,
        0,                         // border, must be 0
        gl::RGBA,                  // format of the pixel data
        gl::UNSIGNED_BYTE,         // data type of the pixel data
        data.as_ptr() as *const _, // pixel data
    );
    log::debug!(
        "created texture {}, gl texture {}",
        texture_id,
        texture_name
    );
    ExternalTextureFrame::new(gl::TEXTURE_2D, texture_name, gl::RGBA8, move || {
        log::debug!(
            "destroying texture {}, gl texture {}",
            texture_id,
            texture_name
        );
        let texture_name = texture_name;
        gl::DeleteTextures(1, &texture_name as *const _)
    })
}
