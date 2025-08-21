use bytemuck::{Pod, Zeroable};
use wasmlink::{Marshal, PodMarshal};

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct Bgra8Color {
    pub b: u8,
    pub g: u8,
    pub r: u8,
    pub a: u8,
}

impl Marshal for Bgra8Color {
    type Strategy = PodMarshal<Self>;
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct UVec2 {
    pub x: u32,
    pub y: u32,
}

impl Marshal for UVec2 {
    type Strategy = PodMarshal<Self>;
}

#[derive(Debug, Copy, Clone, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct DVec2 {
    pub x: f64,
    pub y: f64,
}

impl Marshal for DVec2 {
    type Strategy = PodMarshal<Self>;
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct URect2 {
    pub origin: UVec2,
    pub size: UVec2,
}

impl Marshal for URect2 {
    type Strategy = PodMarshal<Self>;
}

#[derive(Debug, Copy, Clone, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct Affine2 {
    pub comps: [f32; 6],
}

impl Marshal for Affine2 {
    type Strategy = PodMarshal<Self>;
}
