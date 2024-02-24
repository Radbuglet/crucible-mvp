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

pub trait MemoryWrite: MemoryRead {
    fn as_slice_mut(&mut self) -> &mut [u8];

    fn load_range_mut(&mut self, base: u32, len: u32) -> anyhow::Result<&mut [u8]> {
        let mem_len = self.as_slice().len();
        self.as_slice_mut()
            .get_mut(base as usize..)
            .and_then(|s| s.get_mut(..len as usize))
            .with_context(|| {
                format!("failed to read memory range from {base} to {len} (memory size: {mem_len})")
            })
    }

    fn write_range_mut(&mut self, base: u32, data: &[u8]) -> anyhow::Result<()> {
        self.load_range_mut(base, u32::try_from(data.len()).context("slice is too big")?)?
            .copy_from_slice(data);

        Ok(())
    }
}

impl MemoryWrite for [u8] {
    fn as_slice_mut(&mut self) -> &mut [u8] {
        self
    }
}

// === Host-Side Function Handling === //

// CtxHasMainMemory
pub trait CtxHasMainMemory: Sized {
    #[rustfmt::skip]
    fn extract_main_memory<'a>(caller: &'a mut wasmtime::Caller<'_, Self>) ->
        (&'a mut [u8], &'a mut Self);
}

// HostSideMarshaledFunc
pub trait HostSideMarshaledFunc<T, Params, Results>: Sized {
    type PrimParams<'a>;
    type PrimResults;

    #[rustfmt::skip]
    fn wrap_host(self) ->
        impl for<'a> wasmtime::IntoFunc<T, Self::PrimParams<'a>, Self::PrimResults>;
}

macro_rules! impl_func_ty {
    ($($ty:ident)*) => {
        impl<T, F, Ret, $($ty: MarshaledTy,)*> HostSideMarshaledFunc<T, ($($ty,)*), Ret> for F
        where
            T: 'static + CtxHasMainMemory,
            Ret: MarshaledTyList,
            F: 'static + Send + Sync + Fn(&mut T, &mut [u8], $($ty,)*) -> anyhow::Result<Ret>,
        {
            type PrimParams<'a> = (wasmtime::Caller<'a, T>, $(<$ty as MarshaledTy>::Prim,)*);
            type PrimResults = anyhow::Result<Ret::Prims>;

            #[allow(non_snake_case)]
            fn wrap_host(self) -> impl for<'a> wasmtime::IntoFunc<T, Self::PrimParams<'a>, Self::PrimResults> {
                move |mut caller: wasmtime::Caller<'_, T>, $($ty: <$ty as MarshaledTy>::Prim,)*| {
                    let (memory, cx) = T::extract_main_memory(&mut caller);
                    self(cx, memory, $(<$ty>::from_prim($ty).context("failed to parse argument")?,)*)
                        .map(MarshaledTyList::into_prims)
                }
            }
        }
    };
}

impl_variadic!(impl_func_ty);

// `bind_to_linker`
pub fn bind_to_linker<'l, F, T, Params, Results>(
    linker: &'l mut wasmtime::Linker<T>,
    module: &str,
    name: &str,
    func: F,
) -> anyhow::Result<&'l mut wasmtime::Linker<T>>
where
    F: HostSideMarshaledFunc<T, Params, Results>,
{
    linker.func_wrap(module, name, func.wrap_host())
}

// === Guest-Side Function Handling === //

pub type MarshaledTypedFunc<A, R> =
    wasmtime::TypedFunc<<A as MarshaledTyList>::Prims, <R as MarshaledTyList>::Prims>;

pub trait MarshaledTypedFuncExt<A: MarshaledTyList, R: MarshaledTyList> {
    fn call_marshaled(&self, store: impl wasmtime::AsContextMut, args: A) -> anyhow::Result<R>;
}

impl<A: MarshaledTyList, R: MarshaledTyList> MarshaledTypedFuncExt<A, R>
    for MarshaledTypedFunc<A, R>
{
    fn call_marshaled(&self, store: impl wasmtime::AsContextMut, args: A) -> anyhow::Result<R> {
        R::from_prims(self.call(store, A::into_prims(args))?)
            .context("failed to deserialize results")
    }
}
