use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::scene::{DecodedImage, DrawCommand, Frame, ImageCommand};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ImageUniform {
    position: [f32; 2],
    size: [f32; 2],
}

const IMAGE_VERTICES: &[[f32; 2]] = &[
    [0.0, 0.0], [1.0, 0.0], [0.0, 1.0],
    [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
];

const IMAGE_SHADER: &str = r#"
struct Globals {
    viewport_size: vec2<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct ImageUniform {
    position: vec2<f32>,
    size: vec2<f32>,
};
@group(1) @binding(0) var<uniform> image_uniform: ImageUniform;
@group(1) @binding(1) var image_texture: texture_2d<f32>;
@group(1) @binding(2) var image_sampler: sampler;

struct VertexInput {
    @location(0) corner: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let pixel_pos = image_uniform.position + vertex.corner * image_uniform.size;
    let ndc_x = (pixel_pos.x / globals.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / globals.viewport_size.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = vertex.corner;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(image_texture, image_sampler, in.uv);
}
"#;

/// Extracts and scales the `Image` commands from `frame`, in order,
/// ignoring `Rect`/`Text` — pure, no GPU needed, mirrors
/// `quad::build_quad_instances`.
pub fn build_image_instances(frame: &Frame, scale_factor: f32) -> Vec<ImageCommand> {
    frame
        .commands
        .iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::Image(img) => Some(img.scaled(scale_factor)),
            DrawCommand::Rect(_) | DrawCommand::Text(_) => None,
        })
        .collect()
}

struct CachedImage {
    position_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub struct ImagePipeline {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    cache: std::collections::HashMap<usize, CachedImage>,
    draw_order: Vec<usize>,
}

impl ImagePipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image_shader"),
            source: wgpu::ShaderSource::Wgsl(IMAGE_SHADER.into()),
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("image_vertex_buffer"),
            contents: bytemuck::cast_slice(IMAGE_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image_globals_buffer"),
            size: std::mem::size_of::<[f32; 2]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image_globals_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image_globals_bind_group"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buffer.as_entire_binding() }],
        });

        let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image_texture_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image_pipeline_layout"),
            bind_group_layouts: &[&globals_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 }],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            vertex_buffer,
            globals_buffer,
            globals_bind_group,
            texture_bind_group_layout,
            sampler,
            cache: std::collections::HashMap::new(),
            draw_order: Vec::new(),
        }
    }

    /// Uploads a GPU texture for `image` the first time this exact `Arc`
    /// pointer is seen; later calls with the same pointer (same decoded
    /// image, still alive in the current page's `Frame`) reuse it. Returns
    /// the cache key so `prepare` can update just the position uniform.
    fn get_or_create(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, image: &DecodedImage) -> usize {
        let key = std::sync::Arc::as_ptr(&image.rgba) as *const u8 as usize;
        if !self.cache.contains_key(&key) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("image_texture"),
                size: wgpu::Extent3d { width: image.width, height: image.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::ImageCopyTexture { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &image.rgba,
                wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * image.width), rows_per_image: Some(image.height) },
                wgpu::Extent3d { width: image.width, height: image.height, depth_or_array_layers: 1 },
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            let position_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("image_position_buffer"),
                size: std::mem::size_of::<ImageUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("image_texture_bind_group"),
                layout: &self.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: position_buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });

            self.cache.insert(key, CachedImage { position_buffer, bind_group });
        }
        key
    }

    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &Frame, scale_factor: f32, viewport_width: f32, viewport_height: f32) {
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::cast_slice(&[viewport_width, viewport_height]));

        self.draw_order.clear();
        for scaled in build_image_instances(frame, scale_factor) {
            let key = self.get_or_create(device, queue, &scaled.image);
            let cached = self.cache.get(&key).expect("just inserted or already present");
            queue.write_buffer(&cached.position_buffer, 0, bytemuck::cast_slice(&[scaled.x, scaled.y, scaled.width, scaled.height]));
            self.draw_order.push(key);
        }
    }

    pub fn render<'pass>(&'pass self, render_pass: &mut wgpu::RenderPass<'pass>) {
        if self.draw_order.is_empty() {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.globals_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        for key in &self.draw_order {
            let cached = self.cache.get(key).expect("draw_order only contains keys inserted this frame");
            render_pass.set_bind_group(1, &cached.bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{RectCommand, TextCommand};

    fn tiny_image() -> DecodedImage {
        DecodedImage { width: 1, height: 1, rgba: std::sync::Arc::from(vec![1u8, 2, 3, 4]) }
    }

    #[test]
    fn extracts_only_image_commands_in_order() {
        let mut frame = Frame::new();
        frame.push(DrawCommand::Rect(RectCommand { x: 0.0, y: 0.0, width: 1.0, height: 1.0, color: [0.0; 4], corner_radius: 0.0 }));
        frame.push(DrawCommand::Image(ImageCommand { x: 1.0, y: 2.0, width: 10.0, height: 20.0, image: tiny_image() }));
        frame.push(DrawCommand::Text(TextCommand { x: 0.0, y: 0.0, content: "ignored".into(), size: 12.0, color: [0.0; 4] }));
        frame.push(DrawCommand::Image(ImageCommand { x: 5.0, y: 6.0, width: 30.0, height: 40.0, image: tiny_image() }));

        let instances = build_image_instances(&frame, 1.0);
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0].x, 1.0);
        assert_eq!(instances[0].y, 2.0);
        assert_eq!(instances[1].x, 5.0);
    }

    #[test]
    fn empty_frame_yields_no_image_instances() {
        let frame = Frame::new();
        assert!(build_image_instances(&frame, 1.0).is_empty());
    }

    #[test]
    fn applies_scale_factor_to_position_and_size_not_pixel_dimensions() {
        let mut frame = Frame::new();
        frame.push(DrawCommand::Image(ImageCommand { x: 10.0, y: 20.0, width: 30.0, height: 40.0, image: tiny_image() }));

        let instances = build_image_instances(&frame, 2.0);
        assert_eq!(instances[0].x, 20.0);
        assert_eq!(instances[0].y, 40.0);
        assert_eq!(instances[0].width, 60.0);
        assert_eq!(instances[0].height, 80.0);
        assert_eq!(instances[0].image.width, 1, "pixel dimensions of the decoded image are never scaled");
    }
}
