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

// CtxHasMainMemory
pub trait CtxHasMainMemory: Sized {
    fn extract_main_memory<'a>(
        caller: &'a mut wasmtime::Caller<'_, Self>,
    ) -> (&'a mut [u8], &'a mut Self);
}

// MarshaledFunc
pub trait MarshaledFunc<T, Params, Results>: Sized {
    fn make_func_inner(self) -> impl IntoAnyFunc<T>;
}

macro_rules! impl_func_ty {
    ($($ty:ident)*) => {
        impl<T, F, Ret, $($ty: MarshaledTy,)*> MarshaledFunc<T, ($($ty,)*), Ret> for F
        where
            T: 'static + CtxHasMainMemory,
            Ret: MarshaledResults,
            F: 'static + Send + Sync + Fn(&mut T, &mut [u8], $($ty,)*) -> anyhow::Result<Ret>,
        {
            #[allow(non_snake_case)]
            fn make_func_inner(self) -> impl IntoAnyFunc<T> {
                make_into_any_func(move |mut caller: wasmtime::Caller<'_, T>, $($ty: <$ty as MarshaledTy>::Prim,)*| {
                    let (memory, cx) = T::extract_main_memory(&mut caller);
                    self(cx, memory, $(<$ty>::from_prim($ty).context("failed to parse argument")?,)*)
                        .map(MarshaledResults::into_prims)
                })
            }
        }
    };
}

impl_variadic!(impl_func_ty);

// IntoAnyFunc
pub trait IntoAnyFunc<T> {
    type Inner: wasmtime::IntoFunc<T, Self::Params, Self::Results>;
    type Params;
    type Results;

    fn into_inner(self) -> Self::Inner;
}

pub fn make_into_any_func<F, T, Params, Results>(
    f: F,
) -> impl IntoAnyFunc<T, Params = Params, Results = Results>
where
    F: wasmtime::IntoFunc<T, Params, Results>,
{
    struct Inner<F, M>(F, PhantomData<fn(M) -> M>);

    impl<F, Context, Params, Results> IntoAnyFunc<Context> for Inner<F, (Context, Params, Results)>
    where
        F: wasmtime::IntoFunc<Context, Params, Results>,
    {
        type Inner = F;
        type Params = Params;
        type Results = Results;

        fn into_inner(self) -> Self::Inner {
            self.0
        }
    }

    Inner(f, PhantomData)
}

// `bind_to_linker`
pub fn bind_to_linker<'l, F, T, Params, Results>(
    linker: &'l mut wasmtime::Linker<T>,
    module: &str,
    name: &str,
    func: F,
) -> anyhow::Result<&'l mut wasmtime::Linker<T>>
where
    F: MarshaledFunc<T, Params, Results>,
{
    linker.func_wrap(module, name, func.make_func_inner().into_inner())
}
