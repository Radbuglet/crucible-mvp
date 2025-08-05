use std::mem::MaybeUninit;

use bytemuck::{Pod, Zeroable};
use wasmlink::{
    FfiOption, GuestSliceRef, GuestboundOf, HostboundOf, PodMarshalStrategy, marshal_struct,
};

marshal_struct! {
    pub struct ImageCreateArgs {
        width: u32,
        height: u32,
        data: Vec<PodMarshalStrategy<Pixel>>,
        next: Option<i32>,
    }
}

#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
pub struct Pixel {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

pub fn create_image<'a>(
    args: &'a HostboundOf<'a, ImageCreateArgs>,
) -> GuestboundOf<ImageCreateArgs> {
    unsafe extern "C" {
        unsafe fn create_image(
            args: &HostboundOf<ImageCreateArgs>,
            out: *mut GuestboundOf<ImageCreateArgs>,
        );
    }

    let mut out = MaybeUninit::uninit();

    unsafe { create_image(args, out.as_mut_ptr()) };

    unsafe { out.assume_init() }
}

fn main() {
    let mut data = create_image(&ImageCreateArgs {
        width: 1024,
        height: 1024,
        data: GuestSliceRef::new(&[Pixel {
            r: 10,
            g: 40,
            b: 20,
            a: 7,
        }]),
        next: FfiOption::some(1),
    });

    data.height += 1;
    let foo = data.data.decode();
}
