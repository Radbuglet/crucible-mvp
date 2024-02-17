use std::{any::type_name, marker::PhantomData};

use anyhow::Context;
use bytemuck::Pod;

// === Re-Exports === //

pub use crt_marshal::*;

// === Heap Parsing === //

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

pub trait MemoryRead {
    fn as_slice(&self) -> &[u8];

    fn load_range(&self, base: u32, len: u32) -> anyhow::Result<&[u8]> {
        self.as_slice()
            .get(base as usize..)
            .and_then(|s| s.get(..len as usize))
            .with_context(|| {
                format!(
                    "failed to read memory range from {base} to {len} (memory size: {})",
                    self.as_slice().len()
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
        self.load_slice_raw(ptr.base.addr.get(), ptr.len.get())
    }

    fn load_str(&self, ptr: WasmStr) -> anyhow::Result<&str> {
        self.load_str_raw(ptr.0.base.addr.get(), ptr.0.len.get())
    }
}

impl MemoryRead for [u8] {
    fn as_slice(&self) -> &[u8] {
        self
    }
}

// === Function Parsing === //

macro_rules! impl_variadic {
    ($target:path) => {
        impl_variadic!($target; V1 V2 V3 V4 V5 V6 V7 V8 V9 V10 V11 V12);
    };
    ($target:path; $($first:ident $($remaining:ident)*)?) => {
        $target!($($first $($remaining)*)?);
        $(impl_variadic!($target; $($remaining)*);)?
    };
}

pub trait CtxHasMainMemory: Sized {
    fn extract_main_memory<'a>(
        caller: &'a mut wasmtime::Caller<'_, Self>,
    ) -> (&'a mut [u8], &'a mut Self);
}

pub trait MarshaledResTy {
    type Res: wasmtime::WasmRet;

    fn to_normalized_res(self) -> anyhow::Result<Self::Res>;
}

macro_rules! impl_marshaled_res_ty {
    ($($para:ident)*) => {
        impl<$($para: MarshaledArgTy,)*> MarshaledResTy for ($($para,)*) {
            type Res = ($($para::Prim,)*);

            #[allow(clippy::unused_unit, non_snake_case)]
            fn to_normalized_res(self) -> anyhow::Result<Self::Res> {
                let ($($para,)*) = self;
                Ok(($(MarshaledArgTy::into_prim($para),)*))
            }
        }

        impl<$($para: MarshaledArgTy,)*> MarshaledResTy for anyhow::Result<($($para,)*)> {
            type Res = ($($para::Prim,)*);

            #[allow(clippy::unused_unit, non_snake_case)]
            fn to_normalized_res(self) -> anyhow::Result<Self::Res> {
                let ($($para,)*) = self?;
                Ok(($(MarshaledArgTy::into_prim($para),)*))
            }
        }
    };
}

impl_variadic!(impl_marshaled_res_ty);

pub trait MarshaledFunc<C, Args, Ret>
where
    C: CtxHasMainMemory,
    Ret: MarshaledResTy,
{
    fn wrap(self, store: impl wasmtime::AsContextMut<Data = C>) -> wasmtime::Func;
}

macro_rules! impl_func_ty {
    ($($ty:ident)*) => {
        impl<C, F, Ret, $($ty: MarshaledArgTy,)*> MarshaledFunc<C, ($($ty,)*), Ret> for F
        where
            C: CtxHasMainMemory,
            Ret: MarshaledResTy,
            F: 'static + Send + Sync + Fn(&mut C, &mut [u8], $($ty,)*) -> Ret,
        {
            #[allow(non_snake_case)]
            fn wrap(self, store: impl wasmtime::AsContextMut<Data = C>) -> wasmtime::Func {
                wasmtime::Func::wrap(
                    store,
                    move |mut caller: wasmtime::Caller<'_, C>, $($ty: <$ty as MarshaledArgTy>::Prim,)*| {
                        let (memory, cx) = C::extract_main_memory(&mut caller);
                        self(cx, memory, $(<$ty>::from_prim($ty).context("failed to parse argument")?,)*)
                            .to_normalized_res()
                    },
                )
            }
        }
    };
}

impl_variadic!(impl_func_ty);
