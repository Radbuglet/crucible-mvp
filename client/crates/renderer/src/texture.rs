use std::num::NonZeroU64;

use anyhow::Context;
use glam::UVec2;

use crate::{
    Command, GfxContext,
    utils::{
        align::align_to_pow_2,
        blit::{BlitOptions, blit},
    },
};

impl GfxContext {
    pub fn create_texture(&mut self, width: u32, height: u32) -> anyhow::Result<u32> {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("user texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        self.textures.add(texture)
    }

    pub fn clear_texture_rect(&mut self, target_id: u32, x: u32, y: u32, w: u32, h: u32) {
        todo!()
    }

    pub fn fill_texture_rect(
        &mut self,
        target_id: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        color: [u8; 4],
    ) {
        todo!()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upload_texture(
        &mut self,
        target_id: u32,
        src_data: &[[u8; 4]],
        src_size: UVec2,
        put_at: UVec2,
        clip: Option<(UVec2, UVec2)>,
    ) -> anyhow::Result<()> {
        let target = self.textures.get(target_id)?;

        // Determine the size of the staging buffer.
        // WGPU Texture copies require a 256x256 byte-aligned size.
        let staging_size = UVec2::new(
            align_to_pow_2(src_size.x, 256 / 4).context("failed to align buffer width")?,
            align_to_pow_2(src_size.y, 256 / 4).context("failed to align buffer height")?,
        );

        let staging_len = staging_size
            .x
            .checked_mul(staging_size.y)
            .context("staging buffer is too big")?;

        let Some(size) = NonZeroU64::new(staging_len.into()) else {
            // (nothing to write)
            return Ok(());
        };

        // Allocate our staging buffer.
        let staging_alloc = self.belt.allocate(
            size,
            const { NonZeroU64::new(wgpu::COPY_BUFFER_ALIGNMENT).unwrap() },
            &self.device,
        );

        // Blit into it.
        let (clip_pos, clip_size) = clip.unwrap_or((UVec2::ZERO, staging_size));

        blit(
            src_data,
            bytemuck::cast_slice_mut(&mut staging_alloc.get_mapped_range_mut()),
            BlitOptions {
                src_real_size: src_size.as_usizevec2(),
                dest_real_size: staging_size.as_usizevec2(),
                src_crop_start: clip_pos.as_usizevec2(),
                dest_put_start: put_at.as_usizevec2(),
                crop_size: clip_size.as_usizevec2(),
            },
        )?;

        self.commands.push(Command::UploadTexture {
            dest: target.clone(),
            src: staging_alloc.buffer().clone(),
            src_offset: staging_alloc.offset(),
            src_size,
            src_stride: staging_size.x,
            put_at,
        });

        Ok(())
    }

    pub fn draw_texture(
        &mut self,
        target_id: u32,
        src_id: u32,
        transform: [f32; 6],
        clip: [u32; 4],
        opacity: f32,
    ) {
        todo!()
    }

    pub fn destroy_texture(&mut self, handle: u32) {
        todo!()
    }
}
