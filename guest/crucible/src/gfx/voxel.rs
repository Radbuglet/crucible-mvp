use std::num::NonZeroU32;

use glam::{Affine3A, Mat4};

use super::{model::Model, texture::GpuTexturePart};

pub const CHUNK_EDGE: usize = 16;
pub const CHUNK_VOLUME: usize = CHUNK_EDGE * CHUNK_EDGE * CHUNK_EDGE;

#[repr(C)]
pub struct Material {
    pub palette: u16,
    pub variant: u16,
}

// === VoxelChunk === //

#[derive(Debug)]
pub struct VoxelChunk {
    handle: NonZeroU32,
}

impl Default for VoxelChunk {
    fn default() -> Self {
        Self::new()
    }
}

impl VoxelChunk {
    pub fn new() -> Self {
        todo!()
    }

    pub fn set_data(&mut self, data: &[Material; CHUNK_VOLUME]) {
        todo!()
    }

    pub fn set_palette(&mut self, palette: &VoxelPalette) {
        todo!()
    }

    pub fn set_transparent(&mut self, has_transparency: bool) {
        todo!()
    }

    pub fn set_transform(&mut self, xf: Affine3A) {
        todo!()
    }

    pub fn draw(&self, camera: Mat4) {
        todo!()
    }
}

impl Drop for VoxelChunk {
    fn drop(&mut self) {
        todo!()
    }
}

// === VoxelPalette === //

#[derive(Debug)]
pub struct VoxelPalette {
    handle: NonZeroU32,
}

impl Default for VoxelPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl VoxelPalette {
    pub fn new() -> Self {
        todo!()
    }

    pub fn define_simple(&mut self, id: u16, faces: [GpuTexturePart<'_>; 6]) {
        todo!()
    }

    pub fn define_complex(&mut self, id: u16, model: &Model) {
        todo!()
    }

    pub fn unset(&mut self, id: u16) {
        todo!()
    }
}

impl Drop for VoxelPalette {
    fn drop(&mut self) {
        todo!()
    }
}
