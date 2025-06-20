use std::{collections::HashMap, num::NonZeroU64};

use anyhow::Context;
use crevice::std430::AsStd430;
use glam::{Affine2, U8Vec4, UVec2, Vec2};

use crate::{
    Command, GfxContext, Instance,
    utils::{
        align::align_to_pow_2,
        blit::{BlitOptions, blit},
        crevice::vec2_to_crevice,
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

        let texture_view = texture.create_view(&Default::default());

        self.textures.add(crate::TextureState {
            texture,
            texture_view,
            last_draw_command: None,
        })
    }

    pub fn clear_texture_rect(
        &mut self,
        target_id: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    ) -> anyhow::Result<()> {
        todo!()
    }

    pub fn fill_texture_rect(
        &mut self,
        target_id: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        color: U8Vec4,
    ) -> anyhow::Result<()> {
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
        let target = self.textures.get_mut(target_id)?;

        // Allocate a staging buffer for the upload.
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

        // Enqueue the command.
        self.commands.push(Command::UploadTexture {
            dest: target.texture.clone(),
            src: staging_alloc.buffer().clone(),
            src_offset: staging_alloc.offset(),
            src_size,
            src_stride: staging_size.x,
            put_at,
        });

        target.last_draw_command = None;

        Ok(())
    }

    pub fn draw_texture(
        &mut self,
        target_id: u32,
        src_id: u32,
        transform: Affine2,
        clip: (Vec2, Vec2),
        tint: U8Vec4,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(src_id != target_id);

        let target = self.textures.get_mut(target_id)?;

        let cmd_idx = *target.last_draw_command.get_or_insert_with(|| {
            let idx = self.commands.len();

            self.commands.push(Command::DrawTexture {
                dest: target.texture_view.clone(),
                clear: None,
                src_list: Vec::new(),
                src_set: HashMap::default(),
                instances: Vec::new(),
            });

            idx
        });

        let src = self.textures.get_mut(target_id)?;

        src.last_draw_command = None;

        let Command::DrawTexture {
            instances,
            src_list,
            src_set,
            ..
        } = &mut self.commands[cmd_idx]
        else {
            unreachable!()
        };

        let src_idx = *src_set.entry(src.texture_view.clone()).or_insert_with(|| {
            let idx = src_list.len() as u32;
            src_list.push(src.texture_view.clone());
            idx
        });

        instances.push(
            Instance {
                affine_mat_x: vec2_to_crevice(transform.matrix2.x_axis),
                affine_mat_y: vec2_to_crevice(transform.matrix2.y_axis),
                affine_trans: vec2_to_crevice(transform.translation),
                clip_start: vec2_to_crevice(clip.0),
                clip_size: vec2_to_crevice(clip.1),
                tint: u32::from_le_bytes(tint.to_array()),
                src_idx,
            }
            .as_std430(),
        );

        Ok(())
    }

    pub fn destroy_texture(&mut self, handle: u32) -> anyhow::Result<()> {
        todo!()
    }
}
