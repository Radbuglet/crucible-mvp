#![no_std]
#![allow(clippy::missing_safety_doc)]

#[cfg(feature = "alloc")]
extern crate alloc;

use core::{
    fmt,
    marker::PhantomData,
    ptr::{self, NonNull},
};

use bytemuck::{Pod, Zeroable};

// === Helpers === //

#[macro_export]
#[doc(hidden)]
macro_rules! impl_variadic {
    ($target:path) => {
        impl_variadic!($target; V1 V2 V3 V4 V5 V6 V7 V8 V9 V10 V11 V12);
    };
    ($target:path; $($first:ident $($remaining:ident)*)?) => {
        $target!($($first $($remaining)*)?);
        $(impl_variadic!($target; $($remaining)*);)?
    };
}

// === ZstFn === //

pub trait ZstFn<A>: Sized {
    type Output;

    unsafe fn call_static(args: A) -> Self::Output;
}

impl<A, R, F> ZstFn<A> for F
where
    F: 'static + Sized + Send + Sync + Fn(A) -> R,
{
    type Output = R;

    unsafe fn call_static(args: A) -> Self::Output {
        struct StaticValidation<F>(F);

        impl<F> StaticValidation<F> {
            const IS_VALID: bool = {
                if core::mem::size_of::<F>() != 0 {
                    panic!("must be ZST");
                }
                true
            };
        }
        assert!(StaticValidation::<F>::IS_VALID);

        NonNull::<F>::dangling().as_ref()(args)
    }
}

// === GuestMarshaledTy === //

pub trait GuestMarshaledTy: Sized {
    type GuestPrim;

    fn into_guest_prim(me: Self) -> Self::GuestPrim;

    fn from_guest_prim(me: Self::GuestPrim) -> Option<Self>;
}

macro_rules! impl_func_ty {
    ($($ty:ty => $prim:ty),*$(,)?) => {$(
        impl GuestMarshaledTy for $ty {
            type GuestPrim = $prim;

            fn into_guest_prim(me: Self) -> Self::GuestPrim {
                me.into()
            }

            fn from_guest_prim(me: Self::GuestPrim) -> Option<Self> {
                Self::try_from(me).ok()
            }
        }
    )*};
}

impl_func_ty!(
    u8 => u32,
    u16 => u32,
    u32 => u32,
    i8 => i32,
    i16 => i32,
    i32 => i32,
    u64 => u64,
    i64 => i64,
    char => u32,
);

impl GuestMarshaledTy for bool {
    type GuestPrim = u32;

    fn into_guest_prim(me: Self) -> Self::GuestPrim {
        me as u32
    }

    fn from_guest_prim(me: Self::GuestPrim) -> Option<Self> {
        match me {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
}

// === GuestMarshaledTyList === //

pub trait GuestMarshaledTyList: Sized {
    type PrimFunc<R>: Copy;
    type GuestPrims;

    fn wrap_prim_func<F, R>(f: F) -> Self::PrimFunc<R::GuestPrims>
    where
        F: ZstFn<Self, Output = R>,
        R: GuestMarshaledTyList;

    fn into_guest_prims(me: Self) -> Self::GuestPrims;

    fn from_guest_prims(me: Self::GuestPrims) -> Option<Self>;
}

impl<T: GuestMarshaledTy> GuestMarshaledTyList for T {
    type PrimFunc<R> = fn(T::GuestPrim) -> R;
    type GuestPrims = T::GuestPrim;

    fn wrap_prim_func<F, R>(f: F) -> Self::PrimFunc<R::GuestPrims>
    where
        F: ZstFn<Self, Output = R>,
        R: GuestMarshaledTyList,
    {
        let _ = f;

        |arg| {
            let arg = T::from_guest_prim(arg).unwrap();
            let res = unsafe { F::call_static(arg) };
            R::into_guest_prims(res)
        }
    }

    fn into_guest_prims(me: Self) -> Self::GuestPrims {
        T::into_guest_prim(me)
    }

    fn from_guest_prims(me: Self::GuestPrims) -> Option<Self> {
        T::from_guest_prim(me)
    }
}

macro_rules! impl_marshaled_res_ty {
    ($($para:ident)*) => {
        impl<$($para: GuestMarshaledTy,)*> GuestMarshaledTyList for ($($para,)*) {
            type PrimFunc<R> = fn($(<$para as GuestMarshaledTy>::GuestPrim,)*) -> R;
            type GuestPrims = ($(<$para as GuestMarshaledTy>::GuestPrim,)*);

            #[allow(non_snake_case)]
            fn wrap_prim_func<F, R>(f: F) -> Self::PrimFunc<R::GuestPrims>
            where
                F: ZstFn<Self, Output = R>,
                R: GuestMarshaledTyList,
            {
                let _ = f;

                |$($para,)*| {
                    let arg = Self::from_guest_prims(($($para,)*)).unwrap();
                    let res = unsafe { F::call_static(arg) };
                    R::into_guest_prims(res)
                }
            }

            #[allow(clippy::unused_unit, non_snake_case)]
            fn into_guest_prims(($($para,)*): Self) -> Self::GuestPrims {
                ( $(GuestMarshaledTy::into_guest_prim($para),)* )
            }

            #[allow(non_snake_case)]
            fn from_guest_prims(($($para,)*): Self::GuestPrims) -> Option<Self> {
                Some(( $(GuestMarshaledTy::from_guest_prim($para)?,)* ))
            }
        }
    };
}

impl_variadic!(impl_marshaled_res_ty);

// === Little Endian Types === //

macro_rules! define_le {
    ($($name:ident $ty:ty),*$(,)?) => {$(
        #[derive(Copy, Clone, Pod, Zeroable)]
        #[repr(transparent)]
        pub struct $name($ty);

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.get().fmt(f)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.get().fmt(f)
            }
        }

		impl fmt::Binary for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.get().fmt(f)
            }
        }

		impl fmt::LowerHex for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.get().fmt(f)
            }
        }

		impl fmt::LowerExp for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.get().fmt(f)
            }
        }

		impl fmt::Octal for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.get().fmt(f)
            }
        }

		impl fmt::UpperExp for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.get().fmt(f)
            }
        }

        impl $name {
            pub fn new(value: $ty) -> Self {
                Self(value.to_le())
            }

            pub fn get(self) -> $ty {
                <$ty>::from_le(self.0)
            }

            pub fn set(&mut self, v: $ty) {
                *self = Self::new(v)
            }

            pub fn update<R>(&mut self, f: impl FnOnce(&mut $ty) -> R) -> R {
                let mut ne = self.get();
                let res = f(&mut ne);
                self.set(ne);
                res
            }

            pub fn map(self, f: impl FnOnce($ty) -> $ty) -> Self {
                f(self.into()).into()
            }
        }

        impl From<$ty> for $name {
            fn from(value: $ty) -> Self {
                Self::new(value)
            }
        }

        impl From<$name> for $ty {
            fn from(value: $name) -> Self {
                value.get()
            }
        }

        impl GuestMarshaledTy for $name {
            type GuestPrim = <$ty as GuestMarshaledTy>::GuestPrim;

            fn into_guest_prim(me: Self) -> Self::GuestPrim {
                <$ty>::into_guest_prim(me.get())
            }

            fn from_guest_prim(me: Self::GuestPrim) -> Option<Self> {
                <$ty>::from_guest_prim(me).map(Self::new)
            }
        }
    )*};
}

define_le! {
    LeI16 i16,
    LeU16 u16,
    LeI32 i32,
    LeU32 u32,
    LeI64 i64,
    LeU64 u64,
}

// === Pointers === //

// WasmPtr
#[repr(transparent)]
pub struct WasmPtr<T: 'static> {
    pub _ty: PhantomData<fn() -> T>,
    pub addr: LeU32,
}

impl<T> fmt::Debug for WasmPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.addr.get() as usize as *const T).fmt(f)
    }
}

impl<T> fmt::Pointer for WasmPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.addr.get() as usize as *const T).fmt(f)
    }
}

impl<T> Copy for WasmPtr<T> {}

impl<T> Clone for WasmPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> GuestMarshaledTy for WasmPtr<T> {
    type GuestPrim = u32;

    fn into_guest_prim(me: Self) -> Self::GuestPrim {
        GuestMarshaledTy::into_guest_prim(me.addr)
    }

    fn from_guest_prim(me: Self::GuestPrim) -> Option<Self> {
        GuestMarshaledTy::from_guest_prim(me).map(|addr| Self {
            _ty: PhantomData,
            addr,
        })
    }
}

unsafe impl<T> Pod for WasmPtr<T> {}
unsafe impl<T> Zeroable for WasmPtr<T> {}

// WasmSlice
pub struct WasmSlice<T: 'static> {
    pub base: WasmPtr<T>,
    pub len: LeU32,
}

impl<T> fmt::Debug for WasmSlice<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WasmSlice")
            .field("base", &self.base)
            .field("len", &self.len)
            .finish()
    }
}

impl<T> Copy for WasmSlice<T> {}

impl<T> Clone for WasmSlice<T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct WasmSliceRaw(u32, u32);

impl<T> GuestMarshaledTy for WasmSlice<T> {
    type GuestPrim = u64;

    fn into_guest_prim(me: Self) -> Self::GuestPrim {
        bytemuck::cast(WasmSliceRaw(me.base.addr.get(), me.len.get()))
    }

    fn from_guest_prim(me: Self::GuestPrim) -> Option<Self> {
        let WasmSliceRaw(base, len) = bytemuck::cast::<_, WasmSliceRaw>(me);

        Some(Self {
            base: WasmPtr {
                _ty: PhantomData,
                addr: LeU32::new(base),
            },
            len: LeU32::new(len),
        })
    }
}

unsafe impl<T: 'static> Pod for WasmSlice<T> {}
unsafe impl<T: 'static> Zeroable for WasmSlice<T> {}

// WasmStr
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
pub struct WasmStr(pub WasmSlice<u8>);

impl GuestMarshaledTy for WasmStr {
    type GuestPrim = u64;

    fn into_guest_prim(me: Self) -> Self::GuestPrim {
        WasmSlice::into_guest_prim(me.0)
    }

    fn from_guest_prim(me: Self::GuestPrim) -> Option<Self> {
        Some(WasmStr(WasmSlice::from_guest_prim(me).unwrap()))
    }
}

// WasmFuncOnGuest
pub struct WasmFuncOnGuest<A, R>(pub A::PrimFunc<R::GuestPrims>)
where
    A: GuestMarshaledTyList,
    R: GuestMarshaledTyList;

impl<A, R> WasmFuncOnGuest<A, R>
where
    A: GuestMarshaledTyList,
    R: GuestMarshaledTyList,
{
    pub fn new_guest<F: ZstFn<A, Output = R>>(f: F) -> Self {
        Self(A::wrap_prim_func(f))
    }
}

impl<A, R> Copy for WasmFuncOnGuest<A, R>
where
    A: GuestMarshaledTyList,
    R: GuestMarshaledTyList,
{
}

impl<A, R> Clone for WasmFuncOnGuest<A, R>
where
    A: GuestMarshaledTyList,
    R: GuestMarshaledTyList,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<A, R> GuestMarshaledTy for WasmFuncOnGuest<A, R>
where
    A: GuestMarshaledTyList,
    R: GuestMarshaledTyList,
{
    type GuestPrim = A::PrimFunc<R::GuestPrims>;

    fn into_guest_prim(me: Self) -> Self::GuestPrim {
        me.0
    }

    fn from_guest_prim(me: Self::GuestPrim) -> Option<Self> {
        Some(Self(me))
    }
}

// === Guest Constructors === //

// ...as per the suggestion of LegionMammal978 (https://github.com/LegionMammal978). Thanks!
fn slice_len<T>(mut ptr: *const [T]) -> usize {
    // This is done to avoid https://github.com/rust-lang/rust/issues/120440.
    if ptr.is_null() {
        ptr = ptr.wrapping_byte_add(1);
    }
    let ptr = unsafe { NonNull::new_unchecked(ptr.cast_mut()) };
    ptr.len()
}

fn usize_to_u32(v: usize) -> u32 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = v;
        panic!("attempted to call guest function on non-guest platform");
    }

    #[cfg(target_arch = "wasm32")]
    {
        v as u32
    }
}

fn u32_to_usize(v: u32) -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = v;
        panic!("attempted to call guest function on non-guest platform");
    }

    #[cfg(target_arch = "wasm32")]
    {
        v as usize
    }
}

impl<T> WasmPtr<T> {
    pub fn new_guest(ptr: *const T) -> Self {
        Self {
            _ty: PhantomData,
            addr: LeU32::new(usize_to_u32(ptr as usize)),
        }
    }

    pub fn into_guest(self) -> *mut T {
        u32_to_usize(self.addr.get()) as *mut T
    }

    #[cfg(feature = "alloc")]
    pub unsafe fn into_guest_box(self) -> alloc::boxed::Box<T> {
        alloc::boxed::Box::from_raw(self.into_guest())
    }
}

impl<T> WasmSlice<T> {
    pub fn new_guest(ptr: *const [T]) -> Self {
        Self {
            base: WasmPtr::new_guest(ptr.cast::<T>()),
            len: LeU32::new(usize_to_u32(slice_len(ptr))),
        }
    }

    pub fn into_guest(self) -> *mut [T] {
        ptr::slice_from_raw_parts_mut(self.base.into_guest(), u32_to_usize(self.len.get()))
    }

    #[cfg(feature = "alloc")]
    pub unsafe fn into_guest_vec(self) -> alloc::vec::Vec<T> {
        alloc::vec::Vec::from_raw_parts(
            self.into_guest() as *mut T,
            u32_to_usize(self.len.get()),
            u32_to_usize(self.len.get()),
        )
    }
}

impl WasmStr {
    pub fn new_guest(ptr: *const str) -> Self {
        Self(WasmSlice::new_guest(ptr as *const [u8]))
    }

    pub fn into_guest(self) -> *mut [u8] {
        self.0.into_guest()
    }

    #[cfg(feature = "alloc")]
    pub unsafe fn into_guest_string(self) -> alloc::string::String {
        alloc::string::String::from_utf8_unchecked(self.0.into_guest_vec())
    }
}

// === Generator === //

#[macro_export]
macro_rules! generate_guest_ffi {
    (
        $(
            $(#[$fn_attr:meta])*
            $vis:vis fn $module:literal.$fn_name:ident(
                $($arg_name:ident: $arg_ty:ty),*
                $(,)?
            ) $( -> $res_ty:ty )?;
        )*
    ) => {$(
        $(#[$fn_attr])*
        $vis unsafe fn $fn_name($($arg_name: $arg_ty),*) $(-> $res_ty)? {
            #[link(wasm_import_module = $module)]
            extern "C" {
                fn $fn_name(
                    $($arg_name: <$arg_ty as $crate::GuestMarshaledTy>::GuestPrim),*
                ) $(-> <$res_ty as $crate::GuestMarshaledTy>::GuestPrim)?;
            }

            $crate::GuestMarshaledTyList::from_guest_prims($fn_name(
                $($crate::GuestMarshaledTy::into_guest_prim($arg_name),)*
            ))
            .expect("failed to parse result")
        }
    )*};
}
