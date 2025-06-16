use std::num::NonZeroU32;

use glam::{Affine2, Affine3A};

use super::{color::Color8, texture::GpuTexturePart};

// === Model === //

#[derive(Debug)]
pub struct Model {
    handle: NonZeroU32,
}

impl Model {
    pub fn new(parts: &[ModelPart<'_>]) -> Self {
        todo!()
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        todo!()
    }
}

#[derive(Debug)]
pub struct ModelPart<'a> {
    pub affine: Affine2,
    pub texture: GpuTexturePart<'a>,
    pub flags: ModelFlags,
}

bitflags::bitflags! {
    #[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
    pub struct ModelFlags : u32 {
        const FULL_BRIGHT = 1 << 0;
        const BILLBOARD_X = 1 << 1;
        const BILLBOARD_Y = 1 << 2;
    }
}

// === ModelBuffer === //

#[derive(Debug)]
pub struct ModelBuffer {}

impl ModelBuffer {
    pub fn new(model: &Model) -> Self {
        todo!()
    }

    pub fn push(&mut self, affine: Affine3A, tint: Color8) -> InstanceIdx {
        todo!()
    }

    pub fn update(&mut self, affine: Affine3A, tint: Color8) {
        todo!()
    }

    pub fn remove(&mut self, at: InstanceIdx) {
        todo!()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct InstanceIdx(pub u32);
