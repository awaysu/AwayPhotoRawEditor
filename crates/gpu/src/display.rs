//! Drawing a rendered frame on screen straight from its storage buffer — the editor's
//! viewer. Lives here (not in the app) so `awpr viewtest` can run the exact same shader
//! into an offscreen texture and compare it with a CPU reference on every backend.

use bytemuck::{Pod, Zeroable};

pub const DISPLAY_WGSL: &str = r#"
struct View {
    img_size: vec2<f32>,
    origin: vec2<f32>,   // the image's top-left, absolute framebuffer pixels
    bg: vec4<f32>,
    scale: f32,          // framebuffer pixels per image pixel
    srgb_target: u32,
    _pad: vec2<u32>,
};
@group(0) @binding(0) var<storage, read> img: array<vec4<f32>>;
@group(0) @binding(1) var<uniform> view: View;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

fn fetch(q: vec2<f32>) -> vec3<f32> {
    let w = i32(view.img_size.x);
    let h = i32(view.img_size.y);
    let x = clamp(i32(floor(q.x)), 0, w - 1);
    let y = clamp(i32(floor(q.y)), 0, h - 1);
    return img[u32(y * w + x)].rgb;
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // pos.xy is the pixel centre; its footprint starts half a pixel up-left.
    let q = (pos.xy - view.origin) / view.scale;
    var c: vec3<f32>;
    if (q.x < 0.0 || q.y < 0.0 || q.x >= view.img_size.x || q.y >= view.img_size.y) {
        c = view.bg.rgb;
    } else if (view.scale >= 1.0) {
        // 100 % and up: every image pixel exact (no smoothing when judging sharpness).
        c = fetch(q);
    } else {
        // Shrinking: average the screen pixel's footprint (a box filter, no aliasing).
        let f = 1.0 / view.scale;
        let n = min(i32(ceil(f)), 8);
        let step = f / f32(n);
        let base = (pos.xy - vec2<f32>(0.5) - view.origin) / view.scale;
        var sum = vec3<f32>(0.0);
        for (var j = 0; j < n; j++) {
            for (var i = 0; i < n; i++) {
                sum += fetch(base + (vec2<f32>(f32(i), f32(j)) + 0.5) * step);
            }
        }
        c = sum / f32(n * n);
    }
    c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    // The pipeline's output is display-encoded already; an sRGB target would encode it
    // a second time, so undo that.
    if (view.srgb_target == 1u) {
        c = to_linear(c);
    }
    return vec4<f32>(c, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ViewUniform {
    pub img_size: [f32; 2],
    pub origin: [f32; 2],
    pub bg: [f32; 4],
    pub scale: f32,
    pub srgb_target: u32,
    pub _pad: [u32; 2],
}

impl ViewUniform {
    /// Image of `w`×`h` with its top-left at `origin` (framebuffer pixels), `scale`
    /// framebuffer pixels per image pixel, on a display-encoded background.
    pub fn new(w: usize, h: usize, origin: [f32; 2], scale: f32, bg: [f32; 3], srgb_target: bool) -> Self {
        Self {
            img_size: [w as f32, h as f32],
            origin,
            bg: [bg[0], bg[1], bg[2], 1.0],
            scale,
            srgb_target: srgb_target as u32,
            _pad: [0; 2],
        }
    }
}

/// The viewer's render pipeline for one target format.
pub struct DisplayPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub layout: wgpu::BindGroupLayout,
    pub uniform: wgpu::Buffer,
    pub srgb_target: bool,
}

impl DisplayPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewer"),
            source: wgpu::ShaderSource::Wgsl(DISPLAY_WGSL.into()),
        });
        let buffer_entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewer"),
            entries: &[
                buffer_entry(0, wgpu::BufferBindingType::Storage { read_only: true }),
                buffer_entry(1, wgpu::BufferBindingType::Uniform),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewer"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewer"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewer uniform"),
            size: std::mem::size_of::<ViewUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, layout, uniform, srgb_target: format.is_srgb() }
    }

    /// A bind group drawing `buffer` (an RGBA f32 frame).
    pub fn bind(&self, device: &wgpu::Device, buffer: &wgpu::Buffer) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewer frame"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.uniform.as_entire_binding() },
            ],
        })
    }

    pub fn write_uniform(&self, queue: &wgpu::Queue, mut u: ViewUniform) {
        u.srgb_target = self.srgb_target as u32;
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, bind: &wgpu::BindGroup) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, Some(bind), &[]);
        pass.draw(0..3, 0..1);
    }
}

impl crate::GpuPipeline {
    /// Draw `frame` through the viewer shader into an offscreen `out_w`×`out_h` texture of
    /// `format` and read its pixels back, rows tightly packed (`awpr viewtest`). Formats:
    /// Rgba8Unorm / Rgba8UnormSrgb (4 bytes a pixel) or Rgba32Float (16).
    pub fn display_readback(
        &self,
        frame: &crate::GpuFrame,
        out_w: u32,
        out_h: u32,
        view: ViewUniform,
        format: wgpu::TextureFormat,
    ) -> Result<Vec<u8>, String> {
        let device = self.device();
        let dp = DisplayPipeline::new(device, format);
        dp.write_uniform(self.queue(), view);
        let bind = dp.bind(device, frame.buffer());
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewtest"),
            size: wgpu::Extent3d { width: out_w, height: out_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let tv = tex.create_view(&Default::default());
        let bpp: u32 = if format == wgpu::TextureFormat::Rgba32Float { 16 } else { 4 };
        let row = (out_w * bpp).div_ceil(256) * 256;
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewtest readback"),
            size: (row * out_h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewtest"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &tv,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            dp.draw(&mut pass, &bind);
        }
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(out_h) },
            },
            wgpu::Extent3d { width: out_w, height: out_h, depth_or_array_layers: 1 },
        );
        self.queue().submit([enc.finish()]);
        read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
        let view = read.slice(..).get_mapped_range().map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity((out_w * out_h * bpp) as usize);
        for y in 0..out_h as usize {
            let start = y * row as usize;
            out.extend_from_slice(&view[start..start + (out_w * bpp) as usize]);
        }
        Ok(out)
    }
}
