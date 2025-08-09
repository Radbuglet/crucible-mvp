use bytemuck::{Pod, Zeroable};
use glam::UVec2;

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct Rect {
    pub top: UVec2,
    pub size: UVec2,
}

impl Rect {
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self {
            top: UVec2::new(x, y),
            size: UVec2::new(w, h),
        }
    }

    pub const fn x(self) -> u32 {
        self.top.x
    }

    pub const fn y(self) -> u32 {
        self.top.y
    }

    pub const fn w(self) -> u32 {
        self.size.x
    }

    pub const fn h(self) -> u32 {
        self.size.y
    }
}
