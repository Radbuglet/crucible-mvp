use std::{any::type_name, marker::PhantomData};

use anyhow::Context;
use bytemuck::Pod;

// === Re-Exports === //

pub use crt_marshal::*;

// === HostMarshaledTy === //

pub trait HostMarshaledTyBase: Sized {
    type HostPrim: wasmtime::WasmTy;
}

pub trait HostMarshaledTy<D>: HostMarshaledTyBase {
    fn into_host_prim(me: Self) -> Self::HostPrim;

    fn from_host_prim(
        cx: impl wasmtime::AsContextMut<Data = D>,
        me: Self::HostPrim,
    ) -> anyhow::Result<Self>;
}

macro_rules! impl_host_marshal_base_from_guest {
    () => {
        type HostPrim = <Self as GuestMarshaledTy>::GuestPrim;
    };
}

macro_rules! impl_host_marshal_from_guest {
    ($data:ty) => {
        fn into_host_prim(me: Self) -> Self::HostPrim {
            Self::into_guest_prim(me)
        }

        fn from_host_prim(
            _cx: impl wasmtime::AsContextMut<Data = $data>,
            me: Self::HostPrim,
        ) -> anyhow::Result<Self> {
            Self::from_guest_prim(me).context("failed to decode")
        }
    };
}

macro_rules! derive_host_marshal_from_guest {
    ($($ty:ty),*$(,)?) => {$(
        impl HostMarshaledTyBase for $ty {
            impl_host_marshal_base_from_guest!();
        }

        impl<D> HostMarshaledTy<D> for $ty {
            impl_host_marshal_from_guest!(D);
        }
    )*};
}

derive_host_marshal_from_guest! {
    u8, u16, u32, i8, i16, i32, u64, i64, char, bool, LeI16, LeU16, LeI32, LeU32, LeI64, LeU64,
    WasmStr
}

impl<T> HostMarshaledTyBase for WasmPtr<T> {
    impl_host_marshal_base_from_guest!();
}

impl<D, T> HostMarshaledTy<D> for WasmPtr<T> {
    impl_host_marshal_from_guest!(D);
}

impl<T> HostMarshaledTyBase for WasmSlice<T> {
    impl_host_marshal_base_from_guest!();
}

impl<D, T> HostMarshaledTy<D> for WasmSlice<T> {
    impl_host_marshal_from_guest!(D);
}

// === HostMarshaledTyList === //

pub trait HostMarshaledTyListBase: Sized {
    type HostPrims: wasmtime::WasmRet + wasmtime::WasmResults + wasmtime::WasmParams;
}

pub trait HostMarshaledTyList<D>: HostMarshaledTyListBase {
    fn into_host_prims(me: Self) -> Self::HostPrims;

    fn from_host_prims(
        cx: impl wasmtime::AsContextMut<Data = D>,
        me: Self::HostPrims,
    ) -> anyhow::Result<Self>;
}

impl<T: HostMarshaledTyBase> HostMarshaledTyListBase for T {
    type HostPrims = T::HostPrim;
}

// impl<D, T: HostMarshaledTy<D>> HostMarshaledTyList<D> for T {
//     fn into_host_prims(me: Self) -> Self::HostPrims {
//         T::into_host_prim(me)
//     }
//
//     fn from_host_prims(
//         cx: impl wasmtime::AsContextMut<Data = D>,
//         me: Self::HostPrims,
//     ) -> anyhow::Result<Self> {
//         T::from_host_prim(cx, me)
//     }
// }

macro_rules! impl_marshaled_res_ty {
    ($($para:ident)*) => {
        impl<$($para: HostMarshaledTyBase,)*> HostMarshaledTyListBase for ($($para,)*) {
            type HostPrims = ($(<$para as HostMarshaledTyBase>::HostPrim,)*);
        }

        impl<D, $($para: HostMarshaledTy<D>,)*> HostMarshaledTyList<D> for ($($para,)*) {
            #[allow(clippy::unused_unit, non_snake_case)]
            fn into_host_prims(($($para,)*): Self) -> Self::HostPrims {
                ( $(HostMarshaledTy::into_host_prim($para),)* )
            }

            #[allow(non_snake_case, unused)]
            fn from_host_prims(
                mut cx: impl wasmtime::AsContextMut<Data = D>,
                ($($para,)*): Self::HostPrims,
            ) -> anyhow::Result<Self> {
                Ok(($(
                    HostMarshaledTy::from_host_prim(&mut cx, $para)
                        .with_context(|| format!(
                            "failed to decode parameter of type {}",
                            std::any::type_name::<$para>(),
                        ))?,
                )*))
            }
        }
    };
}

impl_variadic!(impl_marshaled_res_ty);

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

pub const fn align_of_32<T>() -> u32 {
    struct AlignOf<T>(PhantomData<fn() -> T>);

    impl<T> AlignOf<T> {
        const SIZE: u32 = {
            let size = std::mem::align_of::<T>();
            if size > u32::MAX as usize {
                panic!("structure is too big");
            }

            size as u32
        };
    }

    <AlignOf<T>>::SIZE
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

    fn write_struct<T: Pod>(&mut self, base: WasmPtr<T>, data: &T) -> anyhow::Result<()> {
        self.write_range_mut(base.addr.get(), bytemuck::bytes_of(data))
    }

    fn write_slice<'a, T: Pod>(
        &mut self,
        base: WasmPtr<T>,
        items: impl IntoIterator<Item = &'a T>,
    ) -> anyhow::Result<u32> {
        let mut offset = base.addr.get();
        let mut count = 0;

        for item in items {
            self.write_struct(
                WasmPtr {
                    _ty: PhantomData,
                    addr: offset.into(),
                },
                item,
            )?;

            offset = offset
                .checked_add(size_of_32::<T>())
                .context("wrote too many elements into memory")?;

            count += 1;
        }

        Ok(count)
    }
}

impl MemoryWrite for [u8] {
    fn as_slice_mut(&mut self) -> &mut [u8] {
        self
    }
}

// === Host-Side Function Handling === //

// HostSideMarshaledFunc
pub trait HostSideMarshaledFunc<D, Params, Results>: Sized {
    type PrimParams<'a>;
    type PrimResults;

    #[rustfmt::skip]
    fn wrap_host(self) ->
        impl for<'a> wasmtime::IntoFunc<D, Self::PrimParams<'a>, Self::PrimResults>;
}

macro_rules! impl_func_ty {
    ($($ty:ident)*) => {
        impl<D, F, Ret, $($ty: HostMarshaledTy<D>,)*> HostSideMarshaledFunc<D, ($($ty,)*), Ret> for F
        where
            D: 'static,
            Ret: HostMarshaledTyList<D>,
            F: 'static + Send + Sync + Fn(wasmtime::Caller<'_, D>, $($ty,)*) -> anyhow::Result<Ret>,
        {
            type PrimParams<'a> = (wasmtime::Caller<'a, D>, $(<$ty as HostMarshaledTyBase>::HostPrim,)*);
            type PrimResults = anyhow::Result<Ret::HostPrims>;

            #[allow(non_snake_case, unused_mut)]
            fn wrap_host(self) -> impl for<'a> wasmtime::IntoFunc<D, Self::PrimParams<'a>, Self::PrimResults> {
                move |mut caller: wasmtime::Caller<'_, D>, $($ty: <$ty as HostMarshaledTyBase>::HostPrim,)*| {
                    $(let $ty = <$ty>::from_host_prim(&mut caller, $ty).context("failed to parse argument")?;)*

                    self(caller, $($ty),*)
                        .map(HostMarshaledTyList::into_host_prims)
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

pub struct WasmFuncOnHost<A, R>
where
    A: HostMarshaledTyListBase,
    R: HostMarshaledTyListBase,
{
    pub index: u32,
    pub func: wasmtime::TypedFunc<A::HostPrims, R::HostPrims>,
}

impl<A, R> Copy for WasmFuncOnHost<A, R>
where
    A: HostMarshaledTyListBase,
    R: HostMarshaledTyListBase,
{
}

impl<A, R> Clone for WasmFuncOnHost<A, R>
where
    A: HostMarshaledTyListBase,
    R: HostMarshaledTyListBase,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<A, R> WasmFuncOnHost<A, R>
where
    A: HostMarshaledTyListBase,
    R: HostMarshaledTyListBase,
{
    pub fn call<D>(
        &self,
        mut store: impl wasmtime::AsContextMut<Data = D>,
        args: A,
    ) -> anyhow::Result<R>
    where
        A: HostMarshaledTyList<D>,
        R: HostMarshaledTyList<D>,
    {
        let res = self.func.call(&mut store, A::into_host_prims(args))?;
        R::from_host_prims(&mut store, res).context("failed to deserialize results")
    }
}

impl<A, R> HostMarshaledTyBase for WasmFuncOnHost<A, R>
where
    A: HostMarshaledTyListBase,
    R: HostMarshaledTyListBase,
{
    type HostPrim = u32;
}

impl<D, A, R> HostMarshaledTy<D> for WasmFuncOnHost<A, R>
where
    D: StoreHasTable,
    A: HostMarshaledTyList<D>,
    R: HostMarshaledTyList<D>,
{
    fn into_host_prim(me: Self) -> Self::HostPrim {
        me.index
    }

    fn from_host_prim(
        mut cx: impl wasmtime::AsContextMut<Data = D>,
        me: Self::HostPrim,
    ) -> anyhow::Result<Self> {
        let table = cx.as_context().data().func_table();
        let func = table
            .get(&mut cx, me)
            .with_context(|| format!("failed to resolve table entry with index {me}"))?;

        let func = func
            .funcref()
            .flatten()
            .context("entry is not a `funcref`")?;

        Ok(Self {
            index: me,
            func: func.typed(&cx).context("func has wrong type")?,
        })
    }
}

// === StoreHasTable === //

pub trait StoreHasTable {
    fn func_table(&self) -> wasmtime::Table;
}

// === StoreHasMemory === //

pub trait StoreHasMemory {
    fn main_memory(&self) -> wasmtime::Memory;

    fn alloc_func(&self) -> WasmFuncOnHost<(u32, u32), (WasmPtr<()>,)>;
}

pub trait ContextMemoryExt: Sized + wasmtime::AsContextMut<Data = Self::Data_> {
    type Data_: StoreHasMemory;

    fn main_memory(&mut self) -> (&mut [u8], &mut Self::Data_) {
        let memory = self.as_context_mut().data().main_memory();
        memory.data_and_store_mut(self)
    }

    fn alloc(&mut self, size: u32, align: u32) -> anyhow::Result<WasmPtr<()>> {
        let alloc = self.as_context_mut().data().alloc_func();
        alloc.call(self, (size, align)).map(|v| v.0)
    }

    fn alloc_struct<T: Pod>(&mut self, value: &T) -> anyhow::Result<WasmPtr<T>> {
        let ptr = self
            .alloc(size_of_32::<T>(), align_of_32::<T>())
            .map(|v| WasmPtr::<T> {
                _ty: PhantomData,
                addr: v.addr,
            })?;

        let (memory, _) = self.main_memory();
        memory.write_struct(ptr, value)?;
        Ok(ptr)
    }

    fn alloc_slice<'a, T: Pod>(
        &mut self,
        values: impl ExactSizeIterator<Item = &'a T>,
    ) -> anyhow::Result<WasmSlice<T>> {
        let len = u32::try_from(values.len()).context("too many elements in slice")?;
        let size = size_of_32::<T>()
            .checked_mul(len)
            .context("Slice is too big")?;

        let base = self.alloc(size, align_of_32::<T>()).map(|v| WasmPtr::<T> {
            _ty: PhantomData,
            addr: v.addr,
        })?;

        let (memory, _) = self.main_memory();
        memory.write_slice(base, values)?;

        Ok(WasmSlice {
            base,
            len: len.into(),
        })
    }

    fn alloc_str(&mut self, data: &str) -> anyhow::Result<WasmStr> {
        self.alloc_slice(data.as_bytes().iter()).map(WasmStr)
    }
}

impl<T: wasmtime::AsContextMut> ContextMemoryExt for T
where
    T::Data: StoreHasMemory,
{
    type Data_ = T::Data;
}
