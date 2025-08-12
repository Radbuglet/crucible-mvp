use std::{
    borrow::Cow,
    collections::HashMap,
    num::{NonZeroU32, NonZeroU64},
};

use anyhow::Context;
use crevice::std430::AsStd430;
use glam::{Affine2, U8Vec4, UVec2};

use crate::{
    Command, Instance, Renderer, TEXTURE_FORMAT,
    utils::{
        align::align_to_pow_2,
        blit::{BlitOptions, blit},
        crevice::{uvec2_to_crevice, vec2_to_crevice, vertex_attributes},
    },
};

// === TextureAssets === //

#[derive(Debug)]
pub struct TextureAssets {
    pub group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
}

impl TextureAssets {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/texture.wgsl"))),
        });

        let group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: Some(const { NonZeroU32::new(32).unwrap() }),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
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
                        clip_start: wgpu::VertexFormat::Uint32x2,
                        clip_size: wgpu::VertexFormat::Uint32x2,
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
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TEXTURE_FORMAT,
                    blend: Some({
                        let comp = wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        };

                        wgpu::BlendState {
                            color: comp,
                            alpha: comp,
                        }
                    }),
                    write_mask: wgpu::ColorWrites::all(),
                })],
            }),
            multiview: None,
            cache: None,
        });

        Self {
            group_layout,
            pipeline,
        }
    }
}

// === GfxContext === //

impl Renderer {
    pub fn create_texture(&mut self, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("user texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }

    pub fn upload_texture(
        &mut self,
        target: &wgpu::Texture,
        src_data: &[[u8; 4]],
        src_size: UVec2,
        put_at: UVec2,
        clip: Option<(UVec2, UVec2)>,
    ) -> anyhow::Result<()> {
        // Allocate a staging buffer for the upload.
        // WGPU Texture copies require a 256x256 byte-aligned size.
        let staging_size = UVec2::new(
            align_to_pow_2(src_size.x, 256 / 4).context("failed to align buffer width")?,
            align_to_pow_2(src_size.y, 256 / 4).context("failed to align buffer height")?,
        );

        let staging_len = staging_size
            .x
            .checked_mul(staging_size.y)
            .and_then(|v| v.checked_mul(4))
            .context("staging buffer is too big")?;

        let Some(staging_len) = NonZeroU64::new(staging_len.into()) else {
            // (nothing to write)
            return Ok(());
        };

        let staging_alloc = self.belt.allocate(
            staging_len,
            const { NonZeroU64::new(wgpu::COPY_BUFFER_ALIGNMENT).unwrap() },
            &self.device,
        );

        // Blit into it.
        let (clip_pos, clip_size) = clip.unwrap_or((UVec2::ZERO, src_size));

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
            dest: target.clone(),
            src: staging_alloc.buffer().clone(),
            src_offset: staging_alloc.offset(),
            src_size,
            src_stride: staging_size.x * 4,
            put_at,
        });

        self.last_texture_bindings.remove(target);

        Ok(())
    }

    fn get_texture_draw_pass(&mut self, texture: &wgpu::Texture) -> usize {
        *self
            .last_texture_bindings
            .entry(texture.clone())
            .or_insert_with(|| {
                let idx = self.commands.len();

                self.commands.push(Command::DrawTexture {
                    dest: self.texture_views.get(texture),
                    clear: None,
                    src_list: Vec::new(),
                    src_set: HashMap::default(),
                    instances: Vec::new(),
                });

                idx
            })
    }

    pub fn clear_texture(&mut self, target: &wgpu::Texture, color: wgpu::Color) {
        let cmd_idx = self.get_texture_draw_pass(target);

        let Command::DrawTexture {
            instances,
            src_list,
            src_set,
            clear,
            ..
        } = &mut self.commands[cmd_idx]
        else {
            unreachable!()
        };

        *clear = Some(color);
        instances.clear();
        src_list.clear();
        src_set.clear();
    }

    pub fn draw_texture(
        &mut self,
        target: &wgpu::Texture,
        src: Option<&wgpu::Texture>,
        transform: Affine2,
        clip: Option<(UVec2, UVec2)>,
        tint: U8Vec4,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(Some(target) != src);

        let cmd_idx = self.get_texture_draw_pass(target);

        if let Some(src) = src {
            self.last_texture_bindings.remove(src);
        }

        let Command::DrawTexture {
            instances,
            src_list,
            src_set,
            ..
        } = &mut self.commands[cmd_idx]
        else {
            unreachable!()
        };

        let src_idx = src.map_or(u32::MAX, |src| {
            let src_view = self.texture_views.get(src);
            *src_set.entry(src_view.clone()).or_insert_with(|| {
                let idx = src_list.len() as u32;
                src_list.push(src_view);
                idx
            })
        });

        let clip = clip.unwrap_or_else(|| {
            (
                UVec2::ZERO,
                src.as_ref()
                    .map_or(UVec2::ONE, |v| UVec2::new(v.width(), v.height())),
            )
        });

        instances.push(
            Instance {
                affine_mat_x: vec2_to_crevice(transform.matrix2.x_axis),
                affine_mat_y: vec2_to_crevice(transform.matrix2.y_axis),
                affine_trans: vec2_to_crevice(transform.translation),
                clip_start: uvec2_to_crevice(clip.0),
                clip_size: uvec2_to_crevice(clip.1),
                tint: u32::from_le_bytes(tint.to_array()),
                src_idx,
            }
            .as_std430(),
        );

        Ok(())
    }
}
