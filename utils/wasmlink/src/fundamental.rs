use std::{
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
};

use anyhow::Context as _;
use bytemuck::Pod;
use derive_where::derive_where;

use crate::{
    FfiPtr, FfiSlice, FfiSliceIndexable, GuestInvokeContext, GuestMemoryContext, Marshal, Strategy,
    StrategyOf, UnifiedStrategy, ffi_offset,
    utils::{align_of_u32, size_of_u32},
};

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
        cx: &(impl ?Sized + GuestMemoryContext),
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        ptr.read(cx).copied()
    }

    fn encode_guestbound(
        cx: &mut impl GuestInvokeContext,
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
        cx: &(impl ?Sized + GuestMemoryContext),
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        T::try_from_raw(T::Raw::decode_hostbound(
            cx,
            ptr.cast::<<T::Raw as UnifiedStrategy>::Unified>(),
        )?)
    }

    fn encode_guestbound(
        cx: &mut impl GuestInvokeContext,
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
        cx: &(impl ?Sized + GuestMemoryContext),
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
        cx: &mut impl GuestInvokeContext,
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

// === Result === //

// Views
#[repr(C)]
pub struct FfiResult<T, E> {
    is_ok: bool,
    value: FfiResultInner<T, E>,
}

#[repr(C)]
union FfiResultInner<T, E> {
    ok: ManuallyDrop<T>,
    err: ManuallyDrop<E>,
}

impl<T, E> FfiResult<T, E> {
    pub fn new(value: Result<T, E>) -> Self {
        match value {
            Ok(value) => Self::ok(value),
            Err(value) => Self::err(value),
        }
    }

    pub const fn ok(value: T) -> Self {
        Self {
            is_ok: true,
            value: FfiResultInner {
                ok: ManuallyDrop::new(value),
            },
        }
    }

    pub const fn err(value: E) -> Self {
        Self {
            is_ok: false,
            value: FfiResultInner {
                err: ManuallyDrop::new(value),
            },
        }
    }

    pub const fn decode(self) -> Result<T, E> {
        match self.is_ok {
            true => Ok(ManuallyDrop::into_inner(unsafe { self.value.ok })),
            false => Err(ManuallyDrop::into_inner(unsafe { self.value.err })),
        }
    }

    pub const fn decode_ref(&self) -> Result<&T, &E> {
        match self.is_ok {
            true => Ok(unsafe { &*(&self.value.ok as *const ManuallyDrop<T> as *const T) }),
            false => Err(unsafe { &*(&self.value.err as *const ManuallyDrop<E> as *const E) }),
        }
    }

    pub const fn decode_mut(&mut self) -> Result<&mut T, &mut E> {
        match self.is_ok {
            true => Ok(unsafe { &mut *(&mut self.value.ok as *mut ManuallyDrop<T> as *mut T) }),
            false => Err(unsafe { &mut *(&mut self.value.err as *mut ManuallyDrop<E> as *mut E) }),
        }
    }
}

impl<T, E> From<Result<T, E>> for FfiResult<T, E> {
    fn from(value: Result<T, E>) -> Self {
        Self::new(value)
    }
}

// Strategy
impl<T: Marshal, E: Marshal> Marshal for Result<T, E> {
    type Strategy = Result<T::Strategy, E::Strategy>;
}

impl<T: Strategy, E: Strategy> Strategy for Result<T, E> {
    type Hostbound<'a> = FfiResult<T::Hostbound<'a>, E::Hostbound<'a>>;
    type HostboundView = Result<T::HostboundView, E::HostboundView>;
    type Guestbound = FfiResult<T::Guestbound, E::Guestbound>;
    type GuestboundView<'a> = Result<T::GuestboundView<'a>, E::GuestboundView<'a>>;

    fn decode_hostbound(
        cx: &(impl ?Sized + GuestMemoryContext),
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        let is_ok = *ptr
            .field(ffi_offset!(
                FfiResult<T::Hostbound<'static>, E::Hostbound<'static>>,
                is_ok
            ))
            .cast::<u8>()
            .read(cx)?;

        match is_ok {
            0 => {
                let value = E::decode_hostbound(
                    cx,
                    ptr.field(ffi_offset!(
                        FfiResult<T::Hostbound<'static>, E::Hostbound<'static>>,
                        value
                    ))
                    .cast(),
                )?;

                Ok(Err(value))
            }
            1 => {
                let value = T::decode_hostbound(
                    cx,
                    ptr.field(ffi_offset!(
                        FfiResult<T::Hostbound<'static>, E::Hostbound<'static>>,
                        value
                    ))
                    .cast(),
                )?;

                Ok(Ok(value))
            }
            v => anyhow::bail!("unknown result variant {v}"),
        }
    }

    fn encode_guestbound(
        cx: &mut impl GuestInvokeContext,
        out_ptr: FfiPtr<Self::Guestbound>,
        value: &Self::GuestboundView<'_>,
    ) -> anyhow::Result<()> {
        *out_ptr
            .field(ffi_offset!(FfiResult<T::Guestbound, E::Guestbound>, is_ok))
            .cast::<u8>()
            .write(cx)? = value.is_ok() as u8;

        match value {
            Ok(value) => {
                T::encode_guestbound(
                    cx,
                    out_ptr
                        .field(ffi_offset!(FfiResult<T::Guestbound, E::Guestbound>, value))
                        .cast(),
                    value,
                )?;
            }
            Err(value) => {
                E::encode_guestbound(
                    cx,
                    out_ptr
                        .field(ffi_offset!(FfiResult<T::Guestbound, E::Guestbound>, value))
                        .cast(),
                    value,
                )?;
            }
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

    pub fn decode(
        self,
        cx: &(impl ?Sized + GuestMemoryContext),
    ) -> anyhow::Result<T::HostboundView> {
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
        cx: &(impl ?Sized + GuestMemoryContext),
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        Ok(HostPtr_ {
            ptr: *ptr.cast::<FfiPtr<T::Hostbound<'static>>>().read(cx)?,
        })
    }

    fn encode_guestbound(
        cx: &mut impl GuestInvokeContext,
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
        cx: &(impl ?Sized + GuestMemoryContext),
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        Ok(HostSlice_::new(
            *ptr.cast::<FfiSlice<T::Hostbound<'static>>>().read(cx)?,
        ))
    }

    fn encode_guestbound(
        cx: &mut impl GuestInvokeContext,
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

    pub fn read(self, cx: &(impl ?Sized + GuestMemoryContext)) -> anyhow::Result<&str> {
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
        cx: &(impl ?Sized + GuestMemoryContext),
        ptr: FfiPtr<Self::Hostbound<'static>>,
    ) -> anyhow::Result<Self::HostboundView> {
        Ok(HostStr::new(*ptr.cast::<FfiSlice<u8>>().read(cx)?))
    }

    fn encode_guestbound(
        cx: &mut impl GuestInvokeContext,
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
