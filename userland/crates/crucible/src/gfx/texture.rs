use std::{iter, ptr, slice};

use glam::{Affine2, UVec2, Vec2};

use super::{color::Color8, rect::Rect};

// === Position Math === //

pub const fn index_to_pos(width: u32, idx: usize) -> UVec2 {
    UVec2::new(idx as u32 % width, idx as u32 / width)
}

pub const fn pos_to_index(width: u32, pos: UVec2) -> usize {
    (pos.y * width + pos.x) as usize
}

#[derive(Debug, Clone)]
pub struct PixelPositions {
    pos: UVec2,
    size: UVec2,
}

impl PixelPositions {
    pub const fn new(size: UVec2) -> Self {
        Self {
            pos: UVec2::ZERO,
            size,
        }
    }
}

impl ExactSizeIterator for PixelPositions {}

impl Iterator for PixelPositions {
    type Item = UVec2;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.pos;

        // Handle end condition.
        if self.pos.y == self.size.y {
            return None;
        }

        // Scan out row by row.
        self.pos.x += 1;
        if self.pos.x == self.size.x {
            self.pos.x = 0;
            self.pos.y += 1;
        }

        Some(next)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = (self.size.x * self.size.y) as usize;

        (size, Some(size))
    }
}

// === CpuTexture === //

pub type CpuPixelPositions<'a> = iter::Zip<PixelPositions, iter::Copied<slice::Iter<'a, Color8>>>;

pub type CpuPixelPositionsMut<'a> = iter::Zip<PixelPositions, slice::IterMut<'a, Color8>>;

#[derive(Debug, Clone)]
pub struct CpuTexture {
    size: UVec2,
    pixels: Vec<Color8>,
}

impl CpuTexture {
    pub fn new(size: UVec2) -> Self {
        Self {
            size,
            pixels: (0..size.x as usize * size.y as usize)
                .map(|_| Color8::ZERO)
                .collect(),
        }
    }

    pub fn from_fn(size: UVec2, mut f: impl FnMut(usize, UVec2) -> Color8) -> Self {
        Self {
            size,
            pixels: (0..size.x as usize * size.y as usize)
                .zip(PixelPositions::new(size))
                .map(|(idx, pos)| f(idx, pos))
                .collect(),
        }
    }

    pub fn from_raw(size: UVec2, pixels: Vec<Color8>) -> Self {
        assert_eq!(size.x * size.y, pixels.len() as u32);

        Self { size, pixels }
    }

    pub fn to_raw(self) -> Vec<Color8> {
        self.pixels
    }

    pub fn size(&self) -> UVec2 {
        self.size
    }

    pub fn width(&self) -> u32 {
        self.size.x
    }

    pub fn height(&self) -> u32 {
        self.size.y
    }

    pub fn pixels(&self) -> &[Color8] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [Color8] {
        &mut self.pixels
    }

    pub fn enumerate(&self) -> PixelPositions {
        PixelPositions::new(self.size)
    }

    pub fn pixels_enumerate(&self) -> CpuPixelPositions<'_> {
        self.enumerate().zip(self.pixels.iter().copied())
    }

    pub fn pixels_enumerate_mut(&mut self) -> CpuPixelPositionsMut<'_> {
        self.enumerate().zip(self.pixels.iter_mut())
    }

    pub fn index_to_pos(&self, idx: usize) -> UVec2 {
        index_to_pos(self.width(), idx)
    }

    pub fn pos_to_index(&self, pos: UVec2) -> usize {
        pos_to_index(self.width(), pos)
    }

    pub fn pixel(&self, at: UVec2) -> Color8 {
        self.pixels()[self.pos_to_index(at)]
    }

    pub fn pixel_mut(&mut self, at: UVec2) -> &mut Color8 {
        let idx = self.pos_to_index(at);

        &mut self.pixels_mut()[idx]
    }

    pub fn make_gpu(&self) -> GpuTexture {
        let mut gpu = GpuTexture::new(self.size());
        gpu.upload(self, UVec2::ZERO, None);
        gpu
    }
}

// === GpuTexture === //

#[derive(Debug)]
pub struct GpuTexture {
    handle: u32,
    size: UVec2,
}

impl GpuTexture {
    pub fn new(size: UVec2) -> Self {
        unsafe extern "C" {
            fn bnuy_create_texture(width: u32, height: u32) -> u32;
        }

        Self {
            handle: unsafe { bnuy_create_texture(size.x, size.y) },
            size,
        }
    }

    pub fn size(&self) -> UVec2 {
        self.size
    }

    pub fn width(&self) -> u32 {
        self.size.x
    }

    pub fn height(&self) -> u32 {
        self.size.y
    }

    pub fn full_rect(&self) -> Rect {
        Rect::new(0, 0, self.width(), self.height())
    }

    pub fn clear(&mut self, color: Color8) {
        self.clear_rect(self.full_rect());

        if color.to_bytes() != [0; 4] {
            self.fill_rect(self.full_rect(), color);
        }
    }

    pub fn clear_rect(&mut self, rect: Rect) {
        unsafe extern "C" {
            fn bnuy_clear_texture_rect(target_id: u32, x: u32, y: u32, w: u32, h: u32);
        }

        unsafe { bnuy_clear_texture_rect(self.handle, rect.x(), rect.y(), rect.w(), rect.h()) };
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color8) {
        unsafe extern "C" {
            fn bnuy_fill_texture_rect(
                target_id: u32,
                x: u32,
                y: u32,
                w: u32,
                h: u32,
                color: *const [u8; 4],
            );
        }

        unsafe {
            bnuy_fill_texture_rect(
                self.handle,
                rect.x(),
                rect.y(),
                rect.w(),
                rect.h(),
                &color.to_bytes(),
            )
        };
    }

    pub fn upload(&mut self, src: &CpuTexture, at: UVec2, clip: Option<Rect>) {
        unsafe extern "C" {
            fn bnuy_upload_texture(
                target_id: u32,
                buffer: *const Color8,
                buffer_width: u32,
                buffer_height: u32,
                at_x: u32,
                at_y: u32,
                clip: *const [u32; 4],
            );
        }

        let clip = clip.map(|rect| [rect.top.x, rect.top.y, rect.size.x, rect.size.y]);
        let clip = clip.map_or(ptr::null(), |clip| &clip);

        unsafe {
            bnuy_upload_texture(
                self.handle,
                src.pixels.as_ptr(),
                src.size.x,
                src.size.y,
                at.x,
                at.y,
                clip,
            );
        }
    }

    pub fn draw(&mut self, args: GpuDrawArgs<'_>) {
        let GpuDrawArgs {
            texture,
            transform,
            clip,
            opacity,
        } = args;

        unsafe extern "C" {
            fn bnuy_draw_texture(
                target_id: u32,
                src_id: u32,
                transform: *const [f32; 6],
                clip: *const [u32; 4],
                opacity: f32,
            );
        }

        let transform = transform.to_cols_array();
        let clip = clip.map(|rect| [rect.top.x, rect.top.y, rect.size.x, rect.size.y]);
        let clip = clip.map_or(ptr::null(), |clip| &clip);

        unsafe { bnuy_draw_texture(self.handle, texture.handle, &transform, clip, opacity) };
    }

    pub fn present(&self) {
        unsafe extern "C" {
            fn bnuy_present(target: u32);
        }

        unsafe { bnuy_present(self.handle) };
    }
}

impl Drop for GpuTexture {
    fn drop(&mut self) {
        unsafe extern "C" {
            fn bnuy_destroy_texture(handle: u32);
        }

        unsafe { bnuy_destroy_texture(self.handle) };
    }
}

#[derive(Debug, Copy, Clone)]
#[must_use]
#[non_exhaustive]
pub struct GpuDrawArgs<'a> {
    pub texture: &'a GpuTexture,
    pub transform: Affine2,
    pub clip: Option<Rect>,
    pub opacity: f32,
}

impl<'a> GpuDrawArgs<'a> {
    pub fn new(texture: &'a GpuTexture) -> Self {
        Self {
            texture,
            transform: Affine2::IDENTITY,
            clip: None,
            opacity: 1.,
        }
    }

    pub fn translated(mut self, by: Vec2) -> Self {
        self.transform = Affine2::from_translation(by) * self.transform;
        self
    }

    pub fn scaled(mut self, factor: Vec2) -> Self {
        self.transform = Affine2::from_scale(factor) * self.transform;
        self
    }

    pub fn transformed(mut self, transform: Affine2) -> Self {
        self.transform = transform;
        self
    }

    pub fn clipped(mut self, rect: Option<Rect>) -> Self {
        self.clip = rect;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

// === TexturePart === //

pub type CpuTexturePart<'a> = TexturePart<&'a CpuTexture>;
pub type GpuTexturePart<'a> = TexturePart<&'a GpuTexture>;

#[derive(Debug, Copy, Clone)]
pub struct TexturePart<T> {
    pub texture: T,
    pub part: Rect,
}

impl<T> TexturePart<T> {
    pub fn as_gpu_ref(&self) -> GpuTexturePart<'_>
    where
        T: AsRef<GpuTexture>,
    {
        TexturePart {
            texture: self.texture.as_ref(),
            part: self.part,
        }
    }

    pub fn as_cpu_ref(&self) -> CpuTexturePart<'_>
    where
        T: AsRef<CpuTexture>,
    {
        TexturePart {
            texture: self.texture.as_ref(),
            part: self.part,
        }
    }
}
