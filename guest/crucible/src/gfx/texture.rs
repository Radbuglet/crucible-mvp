use std::{iter, slice};

use glam::{Affine2, UVec2, Vec2};
use wasmlink::{GuestSliceRef, bind_port};

use super::{color::Bgra8, rect::Rect};

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

pub type CpuPixelPositions<'a> = iter::Zip<PixelPositions, iter::Copied<slice::Iter<'a, Bgra8>>>;

pub type CpuPixelPositionsMut<'a> = iter::Zip<PixelPositions, slice::IterMut<'a, Bgra8>>;

#[derive(Debug, Clone)]
pub struct CpuTexture {
    size: UVec2,
    pixels: Vec<Bgra8>,
}

impl CpuTexture {
    pub fn new(size: UVec2) -> Self {
        Self {
            size,
            pixels: (0..size.x as usize * size.y as usize)
                .map(|_| Bgra8::ZERO)
                .collect(),
        }
    }

    pub fn from_fn(size: UVec2, mut f: impl FnMut(usize, UVec2) -> Bgra8) -> Self {
        Self {
            size,
            pixels: (0..size.x as usize * size.y as usize)
                .zip(PixelPositions::new(size))
                .map(|(idx, pos)| f(idx, pos))
                .collect(),
        }
    }

    pub fn from_raw(size: UVec2, pixels: Vec<Bgra8>) -> Self {
        assert_eq!(size.x * size.y, pixels.len() as u32);

        Self { size, pixels }
    }

    pub fn to_raw(self) -> Vec<Bgra8> {
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

    pub fn pixels(&self) -> &[Bgra8] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [Bgra8] {
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

    pub fn pixel(&self, at: UVec2) -> Bgra8 {
        self.pixels()[self.pos_to_index(at)]
    }

    pub fn pixel_mut(&mut self, at: UVec2) -> &mut Bgra8 {
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
    pub(crate) handle: crucible_abi::GpuTextureHandle,
    pub(crate) size: UVec2,
}

impl GpuTexture {
    pub fn new(size: UVec2) -> Self {
        bind_port! {
            fn [crucible_abi::GPU_CREATE_TEXTURE] "crucible".gpu_create_texture(crucible_abi::UVec2)
                -> crucible_abi::GpuTextureHandle;
        }

        Self {
            handle: gpu_create_texture(&bytemuck::cast(size)),
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

    pub fn clear(&mut self, color: Bgra8) {
        bind_port! {
            fn [crucible_abi::GPU_CLEAR_TEXTURE] "crucible".gpu_clear_texture(crucible_abi::GpuClearTextureArgs);
        }

        gpu_clear_texture(&crucible_abi::GpuClearTextureArgs {
            handle: self.handle,
            color: bytemuck::cast(color),
        });
    }

    pub fn upload(&mut self, src: &CpuTexture, at: UVec2, clip: Option<Rect>) {
        bind_port! {
            fn [crucible_abi::GPU_UPLOAD_TEXTURE] "crucible".gpu_upload_texture(crucible_abi::GpuUploadTextureArgs);
        }

        gpu_upload_texture(&crucible_abi::GpuUploadTextureArgs {
            handle: self.handle,
            buffer: GuestSliceRef::new(bytemuck::cast_slice(src.pixels())),
            buffer_size: bytemuck::cast(self.size),
            at: bytemuck::cast(at),
            clip: clip.map(bytemuck::cast).into(),
        });
    }

    pub fn draw(&mut self, args: GpuDrawArgs<'_>) {
        bind_port! {
            fn [crucible_abi::GPU_DRAW_TEXTURE] "crucible".gpu_draw_texture(crucible_abi::GpuDrawTextureArgs);
        }

        let GpuDrawArgs {
            texture,
            transform,
            transform_mode,
            clip,
            tint,
        } = args;

        let transform = transform_mode.normalize_xf(
            transform,
            texture.map_or_else(
                || clip.map_or(UVec2::ONE, |v| v.size),
                |texture| texture.size,
            ),
            self.size,
        );

        gpu_draw_texture(&crucible_abi::GpuDrawTextureArgs {
            dst_handle: self.handle,
            src_handle: texture.as_ref().map(|v| v.handle).into(),
            transform: crucible_abi::Affine2 {
                comps: transform.to_cols_array(),
            },
            clip: clip.map(bytemuck::cast).into(),
            tint: bytemuck::cast(tint),
        });
    }
}

impl Drop for GpuTexture {
    fn drop(&mut self) {
        bind_port! {
            fn [crucible_abi::GPU_DESTROY_TEXTURE] "crucible".gpu_destroy_texture(crucible_abi::GpuTextureHandle);
        }

        gpu_destroy_texture(&self.handle);
    }
}

#[derive(Debug, Copy, Clone)]
#[must_use]
#[non_exhaustive]
pub struct GpuDrawArgs<'a> {
    pub texture: Option<&'a GpuTexture>,
    pub transform: Affine2,
    pub transform_mode: TransformMode,
    pub clip: Option<Rect>,
    pub tint: Bgra8,
}

impl Default for GpuDrawArgs<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> GpuDrawArgs<'a> {
    pub fn new() -> Self {
        Self {
            texture: None,
            transform: Affine2::IDENTITY,
            transform_mode: TransformMode::default(),
            clip: None,
            tint: Bgra8::from_bytes([0xFF; 4]),
        }
    }

    pub fn textured(mut self, texture: &'a GpuTexture) -> Self {
        self.texture = Some(texture);
        self
    }

    pub fn translate(mut self, by: Vec2) -> Self {
        self.transform = Affine2::from_translation(by) * self.transform;
        self
    }

    pub fn scale(mut self, factor: Vec2) -> Self {
        self.transform = Affine2::from_scale(factor) * self.transform;
        self
    }

    pub fn transform(mut self, transform: Affine2) -> Self {
        self.transform = transform * self.transform;
        self
    }

    pub fn clip(mut self, rect: Option<Rect>) -> Self {
        self.clip = rect;
        self
    }

    pub fn tint(mut self, color: Bgra8) -> Self {
        self.tint = color;
        self
    }

    pub fn mode(mut self, mode: TransformMode) -> Self {
        self.transform_mode = mode;
        self
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Default)]
pub enum TransformMode {
    /// The transform is to be interpreted as a mapping from a unit square from `(0, 0)` to `(1, 1)`
    /// representing the cropped portion of the source texture onto a coordinate system representing
    /// the entire destination texture where `(0, 0)` maps to the top left of the target texture and
    /// `(target.width, target.height)` maps to its bottom right.
    #[default]
    FixSize,

    /// The transform is to be interpreted as a mapping from a rectangle from `(0, 0)` to
    /// `(crop.width, crop.height)` representing the cropped portion of the source texture onto a
    /// coordinate system representing the entire destination texture where `(0, 0)` maps to the top
    /// left of the target texture and `(target.width, target.height)` maps to its bottom right.
    ScaleSize,

    /// The transform is to be interpreted as an OpenGL normalized-device-coordinate-esque mapping
    /// from a unit square from `(-1, -1)` to `(1, 1)` representing the cropped portion of the
    /// source texture onto a coordinate system representing the entire destination texture where
    /// `(-1, -1)` maps to the bottom left of the target texture and `(1, 1)` maps to its top right.
    OpenGl,
}

impl TransformMode {
    pub fn normalize_xf(self, xf: Affine2, src_size: UVec2, target_size: UVec2) -> Affine2 {
        match self {
            TransformMode::FixSize | TransformMode::ScaleSize => {
                let mut whole_xf = Affine2::IDENTITY;

                // We're starting from a unit square representing the cropped portion of the source
                // texture. Let us map this into the source form expected by the mode.
                whole_xf = Affine2::from_translation(Vec2::ONE) * whole_xf;
                whole_xf = Affine2::from_scale(Vec2::splat(0.5)) * whole_xf;

                if self == TransformMode::ScaleSize {
                    whole_xf = Affine2::from_scale(src_size.as_vec2()) * whole_xf;
                }

                // Now, let us apply the user's transformation, getting us into a mode-specific
                // destination space.
                whole_xf = xf * whole_xf;

                // Finally, let's convert our destination space ranging from `(0, 0)` to
                // `(target.width, target.height)` to a space ranging from `(-1, -1)` to `(1, 1)`.
                whole_xf = Affine2::from_scale(target_size.as_vec2().recip() * 2.) * whole_xf;
                whole_xf = Affine2::from_translation(-Vec2::ONE) * whole_xf;
                whole_xf = Affine2::from_scale(Vec2::new(1., -1.)) * whole_xf;

                whole_xf
            }
            TransformMode::OpenGl => xf,
        }
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
