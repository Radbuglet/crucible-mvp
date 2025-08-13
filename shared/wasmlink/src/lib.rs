#![allow(clippy::missing_safety_doc)]

use std::{
    fmt,
    marker::PhantomData,
    mem::{self, MaybeUninit},
    ops::{Deref, Range},
    ptr, slice,
};

use anyhow::Context;
use bytemuck::{Pod, TransparentWrapper, Zeroable};
use derive_where::derive_where;

// === Helpers === //

cfgenius::define! {
    is_wasm = cfg(target_arch = "wasm32");
}

pub const fn align_of_u32<T>() -> u32 {
    const {
        let align = mem::align_of::<T>() as u64;

        if align > u32::MAX as u64 {
            panic!("alignment is too large for guest")
        }

        align as u32
    }
}

pub const fn size_of_u32<T>() -> u32 {
    const {
        let align = mem::size_of::<T>() as u64;

        if align > u32::MAX as u64 {
            panic!("size is too large for guest")
        }

        align as u32
    }
}

pub const fn guest_usize_to_u32(val: usize) -> u32 {
    cfgenius::cond! {
        if macro(is_wasm) {
            val as u32
        } else {
            _ = val;

            unimplemented!();
        }
    }
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

// === PodMarshal === //

pub struct PodMarshal<T: Pod> {
    _ty: PhantomData<fn(T) -> T>,
}

impl<T: Pod> Marshal for PodMarshal<T> {
    type Strategy = Self;
}

impl<T: Pod> Strategy for PodMarshal<T> {
    type Hostbound<'a> = T;
    type HostboundView = T;
    type Guestbound = T;
    type GuestboundView<'a> = T;

    fn decode_hostbound(
        cx: &impl HostContext,
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        ptr.read(cx).copied()
    }

    fn encode_guestbound(
        cx: &mut impl HostContext,
        out_ptr: FfiPtr<Self::Guestbound>,
        value: &Self::GuestboundView<'_>,
    ) -> anyhow::Result<()> {
        *out_ptr.write(cx)? = *value;

        Ok(())
    }
}

macro_rules! alias_pod_marshal {
    ( $($ty:ty),*$(,)? ) => {$(
        impl Marshal for $ty {
            type Strategy = PodMarshal<$ty>;
        }
    )*};
}

alias_pod_marshal! {
    (),
    u8,
    u16,
    u32,
    u64,
    u128,
    i8,
    i16,
    i32,
    i64,
    i128,
    f32,
    f64,
}

// === ConvertMarshal === //

/// ## Safety
///
/// All values of `Self` must be directly transmutable to `Self::Raw`'s `Unified` type.
///
pub unsafe trait ConvertMarshal: Sized + 'static {
    type Raw: UnifiedStrategy;

    fn try_from_raw(raw: <Self::Raw as UnifiedStrategy>::Unified) -> anyhow::Result<Self>;
}

pub struct ConvertMarshalStrategy<T: ConvertMarshal> {
    _ty: PhantomData<fn(T) -> T>,
}

impl<T: ConvertMarshal> Marshal for ConvertMarshalStrategy<T> {
    type Strategy = Self;
}

impl<T: ConvertMarshal> Strategy for ConvertMarshalStrategy<T> {
    type Hostbound<'a> = T;
    type HostboundView = T;
    type Guestbound = T;
    type GuestboundView<'a> = T;

    fn decode_hostbound(
        cx: &impl HostContext,
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        T::try_from_raw(T::Raw::decode_hostbound(
            cx,
            ptr.cast::<<T::Raw as UnifiedStrategy>::Unified>(),
        )?)
    }

    fn encode_guestbound(
        cx: &mut impl HostContext,
        out_ptr: FfiPtr<Self::Guestbound>,
        value: &Self::GuestboundView<'_>,
    ) -> anyhow::Result<()> {
        T::Raw::encode_guestbound(
            cx,
            out_ptr.cast::<<T::Raw as UnifiedStrategy>::Unified>(),
            unsafe { &*(value as *const T as *const <T::Raw as UnifiedStrategy>::Unified) },
        )
    }
}

// === bool and char === //

impl Marshal for bool {
    type Strategy = ConvertMarshalStrategy<Self>;
}

unsafe impl ConvertMarshal for bool {
    type Raw = StrategyOf<u8>;

    fn try_from_raw(raw: <Self::Raw as UnifiedStrategy>::Unified) -> anyhow::Result<Self> {
        match raw {
            0 => Ok(false),
            1 => Ok(true),
            _ => anyhow::bail!("boolean was neither `0` nor `1`"),
        }
    }
}

impl Marshal for char {
    type Strategy = ConvertMarshalStrategy<Self>;
}

unsafe impl ConvertMarshal for char {
    type Raw = StrategyOf<u32>;

    fn try_from_raw(raw: <Self::Raw as UnifiedStrategy>::Unified) -> anyhow::Result<Self> {
        char::try_from(raw).context("invalid unicode codepoint")
    }
}

// === Option === //

// Views
#[repr(C)]
pub struct FfiOption<T> {
    present: bool,
    value: MaybeUninit<T>,
}

impl<T> FfiOption<T> {
    pub fn new(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::some(value),
            None => Self::none(),
        }
    }

    pub const fn some(value: T) -> Self {
        Self {
            present: true,
            value: MaybeUninit::new(value),
        }
    }

    pub const fn none() -> Self {
        Self {
            present: false,
            value: MaybeUninit::uninit(),
        }
    }

    pub const fn decode(self) -> Option<T> {
        match self.present {
            true => Some(unsafe { MaybeUninit::assume_init(self.value) }),
            false => None,
        }
    }

    pub const fn decode_ref(&self) -> Option<&T> {
        match self.present {
            true => Some(unsafe { MaybeUninit::assume_init_ref(&self.value) }),
            false => None,
        }
    }

    pub const fn decode_mut(&mut self) -> Option<&mut T> {
        match self.present {
            true => Some(unsafe { MaybeUninit::assume_init_mut(&mut self.value) }),
            false => None,
        }
    }
}

impl<T> From<Option<T>> for FfiOption<T> {
    fn from(value: Option<T>) -> Self {
        Self::new(value)
    }
}

// Strategy
impl<T: Marshal> Marshal for Option<T> {
    type Strategy = Option<T::Strategy>;
}

impl<T: Strategy> Strategy for Option<T> {
    type Hostbound<'a> = FfiOption<T::Hostbound<'a>>;
    type HostboundView = Option<T::HostboundView>;
    type Guestbound = FfiOption<T::Guestbound>;
    type GuestboundView<'a> = Option<T::GuestboundView<'a>>;

    fn decode_hostbound(
        cx: &impl HostContext,
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        let is_present = *ptr
            .field(ffi_offset!(FfiOption<T::Hostbound<'static>>, present))
            .cast::<u8>()
            .read(cx)?;

        if is_present == 0 {
            return Ok(None);
        }

        let value = T::decode_hostbound(
            cx,
            ptr.field(ffi_offset!(FfiOption<T::Hostbound<'static>>, value))
                .cast(),
        )?;

        Ok(Some(value))
    }

    fn encode_guestbound(
        cx: &mut impl HostContext,
        out_ptr: FfiPtr<Self::Guestbound>,
        value: &Self::GuestboundView<'_>,
    ) -> anyhow::Result<()> {
        *out_ptr
            .field(ffi_offset!(FfiOption<T::Guestbound>, present))
            .cast::<u8>()
            .write(cx)? = value.is_some() as u8;

        if let Some(value) = value {
            T::encode_guestbound(
                cx,
                out_ptr
                    .field(ffi_offset!(FfiOption<T::Guestbound>, value))
                    .cast(),
                value,
            )?;
        }

        Ok(())
    }
}

// === Box === //

// Views
pub type HostPtr<I> = HostPtr_<StrategyOf<I>>;

#[derive_where(Copy, Clone, Hash, Eq, PartialEq)]
#[repr(transparent)]
pub struct HostPtr_<T: Strategy> {
    ptr: FfiPtr<T::Hostbound<'static>>,
}

impl<T: Strategy> HostPtr_<T> {
    pub const fn new(ptr: FfiPtr<T::Hostbound<'static>>) -> Self {
        Self { ptr }
    }

    pub const fn ptr(self) -> FfiPtr<T::Hostbound<'static>> {
        self.ptr
    }

    pub fn decode(self, cx: &impl HostContext) -> anyhow::Result<T::HostboundView> {
        T::decode_hostbound(cx, self.ptr)
    }
}

// Strategy
impl<T: Marshal> Marshal for Box<T> {
    type Strategy = Box<T::Strategy>;
}

impl<T: Strategy> Strategy for Box<T> {
    type Hostbound<'a> = &'a T::Hostbound<'a>;
    type HostboundView = HostPtr_<T>;
    type Guestbound = Box<T::Guestbound>;
    type GuestboundView<'a> = &'a T::GuestboundView<'a>;

    fn decode_hostbound(
        cx: &impl HostContext,
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        Ok(HostPtr_ {
            ptr: *ptr.cast::<FfiPtr<T::Hostbound<'static>>>().read(cx)?,
        })
    }

    fn encode_guestbound(
        cx: &mut impl HostContext,
        out_ptr: FfiPtr<Self::Guestbound>,
        value: &Self::GuestboundView<'_>,
    ) -> anyhow::Result<()> {
        let allocated = cx
            .alloc(
                align_of_u32::<T::Guestbound>(),
                size_of_u32::<T::Guestbound>(),
            )?
            .cast::<T::Guestbound>();

        T::encode_guestbound(cx, allocated, value)?;

        *out_ptr.cast::<FfiPtr<T::Guestbound>>().write(cx)? = allocated;

        Ok(())
    }
}

// === Vec === //

// Views
#[repr(transparent)]
pub struct GuestSlice<T> {
    _ty: PhantomData<Vec<T>>,
    slice: FfiSlice<T>,
}

impl<T> GuestSlice<T> {
    pub unsafe fn new_unchecked(slice: FfiSlice<T>) -> Self {
        Self {
            _ty: PhantomData,
            slice,
        }
    }

    pub fn decode(self) -> Vec<T> {
        unsafe {
            Vec::from_raw_parts(
                self.slice.guest_ptr().cast(),
                self.slice.guest_ptr().len(),
                self.slice.guest_ptr().len(),
            )
        }
    }

    pub fn decode_ref(&self) -> &[T] {
        unsafe { &*self.slice.guest_ptr() }
    }

    pub fn decode_mut(&mut self) -> &mut [T] {
        unsafe { &mut *self.slice.guest_ptr() }
    }
}

#[derive_where(Copy, Clone, Hash, Eq, PartialEq)]
#[repr(transparent)]
pub struct GuestSliceRef<'a, T> {
    _ty: PhantomData<&'a [T]>,
    slice: FfiSlice<T>,
}

impl<'a, T> GuestSliceRef<'a, T> {
    pub fn new(values: &'a [T]) -> Self {
        Self {
            _ty: PhantomData,
            slice: FfiSlice::new_guest(values),
        }
    }

    pub fn decode(self) -> &'a [T] {
        unsafe { &*self.slice.guest_ptr() }
    }
}

impl<'a, T> From<&'a [T]> for GuestSliceRef<'a, T> {
    fn from(values: &'a [T]) -> Self {
        Self::new(values)
    }
}

pub type HostSlice<T> = HostSlice_<StrategyOf<T>>;

#[derive_where(Copy, Clone, Hash, Eq, PartialEq)]
#[repr(transparent)]
pub struct HostSlice_<T: Strategy> {
    slice: FfiSlice<T::Hostbound<'static>>,
}

impl<T: Strategy> HostSlice_<T> {
    pub const fn new(slice: FfiSlice<T::Hostbound<'static>>) -> Self {
        Self { slice }
    }

    pub const fn slice(self) -> FfiSlice<T::Hostbound<'static>> {
        self.slice
    }

    pub const fn is_empty(self) -> bool {
        self.slice.is_empty()
    }

    pub const fn len(self) -> u32 {
        self.slice.len()
    }
}

impl<T: Strategy> Iterator for HostSlice_<T> {
    type Item = HostPtr_<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.slice.next().map(HostPtr_::new)
    }
}

impl<T: Strategy> FfiSliceIndexable for HostSlice_<T> {
    type RawElem = T::Hostbound<'static>;
    type RichPtr = HostPtr_<T>;

    fn unwrap_slice(me: Self) -> FfiSlice<Self::RawElem> {
        me.slice()
    }

    fn wrap_slice(slice: FfiSlice<Self::RawElem>) -> Self {
        Self::new(slice)
    }

    fn wrap_ptr(ptr: FfiPtr<Self::RawElem>) -> Self::RichPtr {
        HostPtr_::new(ptr)
    }
}

// Strategy
impl<T: Marshal> Marshal for Vec<T> {
    type Strategy = Vec<T::Strategy>;
}

impl<T: Strategy> Strategy for Vec<T> {
    type Hostbound<'a> = GuestSliceRef<'a, T::Hostbound<'a>>;
    type HostboundView = HostSlice_<T>;
    type Guestbound = GuestSlice<T::Guestbound>;
    type GuestboundView<'a> = &'a [T::GuestboundView<'a>];

    fn decode_hostbound(
        cx: &impl HostContext,
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        Ok(HostSlice_::new(
            *ptr.cast::<FfiSlice<T::Hostbound<'static>>>().read(cx)?,
        ))
    }

    fn encode_guestbound(
        cx: &mut impl HostContext,
        out_ptr: FfiPtr<Self::Guestbound>,
        value: &Self::GuestboundView<'_>,
    ) -> anyhow::Result<()> {
        let allocated = cx.alloc(
            align_of_u32::<T::Guestbound>(),
            size_of_u32::<T::Guestbound>()
                .checked_mul(u32::try_from(value.len()).expect("too many elements"))
                .expect("slice too big for guest"),
        )?;

        let allocated = FfiSlice::<T::Guestbound>::new(allocated.cast(), value.len() as u32);

        for (value, out_ptr) in value.iter().zip(allocated) {
            T::encode_guestbound(cx, out_ptr, value)?;
        }

        *out_ptr.cast::<FfiSlice<T::Guestbound>>().write(cx)? = allocated;

        Ok(())
    }
}

// === String === //

// Views
#[repr(transparent)]
pub struct GuestStr {
    _ty: PhantomData<String>,
    slice: FfiSlice<u8>,
}

impl GuestStr {
    pub unsafe fn new_unchecked(slice: FfiSlice<u8>) -> Self {
        Self {
            _ty: PhantomData,
            slice,
        }
    }

    pub fn decode(self) -> String {
        unsafe {
            String::from_raw_parts(
                self.slice.guest_ptr().cast(),
                self.slice.guest_ptr().len(),
                self.slice.guest_ptr().len(),
            )
        }
    }

    pub fn decode_ref(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(&*self.slice.guest_ptr()) }
    }

    pub fn decode_mut(&mut self) -> &mut str {
        unsafe { std::str::from_utf8_unchecked_mut(&mut *self.slice.guest_ptr()) }
    }
}

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
#[repr(transparent)]
pub struct GuestStrRef<'a> {
    _ty: PhantomData<&'a str>,
    slice: FfiSlice<u8>,
}

impl<'a> GuestStrRef<'a> {
    pub fn new(values: &'a str) -> Self {
        Self {
            _ty: PhantomData,
            slice: FfiSlice::new_guest(values.as_bytes()),
        }
    }

    pub fn decode(self) -> &'a str {
        unsafe { std::str::from_utf8_unchecked(&*self.slice.guest_ptr()) }
    }
}

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
#[repr(transparent)]
pub struct HostStr {
    slice: FfiSlice<u8>,
}

impl HostStr {
    pub const fn new(slice: FfiSlice<u8>) -> Self {
        Self { slice }
    }

    pub const fn slice(self) -> FfiSlice<u8> {
        self.slice
    }

    pub const fn is_empty(self) -> bool {
        self.slice.is_empty()
    }

    pub const fn len(self) -> u32 {
        self.slice.len()
    }

    pub fn read(self, cx: &impl HostContext) -> anyhow::Result<&str> {
        std::str::from_utf8(self.slice.read(cx)?).context("invalid UTF-8 sequence")
    }
}

// Strategy
impl Marshal for String {
    type Strategy = Self;
}

impl Strategy for String {
    type Hostbound<'a> = GuestStrRef<'a>;
    type HostboundView = HostStr;
    type Guestbound = GuestStr;
    type GuestboundView<'a> = &'a str;

    fn decode_hostbound(
        cx: &impl HostContext,
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        Ok(HostStr::new(*ptr.cast::<FfiSlice<u8>>().read(cx)?))
    }

    fn encode_guestbound(
        cx: &mut impl HostContext,
        out_ptr: FfiPtr<Self::Guestbound>,
        value: &Self::GuestboundView<'_>,
    ) -> anyhow::Result<()> {
        let allocated = cx.alloc(1, u32::try_from(value.len()).expect("string too long"))?;

        let allocated = FfiSlice::<u8>::new(allocated.cast(), value.len() as u32);

        allocated.write(cx)?.copy_from_slice(value.as_bytes());

        *out_ptr.cast::<FfiSlice<u8>>().write(cx)? = allocated;

        Ok(())
    }
}

// === Struct Marshalling === //

// VariantSelector
pub trait VariantSelector: Sized {
    type Output<T: Strategy>;
}

#[non_exhaustive]
pub struct MarkerVariant;

const _: () = {
    pub enum Never {}

    impl VariantSelector for MarkerVariant {
        type Output<T: Strategy> = Never;
    }
};

pub struct HostboundVariant<'a> {
    _ty: PhantomData<&'a ()>,
}

impl<'a> VariantSelector for HostboundVariant<'a> {
    type Output<T: Strategy> = T::Hostbound<'a>;
}

#[non_exhaustive]
pub struct HostboundViewVariant;

impl VariantSelector for HostboundViewVariant {
    type Output<T: Strategy> = T::HostboundView;
}

#[non_exhaustive]
pub struct GuestboundVariant;

impl VariantSelector for GuestboundVariant {
    type Output<T: Strategy> = T::Guestbound;
}

pub struct GuestboundViewVariant<'a> {
    _ty: PhantomData<&'a ()>,
}

impl<'a> VariantSelector for GuestboundViewVariant<'a> {
    type Output<T: Strategy> = T::GuestboundView<'a>;
}

// Macro
pub mod marshal_struct_internals {
    pub use {
        super::{
            FfiPtr, GuestboundVariant, GuestboundViewVariant, HostContext, HostboundVariant,
            HostboundViewVariant, MarkerVariant, Marshal, Strategy, VariantSelector, ffi_offset,
        },
        anyhow::Result,
    };
}

#[macro_export]
macro_rules! marshal_struct {
    ($(
        $(#[$($item_meta:tt)*])*
        $item_vis:vis struct $item_name:ident {
            $(
                $(#[$($field_meta:tt)*])*
                $field_vis:vis $field_name:ident: $field_ty:ty
            ),+
            $(,)?
        }
    )*) => {$(
        $(#[$($item_meta:tt)*])*
        #[repr(C)]
        $item_vis struct $item_name<
            V: $crate::marshal_struct_internals::VariantSelector = $crate::marshal_struct_internals::MarkerVariant
        > {$(
            $(#[$($field_meta)*])*
            $field_vis $field_name: <V as $crate::marshal_struct_internals::VariantSelector>::Output<
                <$field_ty as $crate::marshal_struct_internals::Marshal>::Strategy
            >,
        )*}

        impl $crate::marshal_struct_internals::Marshal for $item_name {
            type Strategy = Self;
        }

        impl $crate::marshal_struct_internals::Strategy for $item_name {
            type Hostbound<'a> = $item_name<$crate::marshal_struct_internals::HostboundVariant<'a>>;
            type HostboundView = $item_name<$crate::marshal_struct_internals::HostboundViewVariant>;
            type Guestbound = $item_name<$crate::marshal_struct_internals::GuestboundVariant>;
            type GuestboundView<'a> = $item_name<$crate::marshal_struct_internals::GuestboundViewVariant<'a>>;

            fn decode_hostbound(
                cx: &impl $crate::marshal_struct_internals::HostContext,
                ptr: $crate::marshal_struct_internals::FfiPtr<Self::Hostbound<'static>>,
            ) -> $crate::marshal_struct_internals::Result<Self::HostboundView> {
                Ok(Self::HostboundView {$(
                    $field_name: <<$field_ty as $crate::marshal_struct_internals::Marshal>::Strategy>::decode_hostbound(
                        cx,
                        ptr.field($crate::marshal_struct_internals::ffi_offset!(
                            Self::Hostbound<'static>, $field_name,
                        )),
                    )?,
                )*})
            }

            fn encode_guestbound(
                cx: &mut impl $crate::marshal_struct_internals::HostContext,
                out_ptr: $crate::marshal_struct_internals::FfiPtr<Self::Guestbound>,
                value: &Self::GuestboundView<'_>,
            ) -> $crate::marshal_struct_internals::Result<()> {
                $(
                    <<$field_ty as $crate::marshal_struct_internals::Marshal>::Strategy>::encode_guestbound(
                        cx,
                        out_ptr.field($crate::marshal_struct_internals::ffi_offset!(
                            Self::Guestbound, $field_name,
                        )),
                        &value.$field_name,
                    )?;
                )*

                Ok(())
            }
        }
    )*};
}

// === Enum Marshalling === //

// Macro
pub mod marshal_enum_internals {
    pub use {
        super::{FfiPtr, HostContext, Marshal, Strategy},
        anyhow::{Result, bail},
        std::{
            clone::Clone,
            cmp::{Eq, Ord, PartialEq, PartialOrd},
            fmt::Debug,
            hash::Hash,
            marker::Copy,
        },
    };

    pub mod primitives {
        // From: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.intro
        pub use std::primitive::{i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize};
    }
}

#[macro_export]
macro_rules! marshal_enum {
    ($(
        $(#[$($item_meta:tt)*])*
        $item_vis:vis enum $item_name:ident : $repr:ident {
            $(
                $(#[$($field_meta:tt)*])*
                $variant_name:ident $(= $variant_val:expr)?
            ),+
            $(,)?
        }
    )*) => {$(
        #[derive(
            $crate::marshal_enum_internals::Debug,
            $crate::marshal_enum_internals::Copy,
            $crate::marshal_enum_internals::Clone,
            $crate::marshal_enum_internals::Hash,
            $crate::marshal_enum_internals::Eq,
            $crate::marshal_enum_internals::PartialEq,
            $crate::marshal_enum_internals::Ord,
            $crate::marshal_enum_internals::PartialOrd,
        )]
        $(#[$($item_meta:tt)*])*
        #[repr($repr)]
        $item_vis enum $item_name {
            $($variant_name $(= $variant_val)?,)*
        }

        impl $crate::marshal_enum_internals::Marshal for $item_name {
            type Strategy = Self;
        }

        impl $crate::marshal_enum_internals::Strategy for $item_name {
            type Hostbound<'a> = Self;
            type HostboundView = Self;
            type Guestbound = Self;
            type GuestboundView<'a> = Self;

            #[allow(non_upper_case_globals)]
            fn decode_hostbound(
                cx: &impl $crate::marshal_enum_internals::HostContext,
                ptr: $crate::marshal_enum_internals::FfiPtr<Self>,
            ) -> $crate::marshal_enum_internals::Result<Self> {
                $(
                    const $variant_name: $crate::marshal_enum_internals::primitives::$repr
                        = $item_name::$variant_name as $crate::marshal_enum_internals::primitives::$repr;
                )*
                match *ptr.cast::<$crate::marshal_enum_internals::primitives::$repr>().read(cx)? {
                    $($variant_name => Ok(Self::$variant_name),)*
                    _ => $crate::marshal_enum_internals::bail!("unknown enum variant"),
                }
            }

            fn encode_guestbound(
                cx: &mut impl $crate::marshal_enum_internals::HostContext,
                out_ptr: $crate::marshal_enum_internals::FfiPtr<Self>,
                value: &Self,
            ) -> $crate::marshal_enum_internals::Result<()> {
                *out_ptr.cast::<$crate::marshal_enum_internals::primitives::$repr>().write(cx)?
                    = *value as $crate::marshal_enum_internals::primitives::$repr;

                Ok(())
            }
        }
    )*};
}

// === Closure === //

// Host View
pub type HostClosure<I> = HostClosure_<StrategyOf<I>>;

#[derive_where(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct HostClosure_<I: Strategy> {
    _ty: PhantomData<fn(I)>,
    id: u64,
}

impl<I: Strategy> HostClosure_<I> {
    pub fn new(id: u64) -> Self {
        Self {
            _ty: PhantomData,
            id,
        }
    }

    pub fn id(self) -> u64 {
        self.id
    }

    pub fn call(
        self,
        cx: &mut impl HostContext,
        arg: &GuestboundViewOf<'_, I>,
    ) -> anyhow::Result<()> {
        let arg_out = cx.alloc(
            align_of_u32::<GuestboundOf<I>>(),
            size_of_u32::<GuestboundOf<I>>(),
        )?;

        let arg_out = arg_out.cast::<GuestboundOf<I>>();

        <I::Strategy>::encode_guestbound(cx, arg_out, arg)?;

        cx.invoke(self.id, arg_out.addr())?;

        Ok(())
    }
}

// Guest Closure

cfgenius::cond! {
    if macro(is_wasm) {
        thread_local! {
            #[expect(clippy::type_complexity)]
            static CLOSURES: std::cell::RefCell<thunderdome::Arena<std::rc::Rc<dyn Fn(*mut ())>>> =
                const { std::cell::RefCell::new(thunderdome::Arena::new()) };
        }
    }
}

pub type OwnedGuestClosure<I> = OwnedGuestClosure_<StrategyOf<I>>;
pub type GuestClosure<I> = GuestClosure_<StrategyOf<I>>;

#[derive_where(Debug)]
#[repr(transparent)]
pub struct OwnedGuestClosure_<I: Strategy> {
    raw: GuestClosure_<I>,
}

impl<I: Strategy> OwnedGuestClosure_<I> {
    #[must_use]
    pub fn new(f: impl 'static + Fn(GuestboundOf<I>)) -> Self {
        Self::wrap(GuestClosure_::new_unmanaged(f))
    }

    #[must_use]
    pub fn wrap(raw: GuestClosure_<I>) -> Self {
        Self { raw }
    }

    pub fn handle(&self) -> GuestClosure_<I> {
        self.raw
    }

    pub fn defuse(self) -> GuestClosure_<I> {
        let raw = self.raw;
        mem::forget(self);
        raw
    }
}

impl<I: Strategy> Deref for OwnedGuestClosure_<I> {
    type Target = GuestClosure_<I>;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl<I: Strategy> Drop for OwnedGuestClosure_<I> {
    fn drop(&mut self) {
        self.raw.unmanaged_destroy();
    }
}

#[derive_where(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct GuestClosure_<I: Strategy> {
    _no_send_sync: PhantomData<*const ()>,
    _ty: PhantomData<fn(I)>,
    raw_handle: u64,
}

impl<I: Strategy> GuestClosure_<I> {
    #[must_use]
    pub fn new_unmanaged(f: impl 'static + Fn(GuestboundOf<I>)) -> Self {
        cfgenius::cond! {
            if macro(is_wasm) {
                Self {
                    _no_send_sync: PhantomData,
                    _ty: PhantomData,
                    raw_handle: CLOSURES.with_borrow_mut(|v| {
                        v.insert(std::rc::Rc::new(move |ptr| {
                            f(*unsafe { Box::from_raw(ptr.cast::<GuestboundOf<I>>()) })
                        }))
                        .to_bits()
                    }),
                }
            } else {
                _ = f;

                unimplemented!()
            }
        }
    }

    #[must_use]
    pub fn is_alive(self) -> bool {
        cfgenius::cond! {
            if macro(is_wasm) {
                CLOSURES.with_borrow(|v| v.contains(self.handle()))
            } else {
                unreachable!()
            }
        }
    }

    pub fn unmanaged_destroy(self) -> bool {
        cfgenius::cond! {
            if macro(is_wasm) {
                CLOSURES
                    .with_borrow_mut(|v| v.remove(self.handle()))
                    .is_some()
            } else {
                unreachable!()
            }
        }
    }

    #[allow(clippy::boxed_local)] // (for non-wasm variant)
    pub fn call(self, arg: Box<GuestboundOf<I>>) {
        cfgenius::cond! {
            if macro(is_wasm) {
                unsafe { call_raw_closure(self.raw_handle, Box::into_raw(arg).cast::<()>()) }
            } else {
                _ = arg;

                unreachable!()
            }
        }
    }
}

cfgenius::cond! {
    if macro(is_wasm) {
        impl<I: Strategy> GuestClosure_<I> {
            fn handle(self) -> thunderdome::Index {
                thunderdome::Index::from_bits(self.raw_handle).unwrap()
            }
        }

        unsafe fn call_raw_closure(handle: u64, boxed_arg: *mut ()) {
            cfgenius::cond! {
                if macro(is_wasm) {
                    let handle = thunderdome::Index::from_bits(handle).unwrap();

                    let slot = CLOSURES
                        .with_borrow(|v| v.get(handle).cloned())
                        .unwrap_or_else(|| panic!("attempted to call dead closure {handle:?}"));

                    slot(boxed_arg);
                } else {
                    _ = boxed_arg;

                    unreachable!();
                }
            }
        }
    }
}

// Marshal
impl<I: Marshal> Marshal for fn(I) {
    type Strategy = fn(I::Strategy);
}

impl<I: Strategy> Strategy for fn(I) {
    type Hostbound<'a> = GuestClosure_<I>;
    type HostboundView = HostClosure_<I>;
    type Guestbound = GuestClosure_<I>;
    type GuestboundView<'a> = HostClosure_<I>;

    fn decode_hostbound(
        cx: &impl HostContext,
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        Ok(HostClosure_::new(*ptr.cast::<u64>().read(cx)?))
    }

    fn encode_guestbound(
        cx: &mut impl HostContext,
        out_ptr: FfiPtr<Self::Guestbound>,
        value: &Self::GuestboundView<'_>,
    ) -> anyhow::Result<()> {
        *out_ptr.cast::<u64>().write(cx)? = value.id();

        Ok(())
    }
}

// === Ports === //

#[derive_where(Debug, Copy, Clone)]
#[expect(clippy::type_complexity)]
pub struct Port<I, O = ()>
where
    I: Marshal,
    O: Marshal,
{
    _ty: PhantomData<fn(I, O) -> (I, O)>,
    module: &'static str,
    func_name: &'static str,
}

impl<I, O> Port<I, O>
where
    I: Marshal,
    O: Marshal,
{
    pub const fn new(module: &'static str, func_name: &'static str) -> Self {
        Self {
            _ty: PhantomData,
            module,
            func_name,
        }
    }

    pub const fn module(self) -> &'static str {
        self.module
    }

    pub const fn func_name(self) -> &'static str {
        self.func_name
    }

    pub const fn is_compatible(self, other: Port<I, O>) -> bool {
        const fn str_eq(a: &str, b: &str) -> bool {
            if a.len() != b.len() {
                return false;
            }

            let mut i = 0;

            while i < a.len() {
                if a.as_bytes()[i] != b.as_bytes()[i] {
                    return false;
                }

                i += 1;
            }

            true
        }

        str_eq(self.module, other.module) && str_eq(self.func_name, other.func_name)
    }

    pub const fn assert_compatible(self, other: Port<I, O>) {
        if !self.is_compatible(other) {
            panic!("incompatible ports");
        }
    }
}

#[doc(hidden)]
pub mod bind_port_internals {
    pub use {
        crate::{GuestboundOf, HostboundOf, Port},
        std::{mem::MaybeUninit, stringify},
    };
}

#[macro_export]
macro_rules! bind_port {
    ($(
        $(#[$($meta:tt)*])*
        $vis:vis fn[$matches_port:expr] $module:literal.$name:ident($input:ty) $(-> $out:ty)?;
    )*) => {$(
        $(#[$meta])*
        $vis fn $name(input: &$crate::bind_port_internals::HostboundOf<$input>) -> ($($crate::bind_port_internals::GuestboundOf<$out>)?) {
            const _: () = {
                $crate::bind_port_internals::Port::<$input, ($($out)?)>::new(
                    $module,
                    $crate::bind_port_internals::stringify!($name),
                )
                .assert_compatible($matches_port)
            };

            #[link(wasm_import_module = $module)]
            unsafe extern "C" {
                fn $name(
                    input: &$crate::bind_port_internals::HostboundOf<$input>,
                    output: *mut $crate::bind_port_internals::GuestboundOf<($($out)?)>,
                );
            }

            unsafe {
                let mut output = $crate::bind_port_internals::MaybeUninit::uninit();
                $name(input, output.as_mut_ptr());
                output.assume_init()
            }
        }
    )*};
}

// === Guest Exports === //

pub const BUILTIN_MEM_ALLOC: &str = "rust_wasmlink_mem_alloc";
pub const BUILTIN_CLOSURE_INVOKE: &str = "rust_wasmlink_closure_invoke";

cfgenius::cond! {
    if macro(is_wasm) {
        #[unsafe(no_mangle)]
        unsafe extern "C" fn rust_wasmlink_mem_alloc(align: usize, size: usize) -> *mut u8 {
            let layout = std::alloc::Layout::from_size_align(size, align).unwrap();

            if layout.size() == 0 {
                align as *mut u8
            } else {
                unsafe { std::alloc::alloc(layout) }
            }
        }

        #[unsafe(no_mangle)]
        unsafe extern "C" fn rust_wasmlink_closure_invoke(handle: u64, boxed_arg: *mut ()) {
            unsafe { call_raw_closure(handle, boxed_arg) };
        }
    }
}
