//! Per-view OpenGL compositing for the GLFW backend.
//!
//! The engine renders every view into a color texture attached to a
//! framebuffer in the *master* context. Presentation happens in the view's own
//! window context, which shares the texture but not the framebuffer object:
//! framebuffer names are per-context container objects, so `present_view`
//! creates a temporary framebuffer in the window's context, attaches the
//! shared texture and blits it into the window's default framebuffer.
//!
//! Presentation is blit-only for now. Without platform views the engine emits
//! exactly one backing-store layer per view; if a second layer appears it is
//! logged and skipped until the shader-based path is needed.

use crate::render::RenderState;
use flutter_engine::FlutterCompositorHandler;
use flutter_engine::view::FlutterViewId;
use flutter_engine_sys::{
    FlutterBackingStore, FlutterBackingStoreType, FlutterLayer, FlutterLayerContentType,
    FlutterOpenGLBackingStore, FlutterOpenGLBackingStore__bindgen_ty_1, FlutterOpenGLFramebuffer,
    FlutterOpenGLTargetType, FlutterSoftwareBackingStore2, FlutterSoftwarePixelFormat,
};
#[cfg(target_os = "macos")]
use flutter_engine_sys::{FlutterMetalBackingStore, FlutterMetalTexture};
use glfw::Context;
use std::cell::Cell;
use std::ffi::{CString, c_void};
use std::mem;
use std::ptr;
use std::sync::Arc;

thread_local! {
    /// `gl`'s function pointers are thread-local; load them once on the raster
    /// thread, where every compositor callback runs.
    static GL_LOADED: Cell<bool> = const { Cell::new(false) };
}

fn ensure_gl() {
    GL_LOADED.with(|loaded| {
        if loaded.get() {
            return;
        }
        gl::load_with(|name| {
            let Ok(name) = CString::new(name) else {
                return ptr::null();
            };
            // SAFETY: `name` is a NUL-terminated symbol name and a context is
            // current on this thread when the compositor calls this.
            unsafe { glfw::ffi::glfwGetProcAddress(name.as_ptr()) }
                .map_or(ptr::null(), |function| function as *const c_void)
        });
        loaded.set(true);
    });
}

/// The GL objects behind one `FlutterBackingStore`.
struct BackingStore {
    texture: u32,
    framebuffer: u32,
    depth_stencil: u32,
    size: (usize, usize),
}

/// The CPU allocation behind one software backing store.
struct SoftwareBackingStore {
    pixels: Box<[u8]>,
    size: (usize, usize),
}

/// One embedder-owned Metal texture handed to the engine as a backing store.
#[cfg(target_os = "macos")]
struct MetalBackingStore {
    texture: crate::metal::MetalTexture,
    size: (usize, usize),
}

/// Release a Metal backing store when the engine no longer needs it.
///
/// The Impeller Metal surface invokes `FlutterMetalTexture::destruction_callback`
/// unconditionally (it does not check for null), so this must always be
/// supplied. The engine owns the store from `create_backing_store` until this
/// callback runs; `collect_backing_store` intentionally does not free it.
///
/// # Safety
///
/// Called by the engine exactly once with the `user_data` from
/// `create_backing_store`'s `FlutterMetalTexture`.
#[cfg(target_os = "macos")]
unsafe extern "C" fn destroy_metal_backing_store(user_data: *mut c_void) {
    if user_data.is_null() {
        return;
    }
    // SAFETY: the pointer came from `Box::into_raw` in
    // `create_backing_store` and the engine promises to call this once.
    unsafe { drop(Box::from_raw(user_data as *mut MetalBackingStore)) };
}

/// How the engine's frames reach the window.
pub(crate) enum CompositorKind {
    /// Engine renders into an FBO in the master GL context; presentation blits
    /// it through the view's GL context.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    OpenGl,
    /// Engine rasterises on the CPU; presentation uploads the pixels through
    /// the view's GL context.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Software,
    /// Engine renders into embedder-provided Metal textures; presentation
    /// copies them into the window's `CAMetalLayer`.
    #[cfg(target_os = "macos")]
    Metal(Arc<crate::metal::MetalState>),
}

pub(crate) struct GlfwCompositor {
    state: Arc<RenderState>,
    kind: CompositorKind,
}

impl GlfwCompositor {
    pub(crate) fn new(state: Arc<RenderState>, kind: CompositorKind) -> Self {
        Self { state, kind }
    }

    fn with_master_current<R>(&self, f: impl FnOnce() -> R) -> R {
        let mut master = self.state.master.lock();
        master.make_current();
        // `glfwGetProcAddress` needs a current context, so load after making
        // the master current, not before.
        ensure_gl();
        f()
    }
}

impl FlutterCompositorHandler for GlfwCompositor {
    fn create_backing_store(
        &self,
        view_id: FlutterViewId,
        size: (usize, usize),
        store: &mut FlutterBackingStore,
    ) -> bool {
        // The implicit view starts with empty metrics; a 0×0 framebuffer is
        // incomplete and the engine may ask for one before the first window
        // reports its size. Allocate a 1×1 target instead.
        let size = (size.0.max(1), size.1.max(1));
        #[cfg(target_os = "macos")]
        if let CompositorKind::Metal(metal) = &self.kind {
            let Some(texture) = metal.create_texture(size) else {
                return false;
            };
            let texture_ptr = crate::metal::texture_ptr(&texture);
            let backing = Box::into_raw(Box::new(MetalBackingStore { texture, size }));
            store.struct_size = mem::size_of::<FlutterBackingStore>();
            store.user_data = ptr::null_mut();
            store.type_ = FlutterBackingStoreType::kFlutterBackingStoreTypeMetal;
            store.did_update = true;
            store.__bindgen_anon_1 = flutter_engine_sys::FlutterBackingStore__bindgen_ty_1 {
                metal: FlutterMetalBackingStore {
                    struct_size: mem::size_of::<FlutterMetalBackingStore>(),
                    __bindgen_anon_1: flutter_engine_sys::FlutterMetalBackingStore__bindgen_ty_1 {
                        texture: FlutterMetalTexture {
                            struct_size: mem::size_of::<FlutterMetalTexture>(),
                            texture_id: view_id,
                            texture: texture_ptr,
                            user_data: backing.cast::<c_void>(),
                            destruction_callback: Some(destroy_metal_backing_store),
                        },
                    },
                },
            };
            return true;
        }
        if matches!(self.kind, CompositorKind::Software) {
            let row_bytes = size.0 * 4;
            let pixels = vec![0u8; row_bytes * size.1].into_boxed_slice();
            let mut backing = Box::new(SoftwareBackingStore { pixels, size });
            // The allocation points into the boxed slice, not into the struct,
            // so it stays valid when the struct itself is moved into the heap.
            // SAFETY: `backing` is a live box; the pointer is only handed to the
            // engine for the lifetime of the store and is freed in
            // `collect_backing_store`.
            let allocation = backing.pixels.as_ptr().cast::<c_void>();
            let user_data = (&mut *backing as *mut SoftwareBackingStore).cast::<c_void>();
            // Ownership of the box passes to the engine, which hands it back in
            // `collect_backing_store`.
            let _ = Box::into_raw(backing);
            store.struct_size = mem::size_of::<FlutterBackingStore>();
            store.user_data = ptr::null_mut();
            store.type_ = FlutterBackingStoreType::kFlutterBackingStoreTypeSoftware2;
            store.did_update = true;
            store.__bindgen_anon_1 = flutter_engine_sys::FlutterBackingStore__bindgen_ty_1 {
                software2: FlutterSoftwareBackingStore2 {
                    struct_size: mem::size_of::<FlutterSoftwareBackingStore2>(),
                    allocation,
                    row_bytes,
                    height: size.1,
                    user_data,
                    destruction_callback: None,
                    // RGBA8888 lays the bytes out as r, g, b, a, which is what
                    // `glTexImage2D(GL_RGBA, GL_UNSIGNED_BYTE)` expects.
                    pixel_format: FlutterSoftwarePixelFormat::kFlutterSoftwarePixelFormatRGBA8888,
                },
            };
            return true;
        }
        self.with_master_current(|| {
            let width = size.0 as i32;
            let height = size.1 as i32;

            let mut texture = 0;
            // SAFETY: a context is current and every object name is an
            // out-parameter written exactly once.
            unsafe {
                gl::GenTextures(1, &mut texture);
                gl::BindTexture(gl::TEXTURE_2D, texture);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGBA8 as i32,
                    width,
                    height,
                    0,
                    gl::RGBA,
                    gl::UNSIGNED_BYTE,
                    ptr::null(),
                );
                gl::BindTexture(gl::TEXTURE_2D, 0);
            }

            let mut framebuffer = 0;
            let mut depth_stencil = 0;
            // SAFETY: as above.
            unsafe {
                gl::GenFramebuffers(1, &mut framebuffer);
                gl::BindFramebuffer(gl::FRAMEBUFFER, framebuffer);
                gl::FramebufferTexture2D(
                    gl::FRAMEBUFFER,
                    gl::COLOR_ATTACHMENT0,
                    gl::TEXTURE_2D,
                    texture,
                    0,
                );

                gl::GenRenderbuffers(1, &mut depth_stencil);
                gl::BindRenderbuffer(gl::RENDERBUFFER, depth_stencil);
                gl::RenderbufferStorage(gl::RENDERBUFFER, gl::DEPTH24_STENCIL8, width, height);
                gl::FramebufferRenderbuffer(
                    gl::FRAMEBUFFER,
                    gl::DEPTH_ATTACHMENT,
                    gl::RENDERBUFFER,
                    depth_stencil,
                );
                gl::FramebufferRenderbuffer(
                    gl::FRAMEBUFFER,
                    gl::STENCIL_ATTACHMENT,
                    gl::RENDERBUFFER,
                    depth_stencil,
                );

                let complete =
                    gl::CheckFramebufferStatus(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE;
                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
                if !complete {
                    gl::DeleteFramebuffers(1, &framebuffer);
                    gl::DeleteRenderbuffers(1, &depth_stencil);
                    gl::DeleteTextures(1, &texture);
                    log::error!("could not create a complete backing store for view {view_id}");
                    return false;
                }
            }

            let backing = Box::into_raw(Box::new(BackingStore {
                texture,
                framebuffer,
                depth_stencil,
                size,
            }));
            store.struct_size = mem::size_of::<FlutterBackingStore>();
            store.user_data = ptr::null_mut();
            store.type_ = FlutterBackingStoreType::kFlutterBackingStoreTypeOpenGL;
            store.did_update = true;
            store.__bindgen_anon_1 = flutter_engine_sys::FlutterBackingStore__bindgen_ty_1 {
                open_gl: FlutterOpenGLBackingStore {
                    type_: FlutterOpenGLTargetType::kFlutterOpenGLTargetTypeFramebuffer,
                    __bindgen_anon_1: FlutterOpenGLBackingStore__bindgen_ty_1 {
                        framebuffer: FlutterOpenGLFramebuffer {
                            target: gl::RGBA8,
                            name: framebuffer,
                            user_data: backing as *mut c_void,
                            destruction_callback: None,
                        },
                    },
                },
            };
            true
        })
    }

    fn collect_backing_store(&self, store: &FlutterBackingStore) {
        match store.type_ {
            FlutterBackingStoreType::kFlutterBackingStoreTypeSoftware2 => {
                // SAFETY: the engine only hands back stores this compositor
                // created, and `user_data` is the `Box<SoftwareBackingStore>`
                // pointer from `create_backing_store`.
                let backing = unsafe { store.__bindgen_anon_1.software2.user_data };
                if backing.is_null() {
                    return;
                }
                // SAFETY: the pointer came from `Box::into_raw` and the engine
                // promises to collect each store exactly once.
                unsafe { drop(Box::from_raw(backing as *mut SoftwareBackingStore)) };
            }
            #[cfg(target_os = "macos")]
            FlutterBackingStoreType::kFlutterBackingStoreTypeMetal => {
                // Metal stores are released by their `destruction_callback`
                // (Impeller calls it unconditionally and may still hold the
                // wrapped texture after this collect). Freeing here would be a
                // use-after-free when that callback runs.
            }
            FlutterBackingStoreType::kFlutterBackingStoreTypeOpenGL => {
                // SAFETY: as above, for the OpenGL `user_data` field.
                let backing = unsafe {
                    store
                        .__bindgen_anon_1
                        .open_gl
                        .__bindgen_anon_1
                        .framebuffer
                        .user_data
                };
                if backing.is_null() {
                    return;
                }
                self.with_master_current(|| {
                    // SAFETY: the pointer came from `Box::into_raw` above and
                    // the engine promises to collect each store exactly once.
                    let backing = unsafe { Box::from_raw(backing as *mut BackingStore) };
                    // SAFETY: a context is current; these names belong to it.
                    unsafe {
                        gl::DeleteFramebuffers(1, &backing.framebuffer);
                        gl::DeleteRenderbuffers(1, &backing.depth_stencil);
                        gl::DeleteTextures(1, &backing.texture);
                    }
                });
            }
            _ => {}
        }
    }

    fn present_view(&self, view_id: FlutterViewId, layers: &[&FlutterLayer]) -> bool {
        let Some((context, framebuffer_size, scale, shrink_wrap)) = ({
            let views = self.state.views.lock();
            views.get(&view_id).map(|view| {
                (
                    Arc::clone(&view.context),
                    view.framebuffer_size,
                    view.scale,
                    view.shrink_wrap,
                )
            })
        }) else {
            // The view has no native window (for example the implicit view
            // before the first window reports metrics). Nothing to present.
            return true;
        };

        let Some(first) = layers.iter().find(|layer| {
            layer.type_ == FlutterLayerContentType::kFlutterLayerContentTypeBackingStore
        }) else {
            return true;
        };
        if layers.len() > 1 {
            log::warn!(
                "view {view_id} presented {} layers; only the first backing store is composited",
                layers.len()
            );
        }

        // SAFETY: `first` is a backing-store layer, so the union holds
        // `backing_store`, and the engine keeps the store alive for this call.
        let store = unsafe { first.__bindgen_anon_1.backing_store };
        if store.is_null() {
            return true;
        }
        // SAFETY: the layer is a backing-store layer; the engine keeps the
        // store alive for the duration of this call.
        let store = unsafe { &*store };

        #[cfg(target_os = "macos")]
        if let CompositorKind::Metal(metal) = &self.kind {
            // SAFETY: `user_data` is the `Box<MetalBackingStore>` created for
            // this engine.
            let user_data = unsafe {
                store
                    .__bindgen_anon_1
                    .metal
                    .__bindgen_anon_1
                    .texture
                    .user_data
            };
            if user_data.is_null() {
                return true;
            }
            // SAFETY: as above.
            let backing = unsafe { &*(user_data as *const MetalBackingStore) };
            return metal.present(view_id, &backing.texture, backing.size, scale);
        }

        // Resolve the source. OpenGL backing stores live in the master context
        // and need a finish before another context reads them; software stores
        // are CPU memory and are uploaded into a texture in the view context.
        let mut gl_source = None;
        let software = match store.type_ {
            FlutterBackingStoreType::kFlutterBackingStoreTypeOpenGL => {
                // The engine wrote the backing texture while the master context
                // was current. Flush is not a completion barrier; finish before
                // another context reads the texture.
                self.with_master_current(|| {
                    // SAFETY: a context is current; `glFinish` has no
                    // out-parameters.
                    unsafe { gl::Finish() };
                });
                // SAFETY: `user_data` is the `Box<BackingStore>` created for
                // this engine.
                let user_data = unsafe {
                    store
                        .__bindgen_anon_1
                        .open_gl
                        .__bindgen_anon_1
                        .framebuffer
                        .user_data
                };
                if !user_data.is_null() {
                    // SAFETY: as above.
                    let backing = unsafe { &*(user_data as *const BackingStore) };
                    gl_source = Some((backing.texture, backing.size));
                }
                None
            }
            FlutterBackingStoreType::kFlutterBackingStoreTypeSoftware2 => {
                // SAFETY: `user_data` is the `Box<SoftwareBackingStore>` created
                // for this engine.
                let user_data = unsafe { store.__bindgen_anon_1.software2.user_data };
                if user_data.is_null() {
                    None
                } else {
                    // SAFETY: as above.
                    Some(unsafe { &*(user_data as *const SoftwareBackingStore) })
                }
            }
            _ => None,
        };

        let mut context = context.lock();
        context.make_current();
        ensure_gl();

        let mut uploaded_texture = 0;
        let (texture, texture_size) = match software {
            Some(backing) => {
                // SAFETY: a context is current. The temporary texture is
                // uploaded here, blitted below and deleted with the temporary
                // framebuffer.
                unsafe {
                    gl::GenTextures(1, &mut uploaded_texture);
                    gl::BindTexture(gl::TEXTURE_2D, uploaded_texture);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
                    gl::TexImage2D(
                        gl::TEXTURE_2D,
                        0,
                        gl::RGBA8 as i32,
                        backing.size.0 as i32,
                        backing.size.1 as i32,
                        0,
                        gl::RGBA,
                        gl::UNSIGNED_BYTE,
                        backing.pixels.as_ptr().cast::<c_void>(),
                    );
                    gl::BindTexture(gl::TEXTURE_2D, 0);
                }
                (uploaded_texture, backing.size)
            }
            None => match gl_source {
                Some(source) => source,
                None => return true,
            },
        };

        let mut saved_read = 0;
        let mut saved_draw = 0;
        // SAFETY: a context is current; the framebuffer names are written by
        // `GetIntegerv` and the temporary framebuffer is created and deleted
        // in this context.
        unsafe {
            gl::GetIntegerv(gl::READ_FRAMEBUFFER_BINDING, &mut saved_read);
            gl::GetIntegerv(gl::DRAW_FRAMEBUFFER_BINDING, &mut saved_draw);

            let mut present_framebuffer = 0;
            gl::GenFramebuffers(1, &mut present_framebuffer);
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, present_framebuffer);
            gl::FramebufferTexture2D(
                gl::READ_FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                texture,
                0,
            );

            let destination_width = framebuffer_size.0.max(texture_size.0 as i32).max(1);
            let destination_height = framebuffer_size.1.max(texture_size.1 as i32).max(1);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
            // Software surfaces are top-down, but a texture upload puts their
            // first row at the texture's bottom; invert the source rectangle so
            // the window shows the frame upright. GL backing stores are already
            // bottom-up and keep the normal rectangle.
            let (source_y0, source_y1) = if uploaded_texture != 0 {
                (texture_size.1 as i32, 0)
            } else {
                (0, texture_size.1 as i32)
            };
            gl::BlitFramebuffer(
                0,
                source_y0,
                texture_size.0 as i32,
                source_y1,
                0,
                0,
                destination_width,
                destination_height,
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );

            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, saved_read as u32);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, saved_draw as u32);
            gl::DeleteFramebuffers(1, &present_framebuffer);
            if uploaded_texture != 0 {
                gl::DeleteTextures(1, &uploaded_texture);
            }
        }

        context.swap_buffers();

        if shrink_wrap && scale.is_finite() && scale > 0.0 {
            let desired = (texture_size.0 as f64 / scale, texture_size.1 as f64 / scale);
            let current = (f64::from(framebuffer_size.0), f64::from(framebuffer_size.1));
            // Compare physical pixels; a one-pixel difference is rounding noise
            // and would otherwise resize the window on every frame.
            if (texture_size.0 as f64 - current.0).abs() > 2.0
                || (texture_size.1 as f64 - current.1).abs() > 2.0
            {
                self.state.set_desired_size(view_id, desired);
            }
        }

        true
    }
}
