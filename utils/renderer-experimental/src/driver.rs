use crate::{
    assets::{DEPTH_FORMAT, GBUF_TEXTURES_FORMAT},
    utils::{AssetLoader, AssetManager, AssetRetainer, ensure_texture_sized},
};

#[derive(Debug)]
pub struct VoxelRenderer {
    device: wgpu::Device,
    assets: AssetRetainer,

    /// The G-buffer holding surface indices and their ST values.
    ///
    /// - R and G channel holds material index.
    /// - B and A channel hold S and T respectively.
    ///
    attachment_gbuf_textures: Option<wgpu::Texture>,

    /// The depth texture.
    attachment_depth: Option<wgpu::Texture>,
}

impl VoxelRenderer {
    pub fn new(device: wgpu::Device, assets: AssetManager) -> Self {
        Self {
            device,
            assets: AssetRetainer::new(assets),
            attachment_gbuf_textures: None,
            attachment_depth: None,
        }
    }

    pub fn submit(&mut self, encoder: &mut wgpu::CommandEncoder, output_view: &wgpu::TextureView) {
        let output = output_view.texture();

        let attachment_gbuf_textures = ensure_texture_sized(
            &self.device,
            &mut self.attachment_gbuf_textures,
            &wgpu::TextureDescriptor {
                label: Some("textures G-buf"),
                size: output.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: GBUF_TEXTURES_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
        );

        let attachment_depth = ensure_texture_sized(
            &self.device,
            &mut self.attachment_depth,
            &wgpu::TextureDescriptor {
                label: Some("voxel depth"),
                size: output.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
        );

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("G-buf pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &attachment_gbuf_textures
                    .create_view(&wgpu::TextureViewDescriptor::default()),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &attachment_depth.create_view(&wgpu::TextureViewDescriptor::default()),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        drop(pass);

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shade pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        drop(pass);

        self.assets.reap();
    }
}
