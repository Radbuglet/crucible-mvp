use std::borrow::Cow;

use crate::{
    structures::QuadInstancePacked,
    utils::{Asset, AssetLoader, ListKey, RefKey},
};

pub const GBUF_TEXTURES_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub fn load_pipeline_layout(
    assets: &mut impl AssetLoader,
    device: &wgpu::Device,
    groups: &[&wgpu::BindGroupLayout],
    push_constants: &[wgpu::PushConstantRange],
) -> Asset<wgpu::PipelineLayout> {
    assets.load(
        device,
        (ListKey(groups), RefKey(push_constants)),
        |_assets, device, (ListKey(groups), RefKey(push_constants))| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: groups,
                push_constant_ranges: push_constants,
            })
        },
    )
}

pub fn load_shader(
    assets: &mut impl AssetLoader,
    device: &wgpu::Device,
) -> Asset<wgpu::ShaderModule> {
    assets.load(device, (), |_assets, device, ()| {
        device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxel shaders"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("voxel.wgsl"))),
        })
    })
}

pub fn load_geometry_pipeline(
    assets: &mut impl AssetLoader,
    device: &wgpu::Device,
) -> Asset<wgpu::RenderPipeline> {
    assets.load(device, (), |assets, device, ()| {
        let shader = load_shader(assets, device);
        let pipeline_layout = load_pipeline_layout(assets, device, &[], &[]);

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("voxel geometry pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("geometry_vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[QuadInstancePacked::LAYOUT],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("geometry_fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: GBUF_TEXTURES_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::all(),
                })],
            }),
            multiview: None,
            cache: None,
        })
    })
}
