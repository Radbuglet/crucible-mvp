use crevice::std430;

// === Crevice conversions === //

pub fn vec2_to_crevice(v: glam::Vec2) -> std430::Vec2 {
    std430::Vec2 { x: v.x, y: v.y }
}

// === `vertex_attributes` macro === //

#[doc(hidden)]
pub mod vertex_attributes_internals {
    pub use {std::mem::offset_of, wgpu::VertexAttribute};
}

macro_rules! vertex_attributes {
    ($struct:ty => $($field:ident: $format:expr),*$(,)?) => {{
        let arr = [$(
            (
                $crate::utils::crevice::vertex_attributes_internals::offset_of!($struct, $field) as _,
                $format
            ),
        )*];

        let mut i = 0;

        arr.map(|(offset, format)| {
            let res = $crate::utils::crevice::vertex_attributes_internals::VertexAttribute {
                format,
                offset,
                shader_location: i,
            };
            i += 1;
            res
        })
    }};
}

pub(crate) use vertex_attributes;
