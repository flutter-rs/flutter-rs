//! Metal presentation for the macOS backend.
//!
//! The engine renders each view into an `MTLTexture` created here and submits
//! its command buffers on the queue handed to it in `FlutterMetalRendererConfig`.
//! [`MetalState::present`] copies that texture into the window's `CAMetalLayer`
//! drawable on the *same* queue, so the copy is naturally ordered after the
//! engine's rendering and no explicit event synchronization is required.

use core_graphics_types::geometry::{CGRect, CGSize};
use metal::foreign_types::ForeignType;
use metal::objc::runtime::Object;
use metal::objc::{msg_send, sel, sel_impl};
use metal::{
    CommandQueue, Device, MTLBlitOption, MTLOrigin, MTLPixelFormat, MTLResourceOptions, MTLSize,
    MTLStorageMode, MTLTextureType, MTLTextureUsage, MetalLayer, Texture, TextureDescriptor,
};
use parking_lot::Mutex;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;

/// The layer and the last frame presented into it.
struct ViewLayer {
    layer: MetalLayer,
    /// The window's `NSView`, as a raw pointer kept alive by the window.
    ns_view: usize,
    /// The last texture the engine presented. The window server can invalidate
    /// a layer's drawable when the window is moved or changes screens, and the
    /// engine does not schedule a new frame for a pure move; keeping the last
    /// texture lets the compositor re-present it on demand.
    last: Option<Texture>,
    size: (usize, usize),
}

pub(crate) struct MetalState {
    device: Device,
    queue: CommandQueue,
    layers: Mutex<HashMap<i64, ViewLayer>>,
}

impl MetalState {
    pub(crate) fn new() -> Option<Arc<Self>> {
        let device = Device::system_default()?;
        let queue = device.new_command_queue();
        // `metal` owns the objects it gets from `MTLCreateSystemDefaultDevice`
        // and `newCommandQueue`, but the device is also bridged to the engine
        // through a plain pointer. Keep an extra reference so an autorelease
        // pool drain between now and the engine's first command cannot free it
        // out from under the engine.
        // SAFETY: both pointers are live Metal objects created just above.
        unsafe {
            retain(device.as_ptr() as *mut Object);
            retain(queue.as_ptr() as *mut Object);
        }
        Some(Arc::new(Self {
            device,
            queue,
            layers: Mutex::new(HashMap::new()),
        }))
    }

    pub(crate) fn device_ptr(&self) -> *const c_void {
        self.device.as_ptr().cast()
    }

    pub(crate) fn queue_ptr(&self) -> *const c_void {
        self.queue.as_ptr().cast()
    }

    /// Attach a `CAMetalLayer` to the window's `NSView`.
    ///
    /// GLFW creates an `NSOpenGLView`; the Metal path never draws through its
    /// GL context, so replacing the view's layer with a Metal layer is enough.
    // objc 0.2's macros expand a `cfg(feature = "cargo-clippy")` check into this
    // crate, where that feature does not exist.
    #[allow(unexpected_cfgs)]
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
        let layer = MetalLayer::new();
        layer.set_device(&self.device);
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        // The drawable destination of a blit must be usable as a blit target.
        layer.set_framebuffer_only(false);
        layer.set_opaque(true);
        layer.set_contents_scale(scale);
        layer.set_drawable_size(CGSize::new(
            f64::from(size.0.max(1)),
            f64::from(size.1.max(1)),
        ));
        // Add the layer as a sublayer of the view's AppKit-owned layer instead
        // of replacing that layer: AppKit swaps out a layer it did not create
        // when the window moves, which left the window black. Layers do not
        // participate in hit-testing, so input keeps reaching GLFW.
        //
        // SAFETY: the view comes from GLFW's live AppKit window and all
        // messages run on the platform (main) thread.
        unsafe {
            let parent = appkit.ns_view.as_ptr() as *mut Object;
            let _: () = msg_send![parent, setWantsLayer: true];
            let parent_layer: *mut Object = msg_send![parent, layer];
            let layer_ptr = layer.as_ptr() as *mut Object;
            let bounds: CGRect = msg_send![parent, bounds];
            let _: () = msg_send![layer_ptr, setFrame: bounds];
            if parent_layer.is_null() {
                log::error!("NSView did not create a backing layer for view {view_id}");
            } else {
                let _: () = msg_send![parent_layer, addSublayer: layer_ptr];
            }
        }
        self.layers.lock().insert(
            view_id,
            ViewLayer {
                layer,
                ns_view: appkit.ns_view.as_ptr() as usize,
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
    // objc 0.2's macros expand a `cfg(feature = "cargo-clippy")` check into this
    // crate, where that feature does not exist.
    #[allow(unexpected_cfgs)]
    fn refresh_layer(&self, view_id: i64) {
        let (ns_view, layer) = {
            let layers = self.layers.lock();
            let Some(view) = layers.get(&view_id) else {
                return;
            };
            (view.ns_view, view.layer.clone())
        };
        // SAFETY: `ns_view` belongs to the live GLFW window, and this runs on
        // the platform (main) thread.
        unsafe {
            let parent = ns_view as *mut Object;
            let bounds: CGRect = msg_send![parent, bounds];
            let layer_ptr = layer.as_ptr() as *mut Object;
            let _: () = msg_send![layer_ptr, setFrame: bounds];
            let parent_layer: *mut Object = msg_send![parent, layer];
            if !parent_layer.is_null() {
                let superlayer: *mut Object = msg_send![layer_ptr, superlayer];
                if superlayer != parent_layer {
                    log::debug!("Metal layer lost its superlayer for view {view_id}; re-adding");
                    let _: () = msg_send![parent_layer, addSublayer: layer_ptr];
                }
            }
        }
    }

    #[allow(unexpected_cfgs)]
    pub(crate) fn remove_layer(&self, view_id: i64) {
        let Some(view) = self.layers.lock().remove(&view_id) else {
            return;
        };
        // SAFETY: the layer is ours and this runs on the platform (main)
        // thread. `addSublayer:` retained it there; dropping the `MetalLayer`
        // releases our own reference.
        unsafe {
            let _: () = msg_send![view.layer.as_ptr() as *mut Object, removeFromSuperlayer];
        }
    }

    /// A render-target texture for one view's backing store.
    pub(crate) fn create_texture(&self, size: (usize, usize)) -> Option<Texture> {
        let descriptor = TextureDescriptor::new();
        descriptor.set_texture_type(MTLTextureType::D2);
        descriptor.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        descriptor.set_width(size.0 as u64);
        descriptor.set_height(size.1 as u64);
        descriptor.set_mipmap_level_count(1);
        descriptor.set_sample_count(1);
        descriptor.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        descriptor.set_storage_mode(MTLStorageMode::Private);
        Some(self.device.new_texture(&descriptor))
    }

    /// Copy `texture` into the drawable of the view's layer.
    pub(crate) fn present(
        &self,
        view_id: i64,
        texture: &Texture,
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
            view.layer.clone()
        };
        layer.set_contents_scale(scale);
        layer.set_drawable_size(CGSize::new(size.0.max(1) as f64, size.1.max(1) as f64));
        let Some(drawable) = layer.next_drawable() else {
            // Minimised or occluded: there is nothing to present into.
            log::debug!("no Metal drawable for view {view_id}");
            return true;
        };
        let command_buffer = self.queue.new_command_buffer();
        let encoder = command_buffer.new_blit_command_encoder();
        let origin = MTLOrigin { x: 0, y: 0, z: 0 };
        let copy_size = MTLSize {
            width: size.0 as u64,
            height: size.1 as u64,
            depth: 1,
        };
        encoder.copy_from_texture(
            texture,
            0,
            0,
            origin,
            copy_size,
            drawable.texture(),
            0,
            0,
            origin,
        );
        let dump = encode_dump(&self.device, encoder, drawable.texture(), size);
        encoder.end_encoding();
        command_buffer.present_drawable(drawable);
        command_buffer.commit();
        if let Some(dump) = dump {
            // One-shot verification: the readback is only valid once the GPU
            // has finished the copy, so wait for this frame.
            command_buffer.wait_until_completed();
            dump.write();
        }
        true
    }
}

#[allow(unexpected_cfgs)]
unsafe fn retain(object: *mut Object) {
    // SAFETY: `object` is a live Objective-C object; `retain` returns it.
    let _: *mut Object = unsafe { msg_send![object, retain] };
}

/// A captured frame waiting for the GPU to finish.
struct FrameDump {
    buffer: metal::Buffer,
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
                self.buffer.contents().cast::<u8>(),
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
    device: &Device,
    encoder: &metal::BlitCommandEncoderRef,
    drawable: &metal::TextureRef,
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
    let buffer = device.new_buffer(
        (bytes_per_row * height) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    encoder.copy_from_texture_to_buffer(
        drawable,
        0,
        0,
        MTLOrigin { x: 0, y: 0, z: 0 },
        MTLSize {
            width: width as u64,
            height: height as u64,
            depth: 1,
        },
        &buffer,
        0,
        bytes_per_row as u64,
        (bytes_per_row * height) as u64,
        MTLBlitOption::None,
    );
    Some(FrameDump {
        buffer,
        path: std::path::PathBuf::from(path).with_file_name("metal-frame.ppm"),
        width,
        height,
    })
}
