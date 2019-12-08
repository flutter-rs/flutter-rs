use std::{
    collections::HashMap,
    convert::TryInto,
    sync::{atomic::AtomicI64, Arc, Mutex, Weak},
};

use flutter_engine_sys::FlutterOpenGLTexture;
use gl::types::*;
use log::{debug, trace};

use crate::ffi::{ExternalTexture as FlutterTexture, FlutterEngine};

type TextureID = i64;

pub struct TextureRegistry {
    engine: Arc<FlutterEngine>,
    textures: TextureStore,
}

struct TextureStore {
    /// Stores freshly registered textures without an OpenGL texture attached.
    initial: HashMap<TextureID, Arc<ExternalTexture>>,
    /// Stores created textures that may be dropped in other locations.
    created: HashMap<TextureID, Weak<ExternalTexture>>,
}

pub struct ExternalTexture {
    texture: FlutterTexture,
    texture_data: Mutex<Option<TextureData>>,
}

struct TextureData {
    name: GLuint,
    width: u32,
    height: u32,
}

struct TextureUserData {
    texture_id: TextureID,
    texture_name: GLuint,
}

impl TextureRegistry {
    pub fn new(engine: Arc<FlutterEngine>) -> Self {
        Self {
            engine,
            textures: TextureStore {
                initial: HashMap::new(),
                created: HashMap::new(),
            },
        }
    }

    pub fn create_texture(&mut self) -> Arc<ExternalTexture> {
        static TEXTURE_ID: AtomicI64 = AtomicI64::new(1);

        let texture_id = TEXTURE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let flutter_texture = self.engine.register_external_texture(texture_id);

        let texture = Arc::new(ExternalTexture {
            texture: flutter_texture,
            texture_data: Mutex::new(None),
        });

        self.textures
            .initial
            .insert(texture_id, Arc::clone(&texture));

        texture
    }

    pub(crate) fn texture_callback(
        &mut self,
        texture_id: TextureID,
        (width, height): (u32, u32),
        gl_texture: &mut FlutterOpenGLTexture,
    ) -> bool {
        if let Some(texture) = self.textures.initial.remove(&texture_id) {
            // texture is still initial --> create it
            unsafe {
                create_gl_texture(texture_id, (width, height), gl_texture);
            }
            let mut data = texture.texture_data.lock().unwrap();
            data.replace(TextureData {
                name: gl_texture.name,
                width,
                height,
            });
            self.textures
                .created
                .insert(texture_id, Arc::downgrade(&texture));
            return true;
        }

        if let Some(texture) = self.textures.created.get(&texture_id) {
            // texture has been created, this is a notification that a new frame is available
            if let Some(_texture) = texture.upgrade() {
                // texture still alive
                // TODO: notify of new frame
            } else {
                // texture was dropped, remove it from here as well
                self.textures.created.remove(&texture_id);
            }
        }
        false
    }
}

impl ExternalTexture {
    pub fn handle(&self) -> TextureID {
        self.texture.texture_id
    }

    pub fn gl_texture(&self) -> Option<GLuint> {
        let data = self.texture_data.lock().unwrap();
        data.as_ref().map(|data| data.name)
    }

    pub fn size(&self) -> Option<(u32, u32)> {
        let data = self.texture_data.lock().unwrap();
        data.as_ref().map(|data| (data.width, data.height))
    }

    pub fn mark_frame_available(&self) {
        self.texture.mark_frame_available();
    }
}

unsafe fn create_gl_texture(
    texture_id: TextureID,
    (width, height): (u32, u32),
    gl_texture: &mut flutter_engine_sys::FlutterOpenGLTexture,
) {
    debug!(
        "creating external texture with id {}, size {}x{}",
        texture_id, width, height,
    );
    gl_texture.target = gl::TEXTURE_2D;
    gl_texture.format = gl::RGBA8;
    gl::GenTextures(1, &mut gl_texture.name as *mut _);
    gl::BindTexture(gl::TEXTURE_2D, gl_texture.name);
    gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
    gl::TexParameteri(
        gl::TEXTURE_2D,
        gl::TEXTURE_MIN_FILTER,
        gl::LINEAR.try_into().unwrap(),
    );
    gl::TexParameteri(
        gl::TEXTURE_2D,
        gl::TEXTURE_MAG_FILTER,
        gl::LINEAR.try_into().unwrap(),
    );
    // length of data: 4 bytes per pixel (RGBA)
    let data_length = (width * height * 4) as usize;
    let data = vec![0_u8; data_length];
    gl::TexImage2D(
        gl::TEXTURE_2D,
        0,                            // mipmap level
        gl::RGBA.try_into().unwrap(), // internal format of the texture
        width.try_into().unwrap(),
        height.try_into().unwrap(),
        0,                         // border, must be 0
        gl::RGBA,                  // format of the pixel data
        gl::UNSIGNED_BYTE,         // data type of the pixel data
        data.as_ptr() as *const _, // pixel data
    );
    let user_data = Box::new(TextureUserData {
        texture_id,
        texture_name: gl_texture.name,
    });
    gl_texture.user_data = Box::into_raw(user_data) as *mut _;
    gl_texture.destruction_callback = Some(texture_destruction_callback);
    debug!(
        "created texture {}, gl texture {}",
        texture_id, gl_texture.name
    );
}

unsafe extern "C" fn texture_destruction_callback(user_data: *mut libc::c_void) {
    trace!("texture_destruction_callback");
    let user_data = Box::from_raw(user_data as *mut TextureUserData);
    debug!(
        "destroying texture {}, gl texture {}",
        user_data.texture_id, user_data.texture_name
    );
    gl::DeleteTextures(1, &user_data.texture_name as *const _);
}
