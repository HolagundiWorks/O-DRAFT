//! Presents the shared canvas mesh into the child window.
//!
//! The vertex layout and shader are [`aad_render::mesh`]. This module only uploads them and
//! issues the draw. On Windows the instance prefers Direct3D 12 so geometry hits the GPU path
//! with the lowest compositor latency; overlay uploads are skipped when the mesh fingerprint is
//! unchanged.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use aad_render::mesh::{CANVAS_SHADER, INSTANCE_SIZE, MeshData, RING_INSTANCE_SIZE, VERTEX_SIZE};
use wgpu::util::DeviceExt;

use crate::surface;

const CHUNK: usize = 4_194_300;
const UNIFORM_SIZE: u64 = 48;
/// 4× MSAA softens hatch fills and TrueType glyph edges; stroke quads still use coverage AA.
const SAMPLE_COUNT: u32 = 4;

struct Batch {
    buffer: wgpu::Buffer,
    count: u32,
}

/// GPU buffers for one spatial mesh tile (Phase B dirty upload).
struct TileGpu {
    content_key: u64,
    lines: Vec<Batch>,
    tris: Vec<Batch>,
    quads: Vec<Batch>,
    rings: Vec<Batch>,
    breaks: Vec<[usize; 4]>,
}

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    linear_out: bool,
    line_pipeline: wgpu::RenderPipeline,
    tri_pipeline: wgpu::RenderPipeline,
    quad_pipeline: wgpu::RenderPipeline,
    ring_pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    lines: Vec<Batch>,
    tris: Vec<Batch>,
    quads: Vec<Batch>,
    rings: Vec<Batch>,
    breaks: Vec<[usize; 4]>,
    key: Option<u64>,
    /// Per-tile GPU buffers when presenting the tiled path; empty on the monolithic path.
    tiles: HashMap<u64, TileGpu>,
    clear: wgpu::Color,
    over_lines: Vec<Batch>,
    over_tris: Vec<Batch>,
    over_quads: Vec<Batch>,
    over_rings: Vec<Batch>,
    over_breaks: Vec<[usize; 4]>,
    over_key: Option<u64>,
    /// Multisampled colour target resolved into the swapchain each frame.
    msaa: Option<(wgpu::Texture, wgpu::TextureView)>,
}

impl Gpu {
    pub fn open(hwnd: isize, width: u32, height: u32) -> Result<Self, String> {
        let (instance, surface, adapter) = open_adapter(hwnd)?;
        let _ = instance;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&adapter);
        let mut config = surface.get_default_config(&adapter, width.max(1), height.max(1)).ok_or("the drawing surface is not supported")?;
        if let Some(format) = caps.formats.iter().copied().find(|f| f.is_srgb()) {
            config.format = format;
        }
        // Default: tear-free vsync. AAD_VSYNC=0/off picks AutoNoVsync (Immediate → Mailbox → FIFO)
        // when the crosshair must track a high-rate pointer with lower latency.
        config.present_mode = present_mode_from_env();
        config.desired_maximum_frame_latency = 1;
        let format = config.format;
        surface.configure(&device, &config);
        let linear_out = format.is_srgb();
        let (line_pipeline, tri_pipeline, quad_pipeline, ring_pipeline, uniform, bind_group) = pipelines(&device, format, SAMPLE_COUNT);
        let msaa = if SAMPLE_COUNT > 1 { Some(msaa_target(&device, &config)) } else { None };
        Ok(Gpu {
            surface,
            device,
            queue,
            config,
            linear_out,
            line_pipeline,
            tri_pipeline,
            quad_pipeline,
            ring_pipeline,
            uniform,
            bind_group,
            lines: Vec::new(),
            tris: Vec::new(),
            quads: Vec::new(),
            rings: Vec::new(),
            breaks: Vec::new(),
            key: None,
            tiles: HashMap::new(),
            clear: wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
            over_lines: Vec::new(),
            over_tris: Vec::new(),
            over_quads: Vec::new(),
            over_rings: Vec::new(),
            over_breaks: Vec::new(),
            over_key: None,
            msaa,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        let width = width.max(1);
        let height = height.max(1);
        if self.config.width == width && self.config.height == height {
            return Ok(());
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.msaa = if SAMPLE_COUNT > 1 { Some(msaa_target(&self.device, &self.config)) } else { None };
        Ok(())
    }

    pub fn set_clear(&mut self, rgb: aad_color::Rgb) {
        self.clear = wgpu::Color { r: f64::from(rgb.0) / 255.0, g: f64::from(rgb.1) / 255.0, b: f64::from(rgb.2) / 255.0, a: 1.0 };
    }

    /// Upload overlay vertices when their content fingerprint changes.
    pub fn set_overlay(&mut self, mesh: &MeshData) {
        let key = mesh_fingerprint(mesh);
        if self.over_key == Some(key) {
            return;
        }
        self.over_lines = upload(&self.device, "cad_over_lines", &mesh.lines, VERTEX_SIZE);
        self.over_tris = upload(&self.device, "cad_over_fills", &mesh.tris, VERTEX_SIZE);
        self.over_quads = upload(&self.device, "cad_over_quads", &mesh.quads, INSTANCE_SIZE);
        self.over_rings = upload(&self.device, "cad_over_rings", &mesh.rings, RING_INSTANCE_SIZE);
        self.over_breaks = mesh.breaks.clone();
        self.over_key = Some(key);
    }

    /// Fingerprint used by the host to skip a full present when nothing changed.
    pub fn overlay_key(mesh: &MeshData) -> u64 {
        mesh_fingerprint(mesh)
    }

    pub fn present(&mut self, key: u64, mesh: &MeshData, uniform: &[f32; 12]) -> Result<(), String> {
        // Monolithic path: drop any retained tile buffers.
        self.tiles.clear();
        if self.key != Some(key) {
            self.lines = upload(&self.device, "cad_lines", &mesh.lines, VERTEX_SIZE);
            self.tris = upload(&self.device, "cad_fills", &mesh.tris, VERTEX_SIZE);
            self.quads = upload(&self.device, "cad_quads", &mesh.quads, INSTANCE_SIZE);
            self.rings = upload(&self.device, "cad_rings", &mesh.rings, RING_INSTANCE_SIZE);
            self.breaks = mesh.breaks.clone();
            self.key = Some(key);
        }
        self.present_frame(uniform, |this, pass| {
            this.draw_parts(pass, &this.tris, &this.lines, &this.quads, &this.rings, &this.breaks);
            this.draw_parts(pass, &this.over_tris, &this.over_lines, &this.over_quads, &this.over_rings, &this.over_breaks);
        })
    }

    /// Present spatial mesh tiles, uploading only tiles whose `content_key` changed.
    ///
    /// `tiles` is `(tile_id_bits, content_key, mesh)` in draw order. `scene_key` identifies the
    /// composed scene for paint-stamp skipping; infinite-line hairlines should be folded into
    /// `extra` when needed (appended after tiles).
    pub fn present_tiles(
        &mut self,
        scene_key: u64,
        tiles: &[(u64, u64, &MeshData)],
        extra: Option<&MeshData>,
        uniform: &[f32; 12],
    ) -> Result<(), String> {
        self.key = None;
        self.lines.clear();
        self.tris.clear();
        self.quads.clear();
        self.rings.clear();
        self.breaks.clear();
        if tiles.len() > aad_render::mesh::MAX_MESH_TILES {
            return Err("too many mesh tiles".into());
        }
        let keep: std::collections::HashSet<u64> = tiles.iter().map(|(id, _, _)| *id).collect();
        self.tiles.retain(|id, _| keep.contains(id));
        for &(id, content_key, mesh) in tiles {
            let dirty = self.tiles.get(&id).is_none_or(|t| t.content_key != content_key);
            if dirty {
                self.tiles.insert(
                    id,
                    TileGpu {
                        content_key,
                        lines: upload(&self.device, "cad_tile_lines", &mesh.lines, VERTEX_SIZE),
                        tris: upload(&self.device, "cad_tile_fills", &mesh.tris, VERTEX_SIZE),
                        quads: upload(&self.device, "cad_tile_quads", &mesh.quads, INSTANCE_SIZE),
                        rings: upload(&self.device, "cad_tile_rings", &mesh.rings, RING_INSTANCE_SIZE),
                        breaks: mesh.breaks.clone(),
                    },
                );
            }
        }
        let extra_batches = extra.map(|mesh| {
            (
                upload(&self.device, "cad_extra_lines", &mesh.lines, VERTEX_SIZE),
                upload(&self.device, "cad_extra_fills", &mesh.tris, VERTEX_SIZE),
                upload(&self.device, "cad_extra_quads", &mesh.quads, INSTANCE_SIZE),
                upload(&self.device, "cad_extra_rings", &mesh.rings, RING_INSTANCE_SIZE),
                mesh.breaks.clone(),
            )
        });
        let _ = scene_key;
        let order: Vec<u64> = tiles.iter().map(|(id, _, _)| *id).collect();
        self.present_frame(uniform, |this, pass| {
            for id in &order {
                let Some(tile) = this.tiles.get(id) else { continue };
                this.draw_parts(pass, &tile.tris, &tile.lines, &tile.quads, &tile.rings, &tile.breaks);
            }
            if let Some((lines, tris, quads, rings, breaks)) = extra_batches.as_ref() {
                this.draw_parts(pass, tris, lines, quads, rings, breaks);
            }
            this.draw_parts(pass, &this.over_tris, &this.over_lines, &this.over_quads, &this.over_rings, &this.over_breaks);
        })
    }

    fn present_frame(&mut self, uniform: &[f32; 12], draw: impl FnOnce(&Self, &mut wgpu::RenderPass<'_>)) -> Result<(), String> {
        let mut words = *uniform;
        words[3] = if self.linear_out { 1.0 } else { 0.0 };
        let bytes: Vec<u8> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.queue.write_buffer(&self.uniform, 0, &bytes);
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    other => return Err(format!("drawing surface: {other:?}")),
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            other => return Err(format!("drawing surface: {other:?}")),
        };
        let resolve = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa_view = self.msaa.as_ref().map(|(_, v)| v);
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("cad") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cad"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: msaa_view.unwrap_or(&resolve),
                    resolve_target: msaa_view.map(|_| &resolve),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear),
                        store: if msaa_view.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            draw(self, &mut pass);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        self.queue.present(frame);
        Ok(())
    }

    fn draw_parts(&self, pass: &mut wgpu::RenderPass<'_>, tris: &[Batch], lines: &[Batch], quads: &[Batch], rings: &[Batch], breaks: &[[usize; 4]]) {
        pass.set_bind_group(0, &self.bind_group, &[]);
        let total = |bs: &[Batch]| bs.iter().map(|b| b.count as usize).sum::<usize>();
        let all = [total(tris), total(lines), total(quads), total(rings)];
        let mut from = [0usize; 4];
        for to in breaks.iter().copied().chain(std::iter::once(all)) {
            for (i, (pipeline, batches)) in
                [(&self.tri_pipeline, tris), (&self.line_pipeline, lines), (&self.quad_pipeline, quads), (&self.ring_pipeline, rings)]
                    .into_iter()
                    .enumerate()
            {
                let parts = batch_ranges(batches.iter().map(|b| b.count), from[i]..to[i]);
                if parts.is_empty() {
                    continue;
                }
                pass.set_pipeline(pipeline);
                for (j, range) in parts {
                    let Some(batch) = batches.get(j) else { continue };
                    pass.set_vertex_buffer(0, batch.buffer.slice(..));
                    if i >= 2 { pass.draw(0..6, range) } else { pass.draw(range, 0..1) }
                }
            }
            from = to;
        }
    }
}

fn pipelines(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline, wgpu::RenderPipeline, wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup) {
    let module = device
        .create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("cad_canvas"), source: wgpu::ShaderSource::Wgsl(CANVAS_SHADER.into()) });
    let entry = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    };
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("cad_canvas"), entries: &[entry(0)] });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("cad_canvas"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cad_canvas_view"),
        size: UNIFORM_SIZE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cad_canvas"),
        layout: &bgl,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
    });
    let pipeline = |label: &str, vs: &str, topology: wgpu::PrimitiveTopology, buffer: wgpu::VertexBufferLayout<'static>| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some(vs), buffers: &[Some(buffer)], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { topology, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: sample_count, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    (
        pipeline("cad_lines", "vs_line", wgpu::PrimitiveTopology::LineList, vertex_layout()),
        pipeline("cad_fills", "vs_line", wgpu::PrimitiveTopology::TriangleList, vertex_layout()),
        pipeline("cad_quads", "vs_quad", wgpu::PrimitiveTopology::TriangleList, instance_layout()),
        pipeline("cad_rings", "vs_ring", wgpu::PrimitiveTopology::TriangleList, ring_layout()),
        uniform,
        bind_group,
    )
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Unorm8x4];
    wgpu::VertexBufferLayout { array_stride: VERTEX_SIZE as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS }
}

fn instance_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4, 3 => Float32];
    wgpu::VertexBufferLayout { array_stride: INSTANCE_SIZE as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &ATTRS }
}

fn ring_layout() -> wgpu::VertexBufferLayout<'static> {
    // centre, U, V, (start,sweep,width,pad), colour — see RING_INSTANCE_SIZE.
    const ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4, 4 => Unorm8x4];
    wgpu::VertexBufferLayout { array_stride: RING_INSTANCE_SIZE as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &ATTRS }
}

fn upload(device: &wgpu::Device, label: &str, bytes: &[u8], stride: usize) -> Vec<Batch> {
    bytes
        .chunks(CHUNK.saturating_mul(stride))
        .filter(|c| c.len() >= stride)
        .map(|c| Batch {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: c,
                usage: wgpu::BufferUsages::VERTEX,
            }),
            count: u32::try_from(c.len() / stride).unwrap_or(0),
        })
        .collect()
}

fn batch_ranges(counts: impl IntoIterator<Item = u32>, range: std::ops::Range<usize>) -> Vec<(usize, std::ops::Range<u32>)> {
    let mut out = Vec::new();
    let mut off = 0usize;
    for (j, n) in counts.into_iter().enumerate() {
        let n = n as usize;
        let lo = range.start.max(off);
        let hi = range.end.min(off + n);
        if lo < hi {
            out.push((j, (lo - off) as u32..(hi - off) as u32));
        }
        off += n;
    }
    out
}

fn msaa_target(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cad_msaa"),
        size: wgpu::Extent3d { width: config.width.max(1), height: config.height.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: SAMPLE_COUNT,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn mesh_fingerprint(mesh: &MeshData) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    mesh.lines.len().hash(&mut h);
    mesh.tris.len().hash(&mut h);
    mesh.quads.len().hash(&mut h);
    mesh.rings.len().hash(&mut h);
    mesh.breaks.hash(&mut h);
    h.write(&mesh.lines);
    h.write(&mesh.tris);
    h.write(&mesh.quads);
    h.write(&mesh.rings);
    h.finish()
}

fn present_mode_from_env() -> wgpu::PresentMode {
    present_mode_for(std::env::var("AAD_VSYNC").ok().as_deref())
}

fn present_mode_for(vsync: Option<&str>) -> wgpu::PresentMode {
    match vsync.map(str::trim).map(|v| v.to_ascii_lowercase()).as_deref() {
        Some("0" | "off" | "false" | "no") => wgpu::PresentMode::AutoNoVsync,
        _ => wgpu::PresentMode::AutoVsync,
    }
}

fn open_adapter(hwnd: isize) -> Result<(wgpu::Instance, wgpu::Surface<'static>, wgpu::Adapter), String> {
    // Prefer DX12 on Windows (native hardware path). WGPU_BACKEND overrides; fall back to all
    // primary backends if the preferred set cannot create a surface adapter.
    let preferred = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::DX12);
    let try_backends = [preferred, wgpu::Backends::PRIMARY];
    let mut last = String::from("no GPU adapter");
    for backends in try_backends {
        if backends.is_empty() {
            continue;
        }
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = backends;
        let instance = wgpu::Instance::new(desc);
        let surface = match surface::from_hwnd(&instance, hwnd) {
            Ok(s) => s,
            Err(e) => {
                last = e;
                continue;
            }
        };
        match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        })) {
            Ok(adapter) => return Ok((instance, surface, adapter)),
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    #[test]
    fn present_mode_defaults_to_vsync() {
        assert!(matches!(super::present_mode_for(None), wgpu::PresentMode::AutoVsync));
        assert!(matches!(super::present_mode_for(Some("1")), wgpu::PresentMode::AutoVsync));
        assert!(matches!(super::present_mode_for(Some("off")), wgpu::PresentMode::AutoNoVsync));
    }
}
