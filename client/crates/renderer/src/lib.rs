use glam::UVec2;
use utils::arena::Arena;
use wgpu::util::StagingBelt;

mod texture;
mod utils;

#[derive(Debug)]
pub struct GfxContext {
    device: wgpu::Device,
    textures: Arena<wgpu::Texture>,
    belt: StagingBelt,
    commands: Vec<Command>,
}

impl GfxContext {
    pub fn new(device: wgpu::Device) -> Self {
        Self {
            device,
            textures: Arena::new(),
            belt: StagingBelt::new(65535),
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
            }
        }
    }
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
}
