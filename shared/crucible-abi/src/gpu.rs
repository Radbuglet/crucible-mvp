use bytemuck::{Pod, Zeroable};
use wasmlink::{Marshal, PodMarshal, Port, marshal_struct};

use crate::math::{Affine2, Bgra8Color, URect2, UVec2};

pub const GPU_CREATE_TEXTURE: Port<UVec2, GpuTextureHandle> =
    Port::new("crucible", "gpu_create_texture");

pub const GPU_CLEAR_TEXTURE: Port<GpuClearTextureArgs> = Port::new("crucible", "gpu_clear_texture");

pub const GPU_UPLOAD_TEXTURE: Port<GpuUploadTextureArgs> =
    Port::new("crucible", "gpu_upload_texture");

pub const GPU_DRAW_TEXTURE: Port<GpuDrawTextureArgs> = Port::new("crucible", "gpu_draw_texture");

pub const GPU_DESTROY_TEXTURE: Port<GpuTextureHandle> =
    Port::new("crucible", "gpu_destroy_texture");

marshal_struct! {
    pub struct GpuClearTextureArgs {
        pub handle: GpuTextureHandle,
        pub color: Bgra8Color,
    }

    pub struct GpuUploadTextureArgs {
        pub handle: GpuTextureHandle,
        pub buffer: Vec<Bgra8Color>,
        pub buffer_size: UVec2,
        pub at: UVec2,
        pub clip: Option<URect2>,
    }

    pub struct GpuDrawTextureArgs {
        pub dst_handle: GpuTextureHandle,
        pub src_handle: Option<GpuTextureHandle>,
        pub transform: Affine2,
        pub clip: Option<URect2>,
        pub tint: Bgra8Color,
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Pod, Zeroable)]
#[repr(transparent)]
pub struct GpuTextureHandle {
    pub raw: u32,
}

impl Marshal for GpuTextureHandle {
    type Strategy = PodMarshal<Self>;
}
