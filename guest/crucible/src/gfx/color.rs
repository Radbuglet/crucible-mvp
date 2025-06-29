use std::fmt;

use bytemuck::{Pod, Zeroable};
use glam::{Vec3, Vec4};

// === Color8 === //

macro_rules! conversions {
    ($(
        $ty:ty,
            $from:ident($from_arg:pat) $from_expr:expr,
            $to:ident($to_arg:pat) $to_expr:expr
        ;
    )*) => {
        $(
            impl Color8 {
                pub const fn $from($from_arg: $ty) -> Self {
                    $from_expr
                }

                pub const fn $to(self) -> $ty {
                    let $to_arg = self;
                    $to_expr
                }
            }

            impl From<$ty> for Color8 {
                fn from(other: $ty) -> Self {
                    Self::$from(other)
                }
            }

            impl From<Color8> for $ty {
                fn from(other: Color8) -> Self {
                    other.$to()
                }
            }
        )*
    };
}

#[derive(Copy, Clone, Hash, Eq, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct Color8 {
    pub b: u8,
    pub g: u8,
    pub r: u8,
    pub a: u8,
}

impl fmt::Debug for Color8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { r, g, b, a } = self;
        write!(f, "Color8({r}, {g}, {b}, {a})")
    }
}

impl Color8 {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

conversions! {
    [u8; 4],
        from_bytes([r, g, b, a]) Self::new(r, g, b, a),
        to_bytes(Self { r, g, b, a }) [r, g, b, a];

    [u8; 3],
        from_bytes_rgb([r, g, b]) Self::new(r, g, b, u8::MAX),
        to_bytes_rgb(Self { r, g, b, a: _ }) [r, g, b];

    (u8, u8, u8, u8),
        from_tup((r, g, b, a)) Self::new(r, g, b, a),
        to_tup(Self { r, g, b, a }) (r, g, b, a);

    (u8, u8, u8),
        from_tup_rgb((r, g, b)) Self::new(r, g, b, u8::MAX),
        to_tup_rgb(Self { r, g, b, a: _ }) (r, g, b);

    [f32; 4],
        from_floats([r, g, b, a]) Self::new(
            (r * 255.) as u8,
            (g * 255.) as u8,
            (b * 255.) as u8,
            (a * 255.) as u8,
        ),
        to_floats(Self { r, g, b, a }) [
            (r * 255) as f32,
            (g * 255) as f32,
            (b * 255) as f32,
            (a * 255) as f32,
        ];

    [f32; 3],
        from_floats_rgb([r, g, b]) Self::new(
            (r * 255.) as u8,
            (g * 255.) as u8,
            (b * 255.) as u8,
            u8::MAX,
        ),
        to_floats_rgb(Self { r, g, b, a: _ }) [
            (r * 255) as f32,
            (g * 255) as f32,
            (b * 255) as f32,
        ];

    (f32, f32, f32, f32),
        from_float_tup((r, g, b, a)) Self::from_floats([r, g, b, a]),
        to_float_tup(v) {
            let [r, g, b, a] = v.to_floats();
            (r, g, b, a)
        };

    (f32, f32, f32),
        from_float_tup_rgb((r, g, b)) Self::from_floats_rgb([r, g, b]),
        to_float_tup_rgb(v) {
            let [r, g, b] = v.to_floats_rgb();
            (r, g, b)
        };

    Vec3,
        from_vec3(v) Self::from_floats_rgb(v.to_array()),
        to_vec3(v) Vec3::from_array(v.to_floats_rgb());

    Vec4,
        from_vec4(v) Self::from_floats(v.to_array()),
        to_vec4(v) Vec4::from_array(v.to_floats());
}

// === Palette === //

impl Color8 {
    pub const ZERO: Self = Self::from_bytes([0; 4]);
    pub const LIGHTGRAY: Self = Self::from_floats([0.78, 0.78, 0.78, 1.00]);
    pub const GRAY: Self = Self::from_floats([0.51, 0.51, 0.51, 1.00]);
    pub const DARKGRAY: Self = Self::from_floats([0.31, 0.31, 0.31, 1.00]);
    pub const YELLOW: Self = Self::from_floats([0.99, 0.98, 0.00, 1.00]);
    pub const GOLD: Self = Self::from_floats([1.00, 0.80, 0.00, 1.00]);
    pub const ORANGE: Self = Self::from_floats([1.00, 0.63, 0.00, 1.00]);
    pub const PINK: Self = Self::from_floats([1.00, 0.43, 0.76, 1.00]);
    pub const RED: Self = Self::from_floats([0.90, 0.16, 0.22, 1.00]);
    pub const MAROON: Self = Self::from_floats([0.75, 0.13, 0.22, 1.00]);
    pub const GREEN: Self = Self::from_floats([0.00, 0.89, 0.19, 1.00]);
    pub const LIME: Self = Self::from_floats([0.00, 0.62, 0.18, 1.00]);
    pub const DARKGREEN: Self = Self::from_floats([0.00, 0.46, 0.17, 1.00]);
    pub const SKYBLUE: Self = Self::from_floats([0.40, 0.75, 1.00, 1.00]);
    pub const BLUE: Self = Self::from_floats([0.00, 0.47, 0.95, 1.00]);
    pub const DARKBLUE: Self = Self::from_floats([0.00, 0.32, 0.67, 1.00]);
    pub const PURPLE: Self = Self::from_floats([0.78, 0.48, 1.00, 1.00]);
    pub const VIOLET: Self = Self::from_floats([0.53, 0.24, 0.75, 1.00]);
    pub const DARKPURPLE: Self = Self::from_floats([0.44, 0.12, 0.49, 1.00]);
    pub const BEIGE: Self = Self::from_floats([0.83, 0.69, 0.51, 1.00]);
    pub const BROWN: Self = Self::from_floats([0.50, 0.42, 0.31, 1.00]);
    pub const DARKBROWN: Self = Self::from_floats([0.30, 0.25, 0.18, 1.00]);
    pub const WHITE: Self = Self::from_floats([1.00, 1.00, 1.00, 1.00]);
    pub const BLACK: Self = Self::from_floats([0.00, 0.00, 0.00, 1.00]);
    pub const BLANK: Self = Self::from_floats([0.00, 0.00, 0.00, 0.00]);
    pub const MAGENTA: Self = Self::from_floats([1.00, 0.00, 1.00, 1.00]);
}
