#![allow(clippy::missing_safety_doc)]

use std::{
    error::Error,
    fmt,
    marker::PhantomData,
    mem::{self, MaybeUninit},
    ops::Range,
};

use bytemuck::{Pod, TransparentWrapper, Zeroable};
use derive_where::derive_where;

// === Helpers === //

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
        let align = mem::align_of::<T>() as u64;

        if align > u32::MAX as u64 {
            panic!("size is too large for guest")
        }

        align as u32
    }
}

pub const fn guest_usize_to_u32(val: usize) -> u32 {
    #[cfg(target_arch = "wasm32")]
    {
        val as u32
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        _ = val;

        unimplemented!();
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
        #[cfg(target_arch = "wasm32")]
        {
            self.addr as usize as *mut T
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            unimplemented!()
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
    pub fn read(self, cx: &impl HostMemory) -> Result<&T, HostMarshalError> {
        Ok(&cx.read(self.addr(), 1)?[0])
    }

    pub fn write(self, cx: &mut impl HostMemoryMut) -> Result<&mut T, HostMarshalError> {
        Ok(&mut cx.write(self.addr(), 1)?[0])
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
        #[cfg(target_arch = "wasm32")]
        {
            std::ptr::slice_from_raw_parts(self.base as usize as *mut T, self.len as usize)
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            unimplemented!()
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
    pub fn read(self, cx: &impl HostMemory) -> Result<&[T], HostMarshalError> {
        cx.read(self.base().addr(), self.len())
    }

    pub fn write(self, cx: &mut impl HostMemoryMut) -> Result<&mut [T], HostMarshalError> {
        cx.write(self.base().addr(), self.len())
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
            $crate::ffi_offset_internals::make_ffi_offset(
                $crate::ffi_offset_internals::offset_of!($ty, $($path)*),
                |me| &me.$($path)*
            )
        }
    };
}

// === Marshal Trait === //

pub type FfiOf<M> = <<M as Marshal>::Strategy as Strategy>::Ffi;
pub type GuestDestOf<M> = <<M as Marshal>::Strategy as Strategy>::GuestDest;
pub type GuestSrcOf<'a, M> = <<M as Marshal>::Strategy as Strategy>::GuestSrc<'a>;
pub type HostDestOf<M> = <<M as Marshal>::Strategy as Strategy>::HostDest;
pub type HostSrcOf<'a, M> = <<M as Marshal>::Strategy as Strategy>::HostSrc<'a>;

#[derive(Debug, Clone)]
pub struct HostMarshalError;

impl fmt::Display for HostMarshalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("failed to marshal value on host")
    }
}

impl Error for HostMarshalError {}

pub trait HostMemory: Sized {
    fn read<T: Pod>(&self, base: u32, len: u32) -> Result<&[T], HostMarshalError>;
}

pub trait HostMemoryMut: HostMemory {
    fn write<T: Pod>(&mut self, base: u32, len: u32) -> Result<&mut [T], HostMarshalError>;
}

pub trait HostAlloc: HostMemoryMut {
    fn alloc(&mut self, align: u32, size: u32) -> Result<FfiPtr<()>, HostMarshalError>;
}

pub trait Marshal: Sized + 'static {
    type Strategy: Strategy;
}

pub unsafe trait Strategy: Sized + 'static + Marshal<Strategy = Self> {
    /// An FFI-safe representation of the value. This type should have the same layout between guest
    /// and host. This isn't exactly a [`Pod`] since it may have padding.
    type Ffi;

    /// The type the value should be decoded as on the guest.
    ///
    /// ## Safety
    ///
    /// Must be transmutable from [`Repr`](Marshal::Repr) when the target is a guest target.
    type GuestDest;

    /// The type the value should be encoded from on the guest.
    ///
    /// ## Safety
    ///
    /// Must be transmutable to [`Repr`](Marshal::Repr) when the target is a guest target.
    type GuestSrc<'a>;

    /// The type the value should be decoded as on the host.
    type HostDest;

    /// The type the value should be encoded from on the host.
    type HostSrc<'a>;

    fn decode_host(
        cx: &impl HostMemory,
        ptr: FfiPtr<Self::Ffi>,
    ) -> Result<Self::HostDest, HostMarshalError>;

    fn encode_host(
        cx: &mut impl HostAlloc,
        out_ptr: FfiPtr<Self::Ffi>,
        value: &Self::HostSrc<'_>,
    ) -> Result<(), HostMarshalError>;
}

// === PodMarshalStrategy === //

pub struct PodMarshalStrategy<T: Pod> {
    _ty: PhantomData<fn(T) -> T>,
}

impl<T: Pod> Marshal for PodMarshalStrategy<T> {
    type Strategy = Self;
}

unsafe impl<T: Pod> Strategy for PodMarshalStrategy<T> {
    type Ffi = T;
    type GuestDest = T;
    type GuestSrc<'a> = T;
    type HostDest = T;
    type HostSrc<'a> = T;

    fn decode_host(
        cx: &impl HostMemory,
        ptr: FfiPtr<Self::Ffi>,
    ) -> Result<Self::HostDest, HostMarshalError> {
        ptr.read(cx).copied()
    }

    fn encode_host(
        cx: &mut impl HostAlloc,
        out_ptr: FfiPtr<Self::Ffi>,
        value: &Self::HostSrc<'_>,
    ) -> Result<(), HostMarshalError> {
        *out_ptr.write(cx)? = *value;

        Ok(())
    }
}

macro_rules! alias_pod_marshal {
    ( $($ty:ty),*$(,)? ) => {$(
        impl Marshal for $ty {
            type Strategy = PodMarshalStrategy<$ty>;
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

// === BoolMarshalStrategy === //

#[non_exhaustive]
pub struct BoolMarshalStrategy;

impl Marshal for BoolMarshalStrategy {
    type Strategy = Self;
}

unsafe impl Strategy for BoolMarshalStrategy {
    type Ffi = bool;
    type GuestDest = bool;
    type GuestSrc<'a> = bool;
    type HostDest = bool;
    type HostSrc<'a> = bool;

    fn decode_host(
        cx: &impl HostMemory,
        ptr: FfiPtr<Self::Ffi>,
    ) -> Result<Self::HostDest, HostMarshalError> {
        match *ptr.cast::<u8>().read(cx)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(HostMarshalError),
        }
    }

    fn encode_host(
        cx: &mut impl HostAlloc,
        out_ptr: FfiPtr<Self::Ffi>,
        value: &Self::HostSrc<'_>,
    ) -> Result<(), HostMarshalError> {
        *out_ptr.cast::<u8>().write(cx)? = *value as u8;

        Ok(())
    }
}

impl Marshal for bool {
    type Strategy = BoolMarshalStrategy;
}

// === CharMarshalStrategy === //

#[non_exhaustive]
pub struct CharMarshalStrategy;

impl Marshal for CharMarshalStrategy {
    type Strategy = Self;
}

unsafe impl Strategy for CharMarshalStrategy {
    type Ffi = char;
    type GuestDest = char;
    type GuestSrc<'a> = char;
    type HostDest = char;
    type HostSrc<'a> = char;

    fn decode_host(
        cx: &impl HostMemory,
        ptr: FfiPtr<Self::Ffi>,
    ) -> Result<Self::HostDest, HostMarshalError> {
        char::try_from(*ptr.cast::<u32>().read(cx)?).map_err(|_| HostMarshalError)
    }

    fn encode_host(
        cx: &mut impl HostAlloc,
        out_ptr: FfiPtr<Self::Ffi>,
        value: &Self::HostSrc<'_>,
    ) -> Result<(), HostMarshalError> {
        *out_ptr.cast::<u32>().write(cx)? = *value as u32;

        Ok(())
    }
}

impl Marshal for char {
    type Strategy = CharMarshalStrategy;
}

// === OptionMarshalStrategy === //

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

// Strategy
pub struct OptionMarshalStrategy<T: Strategy> {
    _ty: PhantomData<fn(T) -> T>,
}

impl<T: Strategy> Marshal for OptionMarshalStrategy<T> {
    type Strategy = Self;
}

unsafe impl<T: Strategy> Strategy for OptionMarshalStrategy<T> {
    type Ffi = FfiOption<T::Ffi>;
    type GuestDest = FfiOption<T::GuestDest>;
    type GuestSrc<'a> = FfiOption<T::GuestSrc<'a>>;
    type HostDest = Option<T::HostDest>;
    type HostSrc<'a> = Option<T::HostSrc<'a>>;

    fn decode_host(
        cx: &impl HostMemory,
        ptr: FfiPtr<Self::Ffi>,
    ) -> Result<Self::HostDest, HostMarshalError> {
        let is_present = *ptr
            .field(ffi_offset!(FfiOption<T>, present))
            .cast::<u8>()
            .read(cx)?;

        if is_present == 0 {
            return Ok(None);
        }

        let value = T::decode_host(cx, ptr.field(ffi_offset!(FfiOption<T>, value)).cast())?;

        Ok(Some(value))
    }

    fn encode_host(
        cx: &mut impl HostAlloc,
        out_ptr: FfiPtr<Self::Ffi>,
        value: &Self::HostSrc<'_>,
    ) -> Result<(), HostMarshalError> {
        *out_ptr
            .field(ffi_offset!(FfiOption<T>, present))
            .cast::<u8>()
            .write(cx)? = value.is_some() as u8;

        if let Some(value) = value {
            T::encode_host(
                cx,
                out_ptr.field(ffi_offset!(FfiOption<T>, value)).cast(),
                value,
            )?;
        }

        Ok(())
    }
}

impl<T: Marshal> Marshal for Option<T> {
    type Strategy = OptionMarshalStrategy<T::Strategy>;
}

// === BoxMarshalStrategy === //

// Views
#[derive_where(Copy, Clone, Hash, Eq, PartialEq)]
#[repr(transparent)]
pub struct HostPtr<T: Strategy> {
    ptr: FfiPtr<T::Ffi>,
}

impl<T: Strategy> HostPtr<T> {
    pub const fn new(ptr: FfiPtr<T::Ffi>) -> Self {
        Self { ptr }
    }

    pub const fn ptr(self) -> FfiPtr<T::Ffi> {
        self.ptr
    }

    pub fn decode(self, cx: &impl HostMemory) -> Result<T::HostDest, HostMarshalError> {
        T::decode_host(cx, self.ptr)
    }
}

// Strategy
pub struct BoxMarshalStrategy<T: Strategy> {
    _ty: PhantomData<fn(T) -> T>,
}

impl<T: Strategy> Marshal for BoxMarshalStrategy<T> {
    type Strategy = Self;
}

unsafe impl<T: Strategy> Strategy for BoxMarshalStrategy<T> {
    type Ffi = FfiPtr<T::Ffi>;
    type GuestDest = Box<T::GuestDest>;
    type GuestSrc<'a> = &'a T::GuestSrc<'a>;
    type HostDest = HostPtr<T>;
    type HostSrc<'a> = &'a T::HostSrc<'a>;

    fn decode_host(
        cx: &impl HostMemory,
        ptr: FfiPtr<Self::Ffi>,
    ) -> Result<Self::HostDest, HostMarshalError> {
        Ok(HostPtr {
            ptr: *ptr.read(cx)?,
        })
    }

    fn encode_host(
        cx: &mut impl HostAlloc,
        out_ptr: FfiPtr<Self::Ffi>,
        value: &Self::HostSrc<'_>,
    ) -> Result<(), HostMarshalError> {
        let allocated = cx
            .alloc(align_of_u32::<T::Ffi>(), size_of_u32::<T::Ffi>())?
            .cast::<T::Ffi>();

        T::encode_host(cx, allocated, value)?;

        *out_ptr.write(cx)? = allocated;

        Ok(())
    }
}

impl<T: Marshal> Marshal for Box<T> {
    type Strategy = BoxMarshalStrategy<T::Strategy>;
}

// === VecMarshalStrategy === //

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

#[derive_where(Copy, Clone, Hash, Eq, PartialEq)]
#[repr(transparent)]
pub struct HostSlice<T: Strategy> {
    slice: FfiSlice<T::Ffi>,
}

impl<T: Strategy> HostSlice<T> {
    pub const fn new(slice: FfiSlice<T::Ffi>) -> Self {
        Self { slice }
    }

    pub const fn slice(self) -> FfiSlice<T::Ffi> {
        self.slice
    }

    pub const fn is_empty(self) -> bool {
        self.slice.is_empty()
    }

    pub const fn len(self) -> u32 {
        self.slice.len()
    }
}

impl<T: Strategy> FfiSliceIndexable for HostSlice<T> {
    type RawElem = T::Ffi;
    type RichPtr = HostPtr<T>;

    fn unwrap_slice(me: Self) -> FfiSlice<Self::RawElem> {
        me.slice()
    }

    fn wrap_slice(slice: FfiSlice<Self::RawElem>) -> Self {
        Self::new(slice)
    }

    fn wrap_ptr(ptr: FfiPtr<Self::RawElem>) -> Self::RichPtr {
        HostPtr::new(ptr)
    }
}

// Strategy
pub struct VecMarshalStrategy<T: Strategy> {
    _ty: PhantomData<fn(T) -> T>,
}

impl<T: Strategy> Marshal for VecMarshalStrategy<T> {
    type Strategy = Self;
}

unsafe impl<T: Strategy> Strategy for VecMarshalStrategy<T> {
    type Ffi = FfiSlice<T::Ffi>;
    type GuestDest = GuestSlice<T::GuestDest>;
    type GuestSrc<'a> = GuestSliceRef<'a, T::GuestSrc<'a>>;
    type HostDest = HostSlice<T>;
    type HostSrc<'a> = &'a [T::HostSrc<'a>];

    fn decode_host(
        cx: &impl HostMemory,
        ptr: FfiPtr<Self::Ffi>,
    ) -> Result<Self::HostDest, HostMarshalError> {
        Ok(HostSlice::new(*ptr.read(cx)?))
    }

    fn encode_host(
        cx: &mut impl HostAlloc,
        out_ptr: FfiPtr<Self::Ffi>,
        value: &Self::HostSrc<'_>,
    ) -> Result<(), HostMarshalError> {
        let allocated = cx.alloc(
            align_of_u32::<T::Ffi>(),
            size_of_u32::<T::Ffi>()
                .checked_mul(u32::try_from(value.len()).expect("too many elements"))
                .expect("slice too big for guest"),
        )?;

        let allocated = FfiSlice::<T::Ffi>::new(allocated.cast(), value.len() as u32);

        for (value, out_ptr) in value.iter().zip(allocated) {
            T::encode_host(cx, out_ptr, value)?;
        }

        *out_ptr.write(cx)? = allocated;

        Ok(())
    }
}

impl<T: Marshal> Marshal for Vec<T> {
    type Strategy = VecMarshalStrategy<T::Strategy>;
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

#[non_exhaustive]
pub struct FfiVariant;

impl VariantSelector for FfiVariant {
    type Output<T: Strategy> = T::Ffi;
}

#[non_exhaustive]
pub struct GuestDestVariant;

impl VariantSelector for GuestDestVariant {
    type Output<T: Strategy> = T::GuestDest;
}

pub struct GuestSrcVariant<'a> {
    _ty: PhantomData<&'a ()>,
}

impl<'a> VariantSelector for GuestSrcVariant<'a> {
    type Output<T: Strategy> = T::GuestSrc<'a>;
}

#[non_exhaustive]
pub struct HostDestVariant;

impl VariantSelector for HostDestVariant {
    type Output<T: Strategy> = T::HostDest;
}

pub struct HostSrcVariant<'a> {
    _ty: PhantomData<&'a ()>,
}

impl<'a> VariantSelector for HostSrcVariant<'a> {
    type Output<T: Strategy> = T::HostSrc<'a>;
}

// Macro
pub mod marshal_struct_internals {
    pub use {
        super::{
            FfiPtr, FfiVariant, GuestDestVariant, GuestSrcVariant, HostAlloc, HostDestVariant,
            HostMarshalError, HostMemory, HostSrcVariant, MarkerVariant, Marshal, Strategy,
            VariantSelector, ffi_offset,
        },
        std::result::Result,
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

        unsafe impl $crate::marshal_struct_internals::Strategy for $item_name {
            type Ffi = $item_name<$crate::marshal_struct_internals::FfiVariant>;
            type GuestDest = $item_name<$crate::marshal_struct_internals::GuestDestVariant>;
            type GuestSrc<'a> = $item_name<$crate::marshal_struct_internals::GuestSrcVariant<'a>>;
            type HostDest = $item_name<$crate::marshal_struct_internals::HostDestVariant>;
            type HostSrc<'a> = $item_name<$crate::marshal_struct_internals::HostSrcVariant<'a>>;

            fn decode_host(
                cx: &impl $crate::marshal_struct_internals::HostMemory,
                ptr: $crate::marshal_struct_internals::FfiPtr<Self::Ffi>,
            ) -> $crate::marshal_struct_internals::Result<
                Self::HostDest,
                $crate::marshal_struct_internals::HostMarshalError,
            > {
                Ok($item_name::<$crate::marshal_struct_internals::HostDestVariant> {$(
                    $field_name: <<$field_ty as $crate::marshal_struct_internals::Marshal>::Strategy>::decode_host(
                        cx,
                        ptr.field($crate::marshal_struct_internals::ffi_offset!(
                            $item_name<$crate::marshal_struct_internals::FfiVariant>,
                            $field_name
                        )),
                    )?,
                )*})
            }

            fn encode_host(
                cx: &mut impl $crate::marshal_struct_internals::HostAlloc,
                out_ptr: $crate::marshal_struct_internals::FfiPtr<Self::Ffi>,
                value: &Self::HostSrc<'_>,
            ) -> $crate::marshal_struct_internals::Result<
                (),
                $crate::marshal_struct_internals::HostMarshalError,
            > {
                $(
                    <<$field_ty as $crate::marshal_struct_internals::Marshal>::Strategy>::encode_host(
                        cx,
                        out_ptr.field($crate::marshal_struct_internals::ffi_offset!(
                            $item_name<$crate::marshal_struct_internals::FfiVariant>,
                            $field_name
                        )),
                        &value.$field_name,
                    )?;
                )*

                Ok(())
            }
        }
    )*};
}
