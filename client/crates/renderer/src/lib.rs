use std::{collections::HashMap, iter};

use crevice::std430::{self, AsStd430};
use glam::UVec2;
use texture::TextureAssets;
use wgpu::util::{DeviceExt, StagingBelt};

mod texture;
mod utils;

pub const REQUIRED_FEATURES: wgpu::Features = wgpu::Features::TEXTURE_BINDING_ARRAY;
pub const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

#[derive(Debug)]
pub struct GfxContext {
    device: wgpu::Device,
    texture_gfx: TextureAssets,
    belt: StagingBelt,
    textures: HashMap<wgpu::Texture, TextureState>,
    commands: Vec<Command>,
}

impl GfxContext {
    pub fn new(device: wgpu::Device) -> Self {
        let texture_gfx = TextureAssets::new(&device);

        Self {
            device,
            texture_gfx,
            belt: StagingBelt::new(65535),
            textures: HashMap::default(),
            commands: Vec::new(),
        }
    }

    pub fn submit(&mut self, queue: &wgpu::Queue) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("user draw encoder"),
            });

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
                        layout: &self.texture_gfx.group_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureViewArray(
                                &src_list
                                    .iter()
                                    .chain(iter::repeat(src_list.last().unwrap()))
                                    .take(32)
                                    .collect::<Vec<_>>(),
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

                    pass.set_pipeline(&self.texture_gfx.pipeline);
                    pass.set_bind_group(0, &group, &[]);
                    pass.set_vertex_buffer(0, instance_buf.slice(..));
                    pass.draw(0..6, 0..instances.len() as u32);
                }
            }
        }

        self.belt.finish();
        queue.submit([encoder.finish()]);
        self.belt.recall();
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
    pub affine_mat_x: std430::Vec2,
    pub affine_mat_y: std430::Vec2,
    pub affine_trans: std430::Vec2,
    pub clip_start: std430::UVec2,
    pub clip_size: std430::UVec2,
    pub tint: u32,
    pub src_idx: u32,
}
