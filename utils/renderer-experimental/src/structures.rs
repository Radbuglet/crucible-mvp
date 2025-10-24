use std::mem::{self, offset_of};

use crevice::std430::AsStd430;
use glam::{IVec3, U8Vec3};

use crate::utils::pack_bitmask;

// === Facing === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct Facing {
    pub axis: Axis3,
    pub angle: AlignedAngle,
}

impl Facing {
    pub fn pack(self) -> FacingPacked {
        FacingPacked {
            encoded: pack_bitmask([(2, self.axis as u32), (2, self.angle as u32)]),
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum Axis3 {
    X,
    Y,
    Z,
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum AlignedAngle {
    Ccw0,
    Ccw90,
    Ccw180,
    Ccw270,
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, AsStd430)]
pub struct FacingPacked {
    pub encoded: u32,
}

// === QuadInstance === //

#[derive(Debug, Copy, Clone)]
pub struct QuadInstance {
    pub pos: IVec3,
    pub scale: U8Vec3,
    pub facing: Facing,
    pub surface_index: u32,
}

impl QuadInstance {
    pub fn pack(self) -> QuadInstancePacked {
        QuadInstancePacked {
            pos_xy: pack_bitmask([(16, self.pos.x as u32), (16, self.pos.y as u32)]),
            pos_z_and_scale_xyz: pack_bitmask([
                (16, self.pos.z as u32),
                (4, self.scale.x as u32),
                (4, self.scale.y as u32),
                (4, self.scale.z as u32),
            ]),
            facing_and_surface_idx: pack_bitmask([
                (4, self.facing.pack().encoded),
                (28, self.surface_index),
            ]),
        }
    }
}

#[derive(Debug, Copy, Clone, AsStd430)]
pub struct QuadInstancePacked {
    pub pos_xy: u32,
    pub pos_z_and_scale_xyz: u32,
    pub facing_and_surface_idx: u32,
}

impl QuadInstancePacked {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<<QuadInstancePacked as AsStd430>::Output>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: Self::ATTRIBUTES,
    };

    pub const ATTRIBUTES: &'static [wgpu::VertexAttribute] = &[
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: offset_of!(Self, pos_xy) as u64,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: offset_of!(Self, pos_z_and_scale_xyz) as u64,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: offset_of!(Self, facing_and_surface_idx) as u64,
            shader_location: 2,
        },
    ];
}
