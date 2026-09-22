use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::scene::{DrawCommand, Frame};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct QuadInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub corner_radius: f32,
}

pub fn build_quad_instances(frame: &Frame, scale_factor: f32) -> Vec<QuadInstance> {
    frame
        .commands
        .iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::Rect(r) => {
                let scaled = r.scaled(scale_factor);
                Some(QuadInstance {
                    position: [scaled.x, scaled.y],
                    size: [scaled.width, scaled.height],
                    color: scaled.color,
                    corner_radius: scaled.corner_radius,
                })
            }
            DrawCommand::Text(_) => None,
        })
        .collect()
}

const QUAD_VERTICES: &[[f32; 2]] = &[
    [0.0, 0.0], [1.0, 0.0], [0.0, 1.0],
    [1.0, 0.0], [1.0, 1.0], [0.0, 1.0],
];

const QUAD_SHADER: &str = r#"
struct Globals {
    viewport_size: vec2<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct VertexInput {
    @location(0) corner: vec2<f32>,
};

struct InstanceInput {
    @location(1) position: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) corner_radius: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) half_size: vec2<f32>,
    @location(3) corner_radius: f32,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let pixel_pos = instance.position + vertex.corner * instance.size;
    let ndc_x = (pixel_pos.x / globals.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / globals.viewport_size.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = instance.color;
    out.half_size = instance.size * 0.5;
    out.local_pos = (vertex.corner - vec2<f32>(0.5, 0.5)) * instance.size;
    out.corner_radius = instance.corner_radius;
    return out;
}

fn rounded_rect_sdf(local_pos: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(local_pos) - half_size + vec2<f32>(radius, radius);
    return length(max(q, vec2<f32>(0.0, 0.0))) - radius;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = rounded_rect_sdf(in.local_pos, in.half_size, in.corner_radius);
    let alpha = 1.0 - smoothstep(-1.0, 1.0, dist);
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

pub struct QuadPipeline {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
}

impl QuadPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad_shader"),
            source: wgpu::ShaderSource::Wgsl(QUAD_SHADER.into()),
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad_vertex_buffer"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let instance_capacity = 256usize;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad_instance_buffer"),
            size: (instance_capacity * std::mem::size_of::<QuadInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad_globals_buffer"),
            size: std::mem::size_of::<[f32; 2]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quad_globals_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quad_globals_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        };

        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { offset: 0, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 8, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
                wgpu::VertexAttribute { offset: 16, shader_location: 3, format: wgpu::VertexFormat::Float32x4 },
                wgpu::VertexAttribute { offset: 32, shader_location: 4, format: wgpu::VertexFormat::Float32 },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_layout, instance_layout],
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

        Self { pipeline, vertex_buffer, instance_buffer, instance_capacity, globals_buffer, globals_bind_group }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[QuadInstance],
        viewport_width: f32,
        viewport_height: f32,
    ) {
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::cast_slice(&[viewport_width, viewport_height]));

        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("quad_instance_buffer"),
                size: (self.instance_capacity * std::mem::size_of::<QuadInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        if !instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }
    }

    pub fn render<'pass>(&'pass self, render_pass: &mut wgpu::RenderPass<'pass>, instance_count: u32) {
        if instance_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.globals_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        render_pass.draw(0..6, 0..instance_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{RectCommand, TextCommand};

    #[test]
    fn extracts_only_rect_commands_in_order() {
        let mut frame = Frame::new();
        frame.push(DrawCommand::Rect(RectCommand {
            x: 1.0, y: 2.0, width: 10.0, height: 20.0, color: [1.0, 0.0, 0.0, 1.0], corner_radius: 4.0,
        }));
        frame.push(DrawCommand::Text(TextCommand {
            x: 0.0, y: 0.0, content: "ignored".into(), size: 12.0, color: [1.0, 1.0, 1.0, 1.0],
        }));
        frame.push(DrawCommand::Rect(RectCommand {
            x: 5.0, y: 6.0, width: 30.0, height: 40.0, color: [0.0, 1.0, 0.0, 1.0], corner_radius: 0.0,
        }));

        let instances = build_quad_instances(&frame, 1.0);
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0].position, [1.0, 2.0]);
        assert_eq!(instances[0].size, [10.0, 20.0]);
        assert_eq!(instances[0].corner_radius, 4.0);
        assert_eq!(instances[1].position, [5.0, 6.0]);
    }

    #[test]
    fn empty_frame_yields_no_instances() {
        let frame = Frame::new();
        assert!(build_quad_instances(&frame, 1.0).is_empty());
    }

    #[test]
    fn applies_scale_factor_to_position_size_and_radius() {
        let mut frame = Frame::new();
        frame.push(DrawCommand::Rect(RectCommand {
            x: 10.0, y: 20.0, width: 30.0, height: 40.0, color: [1.0, 0.0, 0.0, 1.0], corner_radius: 5.0,
        }));

        let instances = build_quad_instances(&frame, 2.0);
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].position, [20.0, 40.0]);
        assert_eq!(instances[0].size, [60.0, 80.0]);
        assert_eq!(instances[0].corner_radius, 10.0);
    }
}
