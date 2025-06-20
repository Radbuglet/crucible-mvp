use std::{borrow::Cow, collections::HashMap, num::NonZeroU32};

use crevice::std430::{AsStd430, Vec2};
use glam::UVec2;
use utils::{arena::Arena, crevice::vertex_attributes};
use wgpu::util::{DeviceExt, StagingBelt};

mod texture;
mod utils;

pub const REQUIRED_FEATURES: wgpu::Features = wgpu::Features::TEXTURE_BINDING_ARRAY;

#[derive(Debug)]
pub struct GfxContext {
    device: wgpu::Device,
    belt: StagingBelt,
    texture_draw_group: wgpu::BindGroupLayout,
    texture_draw_pipeline: wgpu::RenderPipeline,
    textures: Arena<TextureState>,
    commands: Vec<Command>,
}

impl GfxContext {
    pub fn new(device: wgpu::Device) -> Self {
        let texture_draw_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/texture.wgsl"))),
        });

        let texture_draw_group =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: Some(NonZeroU32::new(32).unwrap()),
                }],
            });

        let texture_draw_group_multi =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&texture_draw_group],
                push_constant_ranges: &[],
            });

        let texture_draw_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&texture_draw_group_multi),
                vertex: wgpu::VertexState {
                    module: &texture_draw_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: Instance::std430_size_static() as _,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &vertex_attributes! {
                            <Instance as AsStd430>::Output =>
                            affine_mat_x: wgpu::VertexFormat::Float32x2,
                            affine_mat_y: wgpu::VertexFormat::Float32x2,
                            affine_trans: wgpu::VertexFormat::Float32x2,
                            clip_start: wgpu::VertexFormat::Float32x2,
                            clip_size: wgpu::VertexFormat::Float32x2,
                            tint: wgpu::VertexFormat::Uint32,
                            src_idx: wgpu::VertexFormat::Uint32,
                        },
                    }],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &texture_draw_shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::all(),
                    })],
                }),
                multiview: None,
                cache: None,
            });

        Self {
            device,
            belt: StagingBelt::new(65535),
            texture_draw_group,
            texture_draw_pipeline,
            textures: Arena::new(),
            commands: Vec::new(),
        }
    }

    pub fn dispatch(&mut self, encoder: &mut wgpu::CommandEncoder) {
        for command in self.commands.drain(..) {
            match command {
                Command::UploadTexture {
                    dest,
                    src,
                    src_offset,
                    src_size,
                    src_stride,
                    put_at: dst_put,
                } => {
                    encoder.copy_buffer_to_texture(
                        wgpu::TexelCopyBufferInfo {
                            buffer: &src,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: src_offset,
                                bytes_per_row: Some(src_stride),
                                rows_per_image: None,
                            },
                        },
                        wgpu::TexelCopyTextureInfo {
                            texture: &dest,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: dst_put.x,
                                y: dst_put.y,
                                z: 0,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d {
                            width: src_size.x,
                            height: src_size.y,
                            depth_or_array_layers: 1,
                        },
                    );
                }
                Command::DrawTexture {
                    dest,
                    clear,
                    src_list,
                    src_set: _,
                    instances,
                } => {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("user draw texture pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &dest,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: match clear {
                                    Some(color) => wgpu::LoadOp::Clear(color),
                                    None => wgpu::LoadOp::Load,
                                },
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: None,
                        layout: &self.texture_draw_group,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureViewArray(
                                &src_list.iter().collect::<Vec<_>>(),
                            ),
                        }],
                    });

                    let instance_buf =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: None,
                                contents: bytemuck::cast_slice::<_, u8>(&instances),
                                usage: wgpu::BufferUsages::VERTEX,
                            });

                    pass.set_pipeline(&self.texture_draw_pipeline);
                    pass.set_bind_group(0, &group, &[]);

                    pass.set_vertex_buffer(0, instance_buf.slice(..));

                    pass.draw(0..6, 0..instances.len() as u32);
                }
            }
        }
    }
}

#[derive(Debug)]
struct TextureState {
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    last_draw_command: Option<usize>,
}

#[derive(Debug)]
enum Command {
    UploadTexture {
        dest: wgpu::Texture,
        src: wgpu::Buffer,
        src_offset: u64,
        src_size: UVec2,
        src_stride: u32,
        put_at: UVec2,
    },
    DrawTexture {
        dest: wgpu::TextureView,
        clear: Option<wgpu::Color>,
        src_list: Vec<wgpu::TextureView>,
        src_set: HashMap<wgpu::TextureView, u32>,
        instances: Vec<<Instance as AsStd430>::Output>,
    },
}

#[derive(Debug, Copy, Clone, AsStd430)]
struct Instance {
    pub affine_mat_x: Vec2,
    pub affine_mat_y: Vec2,
    pub affine_trans: Vec2,
    pub clip_start: Vec2,
    pub clip_size: Vec2,
    pub tint: u32,
    pub src_idx: u32,
}
