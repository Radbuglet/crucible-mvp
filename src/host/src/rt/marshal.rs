use std::{any::type_name, marker::PhantomData};

use anyhow::Context;
use bytemuck::Pod;
use crucible_shared::{WasmPtr, WasmSlice, WasmStr};

use crate::util::extension::ExtensionFor;

pub const fn size_of_32<T>() -> u32 {
    struct SizeOf<T>(PhantomData<fn() -> T>);

    impl<T> SizeOf<T> {
        const SIZE: u32 = {
            let size = std::mem::size_of::<T>();
            if size > u32::MAX as usize {
                panic!("structure is too big");
            }

            size as u32
        };
    }

    <SizeOf<T>>::SIZE
}

pub trait MemoryExt: ExtensionFor<[u8]> {
    fn load_range(&self, base: u32, len: u32) -> anyhow::Result<&[u8]> {
        self.v()
            .get(base as usize..)
            .and_then(|s| s.get(..len as usize))
            .with_context(|| {
                format!(
                    "failed to read memory range from {base} to {len} (memory size: {})",
                    self.v().len()
                )
            })
    }

    fn load_struct_raw<T: Pod>(&self, ptr: u32) -> anyhow::Result<&T> {
        bytemuck::try_from_bytes(self.load_range(ptr, size_of_32::<T>())?).map_err(|err| {
            anyhow::anyhow!(
                "failed to parse object (ty: {}, base: {ptr}): {err}",
                type_name::<T>()
            )
        })
    }

    fn load_slice_raw<T: Pod>(&self, base: u32, len: u32) -> anyhow::Result<&[T]> {
        bytemuck::try_cast_slice(
            self.load_range(
                base,
                len.checked_mul(size_of_32::<T>())
                    .context("slice is too big")?,
            )?,
        )
        .map_err(|err| {
            anyhow::anyhow!(
                "failed to parse slice (ty: {}, base: {base}, len: {len}): {err}",
                type_name::<T>()
            )
        })
    }

    fn load_str_raw(&self, base: u32, len: u32) -> anyhow::Result<&str> {
        self.load_range(base, len)
            .and_then(|data| std::str::from_utf8(data).context("invalid UTF-8"))
    }

    fn load_struct<T: Pod>(&self, ptr: WasmPtr<T>) -> anyhow::Result<&T> {
        self.load_struct_raw(ptr.addr.get())
    }

    fn load_slice<T: Pod>(&self, ptr: WasmSlice<T>) -> anyhow::Result<&[T]> {
        let (base, len) = ptr.into_raw();
        self.load_slice_raw(base.addr.get(), len.get())
    }

    fn load_str(&self, ptr: WasmStr) -> anyhow::Result<&str> {
        let (base, len) = ptr.0.into_raw();
        self.load_str_raw(base.addr.get(), len.get())
    }
}

impl MemoryExt for [u8] {}
