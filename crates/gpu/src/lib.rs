//! GPU renderer on wgpu: Direct3D 12 / Vulkan on Windows, Metal on macOS, Vulkan on
//! Linux. Port of the Swift build's `MetalPipeline` + `MetalTarget`.
//!
//! The image is uploaded once, every stage runs on the device, and it comes back once.
//! Healing stays on the CPU (a few small discs: download, fix, upload costs less than a
//! kernel). Anything that fails — no adapter, a shader that does not compile, an image
//! over the size cap — makes `apply` return an error and the caller renders on the CPU.

pub mod display;
mod shaders;

/// The wgpu this crate is built on, for callers that share its device.
pub use wgpu;

use awpr_core::color;
use awpr_core::pipeline::{
    self, BlurOp, PixelStageParams, ProcessContext, ResampleParams, StageError, StageTarget,
};
use awpr_core::masks::{self, MaskParams};
use awpr_core::v3::V3Params;
use awpr_core::{FloatImage, ImageAdjustments, Rotation};
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use std::sync::Mutex;
use wgpu::util::DeviceExt;

// ---- parameter structs, laid out to match the WGSL declarations ----------------

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuPixelParams {
    flags: u32,
    gradient_count: u32,
    wb: [f32; 3],
    m: [f32; 9],
    sat: f32,
    vib: f32,
    vig_amount: f32,
    vig_cx: f32,
    vig_cy: f32,
    vig_inv_max: f32,
    width: u32,
    height: u32,
    _pad: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuGradient {
    sin_a: f32,
    cos_a: f32,
    center_x: f32,
    center_y: f32,
    inv2_range: f32,
    exposure: f32,
    contrast: f32,
    highlights: f32,
    shadows: f32,
    saturation: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuBlurParams {
    width: u32,
    height: u32,
    radius: i32,
    mode: i32,
    amount: f32,
    _pad: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuResampleParams {
    src_width: u32,
    src_height: u32,
    out_width: u32,
    out_height: u32,
    mode: i32,
    k: f32,
    cx: f32,
    cy: f32,
    sin_a: f32,
    cos_a: f32,
    ox: f32,
    oy: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuRotateParams {
    width: u32,
    height: u32,
    rot: i32,
    _pad: u32,
}

const FLAG_LEGACY_WB: u32 = 1 << 0;
const FLAG_LINEAR_MUL: u32 = 1 << 1;
const FLAG_LINEAR_MATRIX: u32 = 1 << 2;
const FLAG_TONE_LUT: u32 = 1 << 3;
const FLAG_VIB_SAT: u32 = 1 << 4;
const FLAG_GRADIENTS: u32 = 1 << 5;
const FLAG_GRADIENT_LINEAR: u32 = 1 << 6;
const FLAG_VIGNETTE: u32 = 1 << 7;

#[derive(Debug)]
pub struct GpuError(pub String);

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GpuError {}

fn err(s: impl Into<String>) -> StageError {
    Box::new(GpuError(s.into()))
}

struct Kernels {
    pixel: wgpu::ComputePipeline,
    pixel_v3: wgpu::ComputePipeline,
    masks: wgpu::ComputePipeline,
    blur_h: wgpu::ComputePipeline,
    blur_v: wgpu::ComputePipeline,
    blur_combine: wgpu::ComputePipeline,
    resample: wgpu::ComputePipeline,
    rotate: wgpu::ComputePipeline,
    histogram: wgpu::ComputePipeline,
}

const HISTOGRAM_BYTES: u64 = 768 * 4;

/// An in-flight histogram (see `GpuPipeline::histogram`).
pub struct HistogramJob {
    result: std::sync::Arc<Mutex<Option<Vec<u32>>>>,
}

impl HistogramJob {
    /// R, G, B counts (256 each, in that order) once the GPU is done.
    pub fn take(&self) -> Option<Vec<u32>> {
        self.result.lock().unwrap().take()
    }
}

/// Device, kernels and the transfer-curve tables, created once.
pub struct GpuPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    kernels: Kernels,
    decode_lut: wgpu::Buffer,
    encode_lut: wgpu::Buffer,
    /// Adapter name and backend, for diagnostics.
    pub status: String,
    /// Largest image (pixels) the GPU path takes on. The device limit is the hard one
    /// (three full copies coexist at the worst moment); `PRACTICAL_PIXEL_CAP` is the
    /// Swift build's measured one. Callers may change it (benchmarks do).
    pub max_pixels: usize,
    /// The device limit alone, without the practical cap.
    pub device_max_pixels: usize,
    pool: Mutex<HashMap<u64, Vec<wgpu::Buffer>>>,
    readback_pool: Mutex<HashMap<u64, Vec<wgpu::Buffer>>>,
    profile: bool,
}

/// Measured on the M2 by the Swift build: above this a full-resolution frame is memory
/// bound and the GPU stops winning. Proxies (2560 long edge ≈ 4.4 MP) are well below it.
pub const PRACTICAL_PIXEL_CAP: usize = 16_000_000;

/// Keep at most this much parked between renders (a handful of proxy-sized buffers).
const POOL_BYTE_LIMIT: u64 = 512 * 1024 * 1024;

impl GpuPipeline {
    /// None when no usable adapter exists or a kernel fails to build.
    pub fn new() -> Result<Self, String> {
        pollster::block_on(Self::create())
    }

    async fn create() -> Result<Self, String> {
        // Windows prefers Direct3D 12: on the Intel Iris Xe it measured about 3x faster
        // than Vulkan on the LUT-heavy pixel stage (15 vs 42 ms per proxy). WGPU_BACKEND
        // still overrides; without D3D12 the other backends are tried.
        let from_env = std::env::var_os("WGPU_BACKEND").is_some();
        let mut tries = Vec::new();
        if cfg!(windows) && !from_env {
            tries.push(wgpu::Backends::DX12);
        }
        tries.push(wgpu::InstanceDescriptor::new_without_display_handle_from_env().backends);
        let mut found = None;
        for backends in tries {
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
            desc.backends = backends;
            let instance = wgpu::Instance::new(desc);
            let opts = wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                apply_limit_buckets: false,
            };
            if let Ok(a) = instance.request_adapter(&opts).await {
                found = Some(a);
                break;
            }
        }
        let adapter = found.ok_or_else(|| "找不到 GPU".to_string())?;
        // AWPR_GPU_PROFILE=1: per-kernel GPU times from timestamp queries, printed after
        // each render (diagnostics only).
        let profile = std::env::var_os("AWPR_GPU_PROFILE").is_some()
            && adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("awpr"),
                required_features: if profile { wgpu::Features::TIMESTAMP_QUERY } else { wgpu::Features::empty() },
                // Full-resolution frames need far more than the 128 MiB default binding.
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("無法建立 GPU 裝置：{e}"))?;
        Self::build(&adapter.get_info(), device, queue, profile).await
    }

    /// Use a device someone else created — the editor's window, so a rendered frame can be
    /// drawn straight from its buffer. The device should have been requested with the
    /// adapter's limits (`required_limits: adapter.limits()`), or large images will not fit.
    pub fn with_device(info: &wgpu::AdapterInfo, device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, String> {
        pollster::block_on(Self::build(info, device, queue, false))
    }

    async fn build(info: &wgpu::AdapterInfo, device: wgpu::Device, queue: wgpu::Queue, profile: bool) -> Result<Self, String> {
        let limits = device.limits();
        // Shader compile errors would otherwise only reach the uncaptured-error handler.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let compute = |name: &str, wgsl: String| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(name),
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(name),
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let make = |name: &str, src: &str| compute(name, shaders::module(src));
        let kernels = Kernels {
            pixel: make("pixelStage", shaders::PIXEL),
            pixel_v3: compute("pixelV3", shaders::PIXEL_V3.to_string()),
            masks: make("masks", shaders::MASKS),
            blur_h: make("blurH", shaders::BLUR_H),
            blur_v: make("blurV", shaders::BLUR_V),
            blur_combine: make("blurCombine", shaders::BLUR_COMBINE),
            resample: make("resample", shaders::RESAMPLE),
            rotate: make("rotate90", shaders::ROTATE),
            histogram: compute("histogram", shaders::HISTOGRAM.to_string()),
        };
        if let Some(e) = scope.pop().await {
            return Err(format!("GPU shader 編譯失敗：{e}"));
        }

        let lut = |label: &str, data: &[f32]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(data),
                usage: wgpu::BufferUsages::STORAGE,
            })
        };
        let decode_lut = lut("decodeLut", color::decode_lut());
        let encode_lut = lut("encodeLut", color::encode_lut());

        let binding = limits.max_storage_buffer_binding_size as u64;
        let buffer = limits.max_buffer_size;
        let device_max_pixels = (binding.min(buffer) / 16) as usize;
        let status = format!("{}（{:?}，{:?}）", info.name, info.backend, info.device_type);
        Ok(Self {
            device,
            queue,
            kernels,
            decode_lut,
            encode_lut,
            status,
            max_pixels: device_max_pixels.min(PRACTICAL_PIXEL_CAP),
            device_max_pixels,
            pool: Mutex::new(HashMap::new()),
            readback_pool: Mutex::new(HashMap::new()),
            profile,
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Count a frame's 8-bit RGB histogram on the device. The counts arrive
    /// asynchronously: poll the device (the editor's frame loop does) and take them from
    /// the returned job once ready.
    pub fn histogram(&self, frame: &GpuFrame) -> HistogramJob {
        let bins = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("histogram bins"),
            size: HISTOGRAM_BYTES,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let read = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("histogram readback"),
            size: HISTOGRAM_BYTES,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dims = self.uniform(&[frame.width as u32, frame.height as u32, 0, 0]);
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.kernels.histogram.get_bind_group_layout(0),
            entries: &three(frame.buffer(), &bins, &dims),
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("histogram") });
        enc.clear_buffer(&bins, 0, None);
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
            pass.set_pipeline(&self.kernels.histogram);
            pass.set_bind_group(0, &bind, &[]);
            let cols = (frame.width as u32).div_ceil(shaders::HISTOGRAM_WG);
            let rows = (frame.height as u32).div_ceil(shaders::HISTOGRAM_ROWS);
            pass.dispatch_workgroups(cols, rows, 1);
        }
        enc.copy_buffer_to_buffer(&bins, 0, &read, 0, HISTOGRAM_BYTES);
        self.queue.submit([enc.finish()]);
        let result: std::sync::Arc<Mutex<Option<Vec<u32>>>> = Default::default();
        let slot = result.clone();
        let read2 = read.clone();
        read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            if r.is_ok() {
                if let Ok(view) = read2.slice(..).get_mapped_range() {
                    *slot.lock().unwrap() = Some(bytemuck::cast_slice::<u8, u32>(&view).to_vec());
                }
                read2.unmap();
            }
        });
        HistogramJob { result }
    }

    /// Run whatever callbacks (histogram readbacks) are ready, without blocking.
    pub fn poll(&self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
    }

    pub fn can_host(&self, width: usize, height: usize) -> bool {
        width * height <= self.max_pixels
    }

    /// Run steps 1-10 on the GPU and read the result back (export, diagnostics).
    /// Err → the caller renders on the CPU; `src` is untouched.
    pub fn apply(&self, src: &FloatImage, adj: &ImageAdjustments, ctx: &ProcessContext) -> Result<FloatImage, StageError> {
        let frame = self.upload(src)?;
        let out = self.render(&frame, adj, ctx)?;
        out.download()
    }

    /// Put an image on the device once — the editor does this when a photo opens, and
    /// every slider change after that renders from it without crossing the bus again.
    pub fn upload(&self, src: &FloatImage) -> Result<GpuFrame<'_>, StageError> {
        self.check_size(src.width, src.height)?;
        let buffer = self.borrow(src.width, src.height);
        self.queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&src.data));
        Ok(GpuFrame { gpu: self, buffer: Some(buffer), width: src.width, height: src.height })
    }

    /// Run steps 1-10 from a resident source. The result stays on the device: the editor
    /// draws it from there; `GpuFrame::download` brings it back when pixels are needed.
    /// The work is submitted but not waited on (`GpuFrame::wait` does that).
    pub fn render(&self, src: &GpuFrame, adj: &ImageAdjustments, ctx: &ProcessContext) -> Result<GpuFrame<'_>, StageError> {
        self.check_size(src.width, src.height)?;
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let oom = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let result = pipeline::run_pipeline(GpuTarget::new(self, src), adj, ctx);
        let oom_err = pollster::block_on(oom.pop());
        let val_err = pollster::block_on(scope.pop());
        if let Some(e) = oom_err.or(val_err) {
            return Err(err(format!("GPU 錯誤：{e}")));
        }
        result
    }

    fn check_size(&self, width: usize, height: usize) -> Result<(), StageError> {
        if self.can_host(width, height) {
            Ok(())
        } else {
            Err(err(format!("{width} x {height} 超過 GPU 尺寸上限 {} MP", self.max_pixels / 1_000_000)))
        }
    }

    fn wait(&self) -> Result<(), StageError> {
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map(|_| ())
            .map_err(|e| err(format!("GPU 等待失敗：{e}")))
    }

    /// Submit `encoder` (if any) and read `buffer` back.
    fn read_back(&self, encoder: Option<wgpu::CommandEncoder>, buffer: &wgpu::Buffer, width: usize, height: usize) -> Result<FloatImage, StageError> {
        let size = (width * height * 16) as u64;
        let staging = self.borrow_readback(size);
        let mut enc = encoder
            .unwrap_or_else(|| self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("readback") }));
        enc.copy_buffer_to_buffer(buffer, 0, &staging, 0, size);
        self.queue.submit([enc.finish()]);

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.wait()?;
        rx.recv()
            .map_err(|e| err(e.to_string()))?
            .map_err(|e| err(format!("讀回失敗：{e}")))?;
        let view = slice.get_mapped_range().map_err(|e| err(format!("讀回失敗：{e}")))?;
        let data = bytemuck::cast_slice::<u8, f32>(&view).to_vec();
        drop(view);
        staging.unmap();
        self.give_back_readback(staging);
        Ok(FloatImage { width, height, data })
    }

    // ---- scratch pool ---------------------------------------------------------
    // A full frame is hundreds of MB and a render wants up to three at once. Allocating
    // per stage dominated the Swift build's timings, so buffers are recycled by size.
    // Reuse across renders is safe without waiting: everything goes through one queue,
    // and wgpu orders a later submission's access after an earlier one's.

    fn borrow(&self, width: usize, height: usize) -> wgpu::Buffer {
        let size = (width * height * 16) as u64;
        if let Some(b) = self.pool.lock().unwrap().get_mut(&size).and_then(Vec::pop) {
            return b;
        }
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn give_back(&self, b: wgpu::Buffer) {
        Self::park(&self.pool, b);
    }

    fn borrow_readback(&self, size: u64) -> wgpu::Buffer {
        if let Some(b) = self.readback_pool.lock().unwrap().get_mut(&size).and_then(Vec::pop) {
            return b;
        }
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn give_back_readback(&self, b: wgpu::Buffer) {
        Self::park(&self.readback_pool, b);
    }

    fn park(pool: &Mutex<HashMap<u64, Vec<wgpu::Buffer>>>, b: wgpu::Buffer) {
        let mut pool = pool.lock().unwrap();
        let held: u64 = pool.iter().map(|(len, list)| len * list.len() as u64).sum();
        if held + b.size() <= POOL_BYTE_LIMIT {
            pool.entry(b.size()).or_default().push(b);
        }
    }

    /// Drop everything parked in the pools.
    pub fn flush_pool(&self) {
        self.pool.lock().unwrap().clear();
        self.readback_pool.lock().unwrap().clear();
    }

    fn uniform<T: Pod>(&self, v: &T) -> wgpu::Buffer {
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: bytemuck::bytes_of(v),
            usage: wgpu::BufferUsages::UNIFORM,
        })
    }

    fn storage<T: Pod>(&self, v: &[T]) -> wgpu::Buffer {
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("table"),
            contents: bytemuck::cast_slice(v),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }
}

/// An image on the device. Its buffer goes back to the pool when dropped.
pub struct GpuFrame<'a> {
    gpu: &'a GpuPipeline,
    buffer: Option<wgpu::Buffer>,
    pub width: usize,
    pub height: usize,
}

impl GpuFrame<'_> {
    /// Bring the pixels back to the CPU.
    pub fn download(&self) -> Result<FloatImage, StageError> {
        self.gpu.read_back(None, self.buffer.as_ref().unwrap(), self.width, self.height)
    }

    /// Block until the GPU has finished everything submitted so far (timing).
    pub fn wait(&self) -> Result<(), StageError> {
        self.gpu.wait()
    }

    /// The device buffer (RGBA f32), for drawing it without a readback.
    pub fn buffer(&self) -> &wgpu::Buffer {
        self.buffer.as_ref().unwrap()
    }
}

impl Drop for GpuFrame<'_> {
    fn drop(&mut self) {
        if let Some(b) = self.buffer.take() {
            self.gpu.give_back(b);
        }
    }
}

/// One render on the GPU. All stages are recorded into one command encoder and submitted
/// once — only the heal stage forces a round trip.
pub struct GpuTarget<'a> {
    gpu: &'a GpuPipeline,
    image: Option<wgpu::Buffer>,
    width: usize,
    height: usize,
    encoder: Option<wgpu::CommandEncoder>,
    /// Scratch this render is done with, reusable by its own later stages and returned to
    /// the shared pool when the render ends.
    retired: Vec<wgpu::Buffer>,
    /// AWPR_GPU_PROFILE: a timestamp pair per dispatch, and its label.
    queries: Option<(wgpu::QuerySet, Vec<&'static str>)>,
}

const MAX_PROFILED_PASSES: u32 = 32;

impl<'a> GpuTarget<'a> {
    fn new(gpu: &'a GpuPipeline, src: &GpuFrame) -> Self {
        let queries = gpu.profile.then(|| {
            let set = gpu.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("profile"),
                ty: wgpu::QueryType::Timestamp,
                count: MAX_PROFILED_PASSES * 2,
            });
            (set, Vec::new())
        });
        let mut t = Self { gpu, image: None, width: src.width, height: src.height, encoder: None, retired: Vec::new(), queries };
        // The pipeline must not modify the resident source, so it works on a device-side
        // copy (the CPU path clones for the same reason).
        let image = gpu.borrow(src.width, src.height);
        let size = (src.width * src.height * 16) as u64;
        t.encoder().copy_buffer_to_buffer(src.buffer(), 0, &image, 0, size);
        t.image = Some(image);
        t
    }

    fn image(&self) -> &wgpu::Buffer {
        self.image.as_ref().unwrap()
    }

    fn take(&mut self, width: usize, height: usize) -> wgpu::Buffer {
        let size = (width * height * 16) as u64;
        if let Some(i) = self.retired.iter().position(|b| b.size() == size) {
            return self.retired.swap_remove(i);
        }
        self.gpu.borrow(width, height)
    }

    fn encoder(&mut self) -> &mut wgpu::CommandEncoder {
        let device = &self.gpu.device;
        self.encoder
            .get_or_insert_with(|| device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("render") }))
    }

    /// One dispatch over `width` x `height`, in its own compute pass so the stages that
    /// read what an earlier one wrote are ordered by wgpu's barriers.
    fn dispatch(&mut self, label: &'static str, kernel: &wgpu::ComputePipeline, entries: &[wgpu::BindGroupEntry], width: usize, height: usize) {
        let bind = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &kernel.get_bind_group_layout(0),
            entries,
        });
        let device = &self.gpu.device;
        let enc = self
            .encoder
            .get_or_insert_with(|| device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("render") }));
        let timestamp_writes = match &mut self.queries {
            Some((set, labels)) if (labels.len() as u32) < MAX_PROFILED_PASSES => {
                let i = labels.len() as u32;
                labels.push(label);
                Some(wgpu::ComputePassTimestampWrites {
                    query_set: set,
                    beginning_of_pass_write_index: Some(i * 2),
                    end_of_pass_write_index: Some(i * 2 + 1),
                })
            }
            _ => None,
        };
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes });
        pass.set_pipeline(kernel);
        pass.set_bind_group(0, &bind, &[]);
        let wg = shaders::WORKGROUP as usize;
        pass.dispatch_workgroups(width.div_ceil(wg) as u32, height.div_ceil(wg) as u32, 1);
    }
}

impl GpuTarget<'_> {
    /// Resolve the timestamps, wait for the render and print each kernel's GPU time.
    fn print_profile(&mut self, set: &wgpu::QuerySet, labels: &[&'static str]) -> Result<(), StageError> {
        let gpu = self.gpu;
        let n = labels.len() as u32 * 2;
        let size = n as u64 * 8;
        let resolve = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("resolve"),
            size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("profile readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let enc = self.encoder();
        enc.resolve_query_set(set, 0..n, &resolve, 0);
        enc.copy_buffer_to_buffer(&resolve, 0, &read, 0, size);
        let cmd = self.encoder.take().unwrap().finish();
        gpu.queue.submit([cmd]);
        read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        gpu.wait()?;
        let view = read.slice(..).get_mapped_range().map_err(|e| err(e.to_string()))?;
        let ticks: &[u64] = bytemuck::cast_slice(&view);
        let period = gpu.queue.get_timestamp_period() as f64;
        let ms = |a: u64, b: u64| b.wrapping_sub(a) as f64 * period / 1e6;
        let parts: Vec<String> = labels
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{l} {:.2}", ms(ticks[i * 2], ticks[i * 2 + 1])))
            .collect();
        eprintln!("    [gpu ms] {}  | first→last {:.2}", parts.join("  "), ms(ticks[0], ticks[n as usize - 1]));
        Ok(())
    }
}

/// Bindings 0, 1, 2 for the two-buffers-plus-params kernels.
fn three<'b>(a: &'b wgpu::Buffer, b: &'b wgpu::Buffer, params: &'b wgpu::Buffer) -> [wgpu::BindGroupEntry<'b>; 3] {
    [
        wgpu::BindGroupEntry { binding: 0, resource: a.as_entire_binding() },
        wgpu::BindGroupEntry { binding: 1, resource: b.as_entire_binding() },
        wgpu::BindGroupEntry { binding: 2, resource: params.as_entire_binding() },
    ]
}

impl Drop for GpuTarget<'_> {
    fn drop(&mut self) {
        // Safe to recycle even with work in flight: see the note on the scratch pool.
        if let Some(b) = self.image.take() {
            self.gpu.give_back(b);
        }
        for b in self.retired.drain(..) {
            self.gpu.give_back(b);
        }
    }
}

impl<'a> StageTarget for GpuTarget<'a> {
    type Output = GpuFrame<'a>;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn pixel(&mut self, p: &PixelStageParams) -> Result<(), StageError> {
        let mut flags = 0;
        if p.legacy_wb {
            flags |= FLAG_LEGACY_WB;
        }
        if p.linear_mul {
            flags |= FLAG_LINEAR_MUL;
        }
        if p.linear_matrix {
            flags |= FLAG_LINEAR_MATRIX;
        }
        if p.tone_lut.is_some() {
            flags |= FLAG_TONE_LUT;
        }
        if p.vib_sat {
            flags |= FLAG_VIB_SAT;
        }
        let grads: Vec<GpuGradient> = p
            .gradients
            .iter()
            .filter(|g| g.has_effect())
            .map(|gr| {
                let a = gr.angle * std::f64::consts::PI / 180.0;
                GpuGradient {
                    sin_a: a.sin() as f32,
                    cos_a: a.cos() as f32,
                    center_x: gr.center_x as f32,
                    center_y: gr.center_y as f32,
                    inv2_range: (1.0 / (2.0 * gr.range.max(1e-3))) as f32,
                    exposure: gr.exposure as f32,
                    contrast: (gr.contrast / 100.0) as f32,
                    highlights: (gr.highlights / 100.0) as f32,
                    shadows: (gr.shadows / 100.0) as f32,
                    saturation: (gr.saturation / 100.0) as f32,
                }
            })
            .collect();
        if !grads.is_empty() {
            flags |= FLAG_GRADIENTS;
            if p.gradient_linear {
                flags |= FLAG_GRADIENT_LINEAR;
            }
        }
        if p.vignette {
            flags |= FLAG_VIGNETTE;
        }
        let params = GpuPixelParams {
            flags,
            gradient_count: grads.len() as u32,
            wb: [p.wb_mul.0, p.wb_mul.1, p.wb_mul.2],
            m: p.m.unwrap_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]),
            sat: p.sat,
            vib: p.vib,
            vig_amount: p.vig_amount,
            vig_cx: p.vig_cx,
            vig_cy: p.vig_cy,
            vig_inv_max: p.vig_inv_max,
            width: self.width as u32,
            height: self.height as u32,
            _pad: [0; 2],
        };
        let gpu = self.gpu;
        let uniform = gpu.uniform(&params);
        // A binding cannot be empty: stub tables keep every slot valid.
        let tone = gpu.storage(p.tone_lut.as_deref().unwrap_or(&[0.0; 4]));
        let stub = [GpuGradient::zeroed()];
        let grad_buf = gpu.storage(if grads.is_empty() { &stub[..] } else { &grads[..] });
        let image = self.image().clone();
        let entries = [
            wgpu::BindGroupEntry { binding: 0, resource: image.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: uniform.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: gpu.decode_lut.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: gpu.encode_lut.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 4, resource: tone.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 5, resource: grad_buf.as_entire_binding() },
        ];
        let (w, h) = (self.width, self.height);
        self.dispatch("pixel", &gpu.kernels.pixel, &entries, w, h);
        Ok(())
    }

    fn pixel_v3(&mut self, p: &V3Params) -> Result<(), StageError> {
        let gpu = self.gpu;
        let (w, h) = (self.width, self.height);
        let words = gpu.storage(&p.words(w, h));
        let luts = gpu.storage(&p.luts);
        let image = self.image().clone();
        let entries = [
            wgpu::BindGroupEntry { binding: 0, resource: image.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: words.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: luts.as_entire_binding() },
        ];
        self.dispatch("pixelV3", &gpu.kernels.pixel_v3, &entries, w, h);
        Ok(())
    }

    fn masks(&mut self, p: &MaskParams) -> Result<(), StageError> {
        let gpu = self.gpu;
        let (w, h) = (self.width, self.height);
        if (p.width, p.height) != (w, h) {
            return Err(err("mask size mismatch"));
        }
        let words = gpu.storage(&masks::words(p));
        let weights = gpu.storage(&masks::packed_weights(p));
        let image = self.image().clone();
        let entries = [
            wgpu::BindGroupEntry { binding: 0, resource: image.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: words.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: weights.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: gpu.decode_lut.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 4, resource: gpu.encode_lut.as_entire_binding() },
        ];
        self.dispatch("masks", &gpu.kernels.masks, &entries, w, h);
        Ok(())
    }

    fn blur(&mut self, op: BlurOp) -> Result<(), StageError> {
        let gpu = self.gpu;
        let (w, h) = (self.width, self.height);
        let tmp = self.take(w, h);
        let blurred = self.take(w, h);
        let params = GpuBlurParams {
            width: w as u32,
            height: h as u32,
            radius: op.radius,
            mode: op.mode,
            amount: op.amount,
            _pad: [0; 3],
        };
        let u = gpu.uniform(&params);
        let image = self.image().clone();
        self.dispatch("blurH", &gpu.kernels.blur_h, &three(&image, &tmp, &u), w, h);
        self.dispatch("blurV", &gpu.kernels.blur_v, &three(&tmp, &blurred, &u), w, h);
        self.dispatch("blurCombine", &gpu.kernels.blur_combine, &three(&image, &blurred, &u), w, h);
        self.retired.push(tmp);
        self.retired.push(blurred);
        Ok(())
    }

    fn heal(&mut self, adj: &ImageAdjustments) -> Result<(), StageError> {
        // The CPU is about to read the pixels: flush, fix on the CPU, upload again.
        let enc = self.encoder.take();
        let mut img = self.gpu.read_back(enc, self.image(), self.width, self.height)?;
        pipeline::heal(&mut img, adj);
        self.gpu.queue.write_buffer(self.image(), 0, bytemuck::cast_slice(&img.data));
        Ok(())
    }

    fn resample(&mut self, p: ResampleParams, out_w: usize, out_h: usize) -> Result<(), StageError> {
        let gpu = self.gpu;
        let dst = self.take(out_w, out_h);
        let params = GpuResampleParams {
            src_width: self.width as u32,
            src_height: self.height as u32,
            out_width: out_w as u32,
            out_height: out_h as u32,
            mode: p.mode,
            k: p.k as f32,
            cx: p.cx as f32,
            cy: p.cy as f32,
            sin_a: p.sin_a as f32,
            cos_a: p.cos_a as f32,
            ox: p.ox as f32,
            oy: p.oy as f32,
        };
        let u = gpu.uniform(&params);
        let image = self.image().clone();
        let entries = three(&image, &dst, &u);
        self.dispatch("resample", &gpu.kernels.resample, &entries, out_w, out_h);
        let old = self.image.replace(dst).unwrap();
        self.retired.push(old);
        self.width = out_w;
        self.height = out_h;
        Ok(())
    }

    fn rotate(&mut self, rot: Rotation) -> Result<(), StageError> {
        let deg = match rot {
            Rotation::R0 => return Ok(()),
            Rotation::R90 => 90,
            Rotation::R180 => 180,
            Rotation::R270 => 270,
        };
        let gpu = self.gpu;
        let (w, h) = (self.width, self.height);
        let swap = deg != 180;
        let (out_w, out_h) = if swap { (h, w) } else { (w, h) };
        let dst = self.take(out_w, out_h);
        let u = gpu.uniform(&GpuRotateParams { width: w as u32, height: h as u32, rot: deg, _pad: 0 });
        let image = self.image().clone();
        let entries = three(&image, &dst, &u);
        self.dispatch("rotate90", &gpu.kernels.rotate, &entries, w, h);
        let old = self.image.replace(dst).unwrap();
        self.retired.push(old);
        self.width = out_w;
        self.height = out_h;
        Ok(())
    }

    fn result(mut self) -> Result<GpuFrame<'a>, StageError> {
        if let Some((set, labels)) = self.queries.take() {
            if !labels.is_empty() {
                self.print_profile(&set, &labels)?;
            }
        }
        if let Some(enc) = self.encoder.take() {
            self.gpu.queue.submit([enc.finish()]);
        }
        Ok(GpuFrame { gpu: self.gpu, buffer: self.image.take(), width: self.width, height: self.height })
    }
}
