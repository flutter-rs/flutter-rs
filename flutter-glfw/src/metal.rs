//! Metal presentation for the macOS backend.
//!
//! The engine renders each view into an `MTLTexture` created here and submits
//! its command buffers on the queue handed to it in `FlutterMetalRendererConfig`.
//! [`MetalState::present`] copies that texture into the window's `CAMetalLayer`
//! drawable on the *same* queue, so the copy is naturally ordered after the
//! engine's rendering and no explicit event synchronization is required.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{class, msg_send};
use objc2_core_foundation::{CGRect, CGSize};
use objc2_metal::{
    MTLBlitCommandEncoder, MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLCreateSystemDefaultDevice, MTLDevice, MTLDrawable, MTLOrigin, MTLPixelFormat,
    MTLResourceOptions, MTLSize, MTLStorageMode, MTLTexture, MTLTextureDescriptor, MTLTextureUsage,
};
use parking_lot::Mutex;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;

/// An engine-owned render target texture.
pub(crate) type MetalTexture = Retained<ProtocolObject<dyn MTLTexture>>;

/// The raw `id<MTLTexture>` pointer the engine expects in its backing store.
pub(crate) fn texture_ptr(texture: &MetalTexture) -> *const c_void {
    Retained::as_ptr(texture).cast()
}

/// The layer and the last frame presented into it.
struct ViewLayer {
    /// Our `CAMetalLayer`, owned by the `new` reference and retained by its
    /// superlayer while attached.
    layer: *mut AnyObject,
    /// The window's `NSView`, as a raw pointer kept alive by the window.
    ns_view: *mut AnyObject,
    /// The last texture the engine presented. The window server can invalidate
    /// a layer's drawable when the window is moved or changes screens, and the
    /// engine does not schedule a new frame for a pure move; keeping the last
    /// texture lets the compositor re-present it on demand.
    last: Option<MetalTexture>,
    size: (usize, usize),
}

pub(crate) struct MetalState {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    layers: Mutex<HashMap<i64, ViewLayer>>,
}

// SAFETY: Metal objects are documented as safe to use from multiple threads,
// and the per-view layer map is protected by the mutex. The texture type is not
// marked `Send + Sync` by the bindings, but a retained texture handle is.
unsafe impl Send for MetalState {}
unsafe impl Sync for MetalState {}

impl MetalState {
    pub(crate) fn new() -> Option<Arc<Self>> {
        let device = MTLCreateSystemDefaultDevice()?;
        let queue = device.newCommandQueue()?;
        Some(Arc::new(Self {
            device,
            queue,
            layers: Mutex::new(HashMap::new()),
        }))
    }

    pub(crate) fn device_ptr(&self) -> *const c_void {
        Retained::as_ptr(&self.device).cast()
    }

    pub(crate) fn queue_ptr(&self) -> *const c_void {
        Retained::as_ptr(&self.queue).cast()
    }

    /// Attach a `CAMetalLayer` to the window's `NSView`.
    ///
    /// GLFW creates an `NSOpenGLView`; the Metal path never draws through its
    /// GL context. The layer is added as a sublayer of the view's AppKit-owned
    /// layer instead of replacing it: AppKit swaps out a layer it did not
    /// create when the window moves, which left the window black. Layers do not
    /// participate in hit-testing, so input keeps reaching GLFW.
    pub(crate) fn attach_layer(
        &self,
        window: &glfw::PWindow,
        view_id: i64,
        size: (i32, i32),
        scale: f64,
    ) {
        let Ok(handle) = window.window_handle() else {
            log::error!("could not get the AppKit handle for view {view_id}");
            return;
        };
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            log::error!("window for view {view_id} is not an AppKit window");
            return;
        };
        // SAFETY: `new` creates a fresh layer we own; every message below is a
        // plain property of `CAMetalLayer`/`NSView`/`CALayer`, sent on the
        // platform (main) thread.
        let layer: *mut AnyObject = unsafe { msg_send![class!(CAMetalLayer), new] };
        if layer.is_null() {
            log::error!("could not create a CAMetalLayer for view {view_id}");
            return;
        }
        let device = Retained::as_ptr(&self.device) as *mut AnyObject;
        let drawable_size = CGSize::new(f64::from(size.0.max(1)), f64::from(size.1.max(1)));
        let parent = appkit.ns_view.as_ptr() as *mut AnyObject;
        // SAFETY: the pointers are live objects; see the comment above.
        unsafe {
            let _: () = msg_send![layer, setDevice: device];
            let _: () = msg_send![layer, setPixelFormat: MTLPixelFormat::BGRA8Unorm.0];
            // The drawable destination of a blit must be usable as a blit target.
            let _: () = msg_send![layer, setFramebufferOnly: false];
            let _: () = msg_send![layer, setOpaque: true];
            let _: () = msg_send![layer, setContentsScale: scale];
            let _: () = msg_send![layer, setDrawableSize: drawable_size];

            let _: () = msg_send![parent, setWantsLayer: true];
            let parent_layer: *mut AnyObject = msg_send![parent, layer];
            let bounds: CGRect = msg_send![parent, bounds];
            let _: () = msg_send![layer, setFrame: bounds];
            if parent_layer.is_null() {
                log::error!("NSView did not create a backing layer for view {view_id}");
            } else {
                let _: () = msg_send![parent_layer, addSublayer: layer];
            }
        }
        self.layers.lock().insert(
            view_id,
            ViewLayer {
                layer,
                ns_view: parent,
                last: None,
                size: (size.0.max(1) as usize, size.1.max(1) as usize),
            },
        );
    }

    /// Re-present the last frame after the layer's drawable was invalidated.
    pub(crate) fn represent(&self, view_id: i64, scale: f64) {
        let cached = {
            let layers = self.layers.lock();
            layers
                .get(&view_id)
                .and_then(|view| view.last.clone().map(|texture| (texture, view.size)))
        };
        if let Some((texture, size)) = cached {
            log::debug!("re-presenting the last Metal frame for view {view_id}");
            self.refresh_layer(view_id);
            self.present(view_id, &texture, size, scale);
        }
    }

    /// Keep the sublayer in step with the view's bounds and parent layer.
    ///
    /// AppKit can replace the view's own layer when the window moves or changes
    /// screens; the sublayer then has no superlayer anymore, so re-attach it.
    fn refresh_layer(&self, view_id: i64) {
        let (ns_view, layer) = {
            let layers = self.layers.lock();
            let Some(view) = layers.get(&view_id) else {
                return;
            };
            (view.ns_view, view.layer)
        };
        // SAFETY: `ns_view` belongs to the live GLFW window, and this runs on
        // the platform (main) thread.
        unsafe {
            let bounds: CGRect = msg_send![ns_view, bounds];
            let _: () = msg_send![layer, setFrame: bounds];
            let parent_layer: *mut AnyObject = msg_send![ns_view, layer];
            if !parent_layer.is_null() {
                let superlayer: *mut AnyObject = msg_send![layer, superlayer];
                if superlayer != parent_layer {
                    log::debug!("Metal layer lost its superlayer for view {view_id}; re-adding");
                    let _: () = msg_send![parent_layer, addSublayer: layer];
                }
            }
        }
    }

    pub(crate) fn remove_layer(&self, view_id: i64) {
        let Some(view) = self.layers.lock().remove(&view_id) else {
            return;
        };
        // SAFETY: the layer is ours and this runs on the platform (main)
        // thread. `removeFromSuperlayer` releases the superlayer's reference;
        // `release` drops the reference created by `new`.
        unsafe {
            let _: () = msg_send![view.layer, removeFromSuperlayer];
            let _: () = msg_send![view.layer, release];
        }
    }

    /// A render-target texture for one view's backing store.
    pub(crate) fn create_texture(&self, size: (usize, usize)) -> Option<MetalTexture> {
        // SAFETY: the descriptor factory only reads its arguments.
        let descriptor = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                MTLPixelFormat::BGRA8Unorm,
                size.0.max(1),
                size.1.max(1),
                false,
            )
        };
        descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        descriptor.setStorageMode(MTLStorageMode::Private);
        self.device.newTextureWithDescriptor(&descriptor)
    }

    /// Copy `texture` into the drawable of the view's layer.
    pub(crate) fn present(
        &self,
        view_id: i64,
        texture: &MetalTexture,
        size: (usize, usize),
        scale: f64,
    ) -> bool {
        let layer = {
            let mut layers = self.layers.lock();
            let Some(view) = layers.get_mut(&view_id) else {
                return true;
            };
            view.last = Some(texture.clone());
            view.size = size;
            view.layer
        };
        let drawable_size = CGSize::new(size.0.max(1) as f64, size.1.max(1) as f64);
        // SAFETY: a layer and its drawable are live Metal/QuartzCore objects,
        // and this runs on the raster thread, which owns the engine's queue.
        // The `nextDrawable` and `texture` messages are safe on those objects.
        let drawable = unsafe {
            let _: () = msg_send![layer, setContentsScale: scale];
            let _: () = msg_send![layer, setDrawableSize: drawable_size];
            let drawable: *mut AnyObject = msg_send![layer, nextDrawable];
            drawable
        };
        if drawable.is_null() {
            // Minimised or occluded: there is nothing to present into.
            log::debug!("no Metal drawable for view {view_id}");
            return true;
        }
        // SAFETY: the drawable is a `CAMetalDrawable` and its texture is an
        // `MTLTexture`; both are live for the duration of this presentation.
        let drawable_texture = unsafe {
            let texture: *mut AnyObject = msg_send![drawable, texture];
            if texture.is_null() {
                return true;
            }
            &*(texture as *const ProtocolObject<dyn MTLTexture>)
        };
        let Some(command_buffer) = self.queue.commandBuffer() else {
            return true;
        };
        let Some(encoder) = command_buffer.blitCommandEncoder() else {
            return true;
        };
        let origin = MTLOrigin { x: 0, y: 0, z: 0 };
        let copy_size = MTLSize {
            width: size.0.max(1),
            height: size.1.max(1),
            depth: 1,
        };
        // SAFETY: the copy is within the source and destination textures, and
        // both are alive until the command buffer completes.
        unsafe {
            encoder
                .copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                    texture,
                    0,
                    0,
                    origin,
                    copy_size,
                    drawable_texture,
                    0,
                    0,
                    origin,
                );
        }
        let dump = encode_dump(&self.device, &encoder, drawable_texture, size);
        encoder.endEncoding();
        // SAFETY: the drawable is a live `CAMetalDrawable`.
        let drawable_ref = unsafe { &*(drawable as *const ProtocolObject<dyn MTLDrawable>) };
        command_buffer.presentDrawable(drawable_ref);
        command_buffer.commit();
        if let Some(dump) = dump {
            // One-shot verification: the readback is only valid once the GPU
            // has finished the copy, so wait for this frame.
            command_buffer.waitUntilCompleted();
            dump.write();
        }
        true
    }
}

/// A captured frame waiting for the GPU to finish.
struct FrameDump {
    buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    path: std::path::PathBuf,
    width: usize,
    height: usize,
}

impl FrameDump {
    fn write(self) {
        use std::io::Write;

        let bytes_per_row = self.width * 4;
        // SAFETY: the buffer is shared storage with exactly this length, and
        // the command buffer that fills it has completed.
        let pixels = unsafe {
            std::slice::from_raw_parts(
                self.buffer.contents().as_ptr().cast::<u8>(),
                bytes_per_row * self.height,
            )
        };
        let Ok(mut file) = std::fs::File::create(&self.path) else {
            return;
        };
        if write!(file, "P6\n{} {}\n255\n", self.width, self.height).is_err() {
            return;
        }
        // BGRA8888 in memory; PPM wants RGB. Metal rows start at the top.
        let mut row = Vec::with_capacity(self.width * 3);
        for y in 0..self.height {
            row.clear();
            for x in 0..self.width {
                let offset = (y * self.width + x) * 4;
                row.push(pixels[offset + 2]);
                row.push(pixels[offset + 1]);
                row.push(pixels[offset]);
            }
            if file.write_all(&row).is_err() {
                return;
            }
        }
        log::info!("dumped Metal frame to {}", self.path.display());
    }
}

/// Read the presented drawable back once when `FLUTTER_GLFW_DUMP_FRAME` is set.
///
/// Temporary verification aid: screenshot capture over SSH is blocked by TCC,
/// so this proves what the window actually received.
fn encode_dump(
    device: &ProtocolObject<dyn MTLDevice>,
    encoder: &ProtocolObject<dyn MTLBlitCommandEncoder>,
    drawable: &ProtocolObject<dyn MTLTexture>,
    size: (usize, usize),
) -> Option<FrameDump> {
    use std::sync::atomic::{AtomicBool, Ordering};

    static DUMPED: AtomicBool = AtomicBool::new(false);
    let path = std::env::var_os("FLUTTER_GLFW_DUMP_FRAME")?;
    if DUMPED.swap(true, Ordering::Relaxed) {
        return None;
    }
    let (width, height) = (size.0.max(1), size.1.max(1));
    let bytes_per_row = width * 4;
    let buffer = device.newBufferWithLength_options(
        bytes_per_row * height,
        MTLResourceOptions::StorageModeShared,
    )?;
    // SAFETY: the copy is within the destination buffer, which is alive until
    // the command buffer completes.
    unsafe {
        encoder
            .copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                drawable,
                0,
                0,
                MTLOrigin { x: 0, y: 0, z: 0 },
                MTLSize {
                    width,
                    height,
                    depth: 1,
                },
                &buffer,
                0,
                bytes_per_row,
                bytes_per_row * height,
            );
    }
    Some(FrameDump {
        buffer,
        path: std::path::PathBuf::from(path).with_file_name("metal-frame.ppm"),
        width,
        height,
    })
}
