use std::{fmt, marker::PhantomData, mem, ops::Range, ptr, slice};

use anyhow::Context;
use bytemuck::{Pod, TransparentWrapper, Zeroable};
use derive_where::derive_where;

use crate::utils::{guest_usize_to_u32, is_wasm, size_of_u32};

// === Marshal Trait === //

pub type StrategyOf<M> = <M as Marshal>::Strategy;
pub type GuestboundOf<M> = <StrategyOf<M> as Strategy>::Guestbound;
pub type HostboundOf<'a, M> = <StrategyOf<M> as Strategy>::Hostbound<'a>;
pub type HostboundViewOf<M> = <StrategyOf<M> as Strategy>::HostboundView;
pub type GuestboundViewOf<'a, M> = <StrategyOf<M> as Strategy>::GuestboundView<'a>;

pub trait HostContext: Sized {
    fn guest_memory(&self) -> &[u8];

    fn guest_memory_mut(&mut self) -> &mut [u8];

    fn alloc(&mut self, align: u32, size: u32) -> anyhow::Result<FfiPtr<()>>;

    fn invoke(&mut self, id: u64, boxed_arg: u32) -> anyhow::Result<()>;

    fn read_slice<T: Pod>(&self, base: u32, len: u32) -> anyhow::Result<&[T]> {
        if mem::size_of::<T>() == 0 {
            return Ok(unsafe { slice::from_raw_parts(ptr::dangling::<T>(), len as usize) });
        }

        let bytes = self
            .guest_memory()
            .get(base as usize..)
            .context("memory base address too large")?
            .get(
                ..mem::size_of::<T>()
                    .checked_mul(len as usize)
                    .context("arithmetic overflow during addressing")?,
            )
            .context("read past bounds of memory")?;

        let bytes = bytemuck::try_cast_slice::<u8, T>(bytes)
            .ok()
            .context("failed to convert byte view to POD slice")?;

        Ok(bytes)
    }

    fn write_slice<T: Pod>(&mut self, base: u32, len: u32) -> anyhow::Result<&mut [T]> {
        if mem::size_of::<T>() == 0 {
            return Ok(unsafe {
                slice::from_raw_parts_mut(ptr::dangling_mut::<T>(), len as usize)
            });
        }

        let bytes = self
            .guest_memory_mut()
            .get_mut(base as usize..)
            .context("memory base address too large")?
            .get_mut(
                ..mem::size_of::<T>()
                    .checked_mul(len as usize)
                    .context("arithmetic overflow during addressing")?,
            )
            .context("read past bounds of memory")?;

        let bytes = bytemuck::try_cast_slice_mut::<u8, T>(bytes)
            .ok()
            .context("failed to convert byte view to POD slice")?;

        Ok(bytes)
    }

    fn read_array<T: Pod, const N: usize>(&self, addr: u32) -> anyhow::Result<&[T; N]> {
        self.read_slice(addr, N as u32)
            .map(|v| &bytemuck::cast_slice::<T, [T; N]>(v)[0])
    }

    fn write_array<T: Pod, const N: usize>(&mut self, addr: u32) -> anyhow::Result<&mut [T; N]> {
        self.write_slice(addr, N as u32)
            .map(|v| &mut bytemuck::cast_slice_mut::<T, [T; N]>(v)[0])
    }
}

pub trait Marshal: Sized + 'static {
    type Strategy: Strategy;
}

pub trait Strategy: Sized + 'static + Marshal<Strategy = Self> {
    /// An FFI-safe representation of the value written by the guest into its own memory and
    /// interpreted on the host.
    type Hostbound<'a>;

    /// A view of a hostbound value on the host.
    type HostboundView;

    /// An FFI-safe representation of the value written by the host into the guest's memory and
    /// interpreted by the guest.
    type Guestbound;

    /// A view of a guestbound value to be encoded by host.
    type GuestboundView<'a>;

    fn decode_hostbound(
        cx: &impl HostContext,
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView>;

    fn encode_guestbound(
        cx: &mut impl HostContext,
        out_ptr: FfiPtr<Self::Guestbound>,
        value: &Self::GuestboundView<'_>,
    ) -> anyhow::Result<()>;
}

pub trait UnifiedStrategy:
    for<'a> Strategy<
        Hostbound<'a> = Self::Unified,
        HostboundView = Self::Unified,
        Guestbound = Self::Unified,
        GuestboundView<'a> = Self::Unified,
    >
{
    type Unified;
}

impl<U, T> UnifiedStrategy for T
where
    T: for<'a> Strategy<
            Hostbound<'a> = U,
            HostboundView = U,
            Guestbound = U,
            GuestboundView<'a> = U,
        >,
{
    type Unified = U;
}

// === FfiIndex === //

pub trait FfiIndex<S: FfiSliceIndexable>: Sized + fmt::Debug {
    type Output;

    fn try_get(&self, slice: S) -> Option<Self::Output>;
}

pub trait FfiSliceIndexable: Sized + Copy {
    type RawElem;
    type RichPtr;

    fn unwrap_slice(me: Self) -> FfiSlice<Self::RawElem>;

    fn wrap_slice(slice: FfiSlice<Self::RawElem>) -> Self;

    fn wrap_ptr(ptr: FfiPtr<Self::RawElem>) -> Self::RichPtr;

    fn try_get<I: FfiIndex<Self>>(self, index: I) -> Option<I::Output> {
        index.try_get(self)
    }

    fn get<I: FfiIndex<Self>>(self, index: I) -> I::Output {
        index.try_get(self).unwrap_or_else(|| {
            panic!(
                "index {index:?} out of bounds for slice of length {}",
                Self::unwrap_slice(self).len()
            )
        })
    }
}

impl<S: FfiSliceIndexable> FfiIndex<S> for u32 {
    type Output = S::RichPtr;

    fn try_get(&self, slice: S) -> Option<Self::Output> {
        let slice = S::unwrap_slice(slice);

        if *self < slice.len() {
            Some(S::wrap_ptr(slice.base().add(*self)))
        } else {
            None
        }
    }
}

impl<S: FfiSliceIndexable> FfiIndex<S> for Range<u32> {
    type Output = S;

    fn try_get(&self, slice: S) -> Option<Self::Output> {
        let slice = S::unwrap_slice(slice);

        if self.start < slice.len() && self.end <= slice.len() && self.start <= self.end {
            Some(S::wrap_slice(FfiSlice::new(
                slice.base().add(self.start),
                self.end - self.start,
            )))
        } else {
            None
        }
    }
}

// === FFI Types === //

#[derive(TransparentWrapper)]
#[derive_where(Copy, Clone, Hash, Eq, PartialEq)]
#[repr(transparent)]
#[transparent(u32)]
pub struct FfiPtr<T> {
    _ty: PhantomData<fn(T) -> T>,
    addr: u32,
}

unsafe impl<T: 'static> Pod for FfiPtr<T> {}

unsafe impl<T: 'static> Zeroable for FfiPtr<T> {}

impl<T> FfiPtr<T> {
    pub const fn new(addr: u32) -> Self {
        Self {
            _ty: PhantomData,
            addr,
        }
    }

    pub fn new_guest(ptr: *const T) -> Self {
        Self::new(guest_usize_to_u32(ptr as usize))
    }

    pub const fn guest_ptr(self) -> *mut T {
        cfgenius::cond! {
            if macro(is_wasm) {
                self.addr as usize as *mut T
            } else {
                unimplemented!()
            }
        }
    }

    pub const fn addr(self) -> u32 {
        self.addr
    }

    pub const fn cast<V>(self) -> FfiPtr<V> {
        FfiPtr::new(self.addr())
    }

    pub const fn field<V>(self, field: FfiOffset<T, V>) -> FfiPtr<V> {
        FfiPtr::new(self.addr + field.get())
    }

    pub const fn add(self, cnt: u32) -> FfiPtr<T> {
        Self::new(self.addr() + cnt * size_of_u32::<T>())
    }

    pub const fn sub(self, cnt: u32) -> FfiPtr<T> {
        Self::new(self.addr() - cnt * size_of_u32::<T>())
    }
}

impl<T: Pod> FfiPtr<T> {
    pub fn read(self, cx: &impl HostContext) -> anyhow::Result<&T> {
        let [value] = cx.read_array(self.addr())?;
        Ok(value)
    }

    pub fn write(self, cx: &mut impl HostContext) -> anyhow::Result<&mut T> {
        let [value] = cx.write_array(self.addr())?;
        Ok(value)
    }
}

#[derive_where(Copy, Clone, Hash, Eq, PartialEq)]
#[repr(C)]
pub struct FfiSlice<T> {
    _ty: PhantomData<fn(T) -> T>,
    base: FfiPtr<T>,
    len: u32,
}

unsafe impl<T: 'static> Pod for FfiSlice<T> {}

unsafe impl<T: 'static> Zeroable for FfiSlice<T> {}

impl<T> FfiSlice<T> {
    pub const fn new(base: FfiPtr<T>, len: u32) -> Self {
        Self {
            _ty: PhantomData,
            base,
            len,
        }
    }

    pub fn new_guest(ptr: *const [T]) -> Self {
        Self::new(
            FfiPtr::new_guest(ptr as *const T),
            guest_usize_to_u32(ptr.len()),
        )
    }

    pub const fn guest_ptr(self) -> *mut [T] {
        cfgenius::cond! {
            if macro(is_wasm) {
                std::ptr::slice_from_raw_parts_mut(self.base.guest_ptr(), self.len as usize)
            } else {
                unimplemented!()
            }
        }
    }

    pub const fn base(self) -> FfiPtr<T> {
        self.base
    }

    pub const fn len(self) -> u32 {
        self.len
    }

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    pub const fn pack(self) -> u64 {
        self.base.addr as u64 | ((self.len as u64) << 32)
    }

    pub const fn unpack(val: u64) -> Self {
        Self::new(FfiPtr::new(val as u32), (val >> 32) as u32)
    }
}

impl<T> FfiSliceIndexable for FfiSlice<T> {
    type RawElem = T;
    type RichPtr = FfiPtr<T>;

    fn unwrap_slice(me: Self) -> FfiSlice<Self::RawElem> {
        me
    }

    fn wrap_slice(slice: FfiSlice<Self::RawElem>) -> Self {
        slice
    }

    fn wrap_ptr(ptr: FfiPtr<Self::RawElem>) -> Self::RichPtr {
        ptr
    }
}

impl<T: Pod> FfiSlice<T> {
    pub fn read(self, cx: &impl HostContext) -> anyhow::Result<&[T]> {
        cx.read_slice(self.base().addr(), self.len())
    }

    pub fn write(self, cx: &mut impl HostContext) -> anyhow::Result<&mut [T]> {
        cx.write_slice(self.base().addr(), self.len())
    }
}

impl<T> Iterator for FfiSlice<T> {
    type Item = FfiPtr<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_empty() {
            return None;
        }

        let ptr = self.base;
        self.base = self.base.add(1);
        self.len -= 1;

        Some(ptr)
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct FfiOffset<S, T> {
    _ty: PhantomData<fn() -> (S, T)>,
    offset: u32,
}

impl<S, T> FfiOffset<S, T> {
    pub const fn new(offset: u32) -> Self {
        Self {
            _ty: PhantomData,
            offset,
        }
    }

    pub const fn get(self) -> u32 {
        self.offset
    }
}

#[doc(hidden)]
pub mod ffi_offset_internals {
    pub use std::mem::offset_of;

    pub const fn make_ffi_offset<S, T>(
        offset: usize,
        _ty_assert: fn(&S) -> &T,
    ) -> super::FfiOffset<S, T> {
        if offset as u64 > u32::MAX as u64 {
            panic!("field offset is too large for guest")
        }

        super::FfiOffset::new(offset as u32)
    }
}

#[macro_export]
macro_rules! ffi_offset {
    ($ty:ty, $($path:tt)*) => {
        const {
            $crate::ffi_offset_internals::make_ffi_offset::<$ty, _>(
                $crate::ffi_offset_internals::offset_of!($ty, $($path)*),
                |me| &me.$($path)*
            )
        }
    };
}
