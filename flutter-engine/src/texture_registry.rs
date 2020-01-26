use crate::ffi::{ExternalTexture, ExternalTextureFrame, TextureId};
use crate::FlutterEngine;
#[cfg(feature = "image")]
use image::RgbaImage;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Texture {
    texture_id: TextureId,
    registry: TextureRegistry,
}

impl Texture {
    pub fn id(&self) -> TextureId {
        self.texture_id
    }

    pub fn mark_frame_available(&self) {
        self.registry.mark_frame_available(self.texture_id)
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        log::trace!("dropping Texture");
        self.registry.drop_texture(self.texture_id);
    }
}

#[derive(Clone, Default)]
pub struct TextureRegistry {
    textures: Arc<RwLock<HashMap<TextureId, FlutterTexture>>>,
}

impl TextureRegistry {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn create_texture(&self, engine: &FlutterEngine, gl: Box<dyn GlTexture>) -> Texture {
        let texture = ExternalTexture::new(engine.engine_ptr());
        let texture_id = texture.id();
        let texture = FlutterTexture::new(texture, gl);
        self.textures.write().insert(texture_id, texture);

        let textures = self.textures.clone();
        engine.run_on_platform_thread(move |_engine| {
            if let Some(texture) = textures.read().get(&texture_id) {
                texture.register();
            }
        });

        Texture {
            texture_id,
            registry: self.clone(),
        }
    }

    pub(crate) fn get_texture_frame(
        &self,
        texture_id: TextureId,
        _size: (usize, usize),
    ) -> Option<ExternalTextureFrame> {
        let textures = self.textures.read();
        textures
            .get(&texture_id)
            .map(|texture| texture.get_texture_frame())
    }

    fn mark_frame_available(&self, texture_id: TextureId) {
        let textures = self.textures.read();
        if let Some(texture) = textures.get(&texture_id) {
            texture.mark_frame_available();
        }
    }

    fn drop_texture(&self, texture_id: TextureId) {
        let mut textures = self.textures.write();
        textures.remove(&texture_id);
    }
}

struct FlutterTexture {
    texture: ExternalTexture,
    gl: Box<dyn GlTexture>,
}

impl FlutterTexture {
    fn new(texture: ExternalTexture, gl: Box<dyn GlTexture>) -> Self {
        Self { texture, gl }
    }

    fn register(&self) {
        self.texture.register();
    }

    fn mark_frame_available(&self) {
        self.texture.mark_frame_available();
    }

    fn get_texture_frame(&self) -> ExternalTextureFrame {
        self.gl.get_texture_frame()
    }
}

impl Drop for FlutterTexture {
    fn drop(&mut self) {
        log::trace!("dropping FlutterTexture");
        self.texture.unregister();
    }
}

pub trait GlTexture: Send + Sync {
    fn get_texture_frame(&self) -> ExternalTextureFrame;
}

#[cfg(feature = "image")]
#[derive(Clone)]
pub struct RgbaTexture(Arc<RwLock<RgbaImage>>);

#[cfg(feature = "image")]
impl RgbaTexture {
    pub fn new(img: RgbaImage) -> Self {
        Self(Arc::new(RwLock::new(img)))
    }

    pub fn post_frame_rgba(&mut self, img: RgbaImage) {
        *self.0.write() = img;
    }
}

#[cfg(feature = "image")]
impl GlTexture for RgbaTexture {
    fn get_texture_frame(&self) -> ExternalTextureFrame {
        let mut id: u32 = 0;
        let img = self.0.read();
        let (width, height) = img.dimensions();
        unsafe {
            gl::GenTextures(1, &mut id as *mut _);
            gl::BindTexture(gl::TEXTURE_2D, id);
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
        log::debug!("created gl texture with id {}", id);
        ExternalTextureFrame::new(gl::TEXTURE_2D, id, gl::RGBA8, move || unsafe {
            gl::DeleteTextures(1, &id as *const _);
        })
    }
}
