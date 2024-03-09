#![no_std]
#![allow(clippy::missing_safety_doc)]

#[cfg(feature = "alloc")]
extern crate alloc;

use core::{
    any::type_name,
    fmt,
    marker::PhantomData,
    ptr::{self, NonNull},
};

use bytemuck::{Pod, Zeroable};

// === Macro Re-exports === //

#[doc(hidden)]
pub mod macro_rexp {
    pub use core::option::Option;
}

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

// === WasmPrimitive === //

#[cfg(feature = "wasmtime")]
mod sealed {
    pub trait WasmPrimitive: wasmtime::WasmTy {}

    pub trait WasmPrimitiveList:
        wasmtime::WasmRet + wasmtime::WasmResults + wasmtime::WasmParams
    {
    }
}

#[cfg(not(feature = "wasmtime"))]
mod sealed {
    pub trait WasmPrimitive {}

    pub trait WasmPrimitiveList {}
}

pub trait WasmPrimitive: sealed::WasmPrimitive {}

pub trait WasmPrimitiveList: sealed::WasmPrimitiveList {}

macro_rules! impl_wasm_primitive {
    ($($ty:ty),*$(,)?) => {
        $(impl sealed::WasmPrimitive for $ty {})*
        $(impl WasmPrimitive for $ty {})*
    };
}

macro_rules! impl_wasm_primitive_list {
    ($($param:ident)*) => {
        impl<$($param: WasmPrimitive),*> sealed::WasmPrimitiveList for ($($param,)*) {}
        impl<$($param: WasmPrimitive),*> WasmPrimitiveList for ($($param,)*) {}
    };
}

impl_wasm_primitive!(u32, i32, u64, i64, f32, f64);
impl_variadic!(impl_wasm_primitive_list);

impl<T: WasmPrimitive> sealed::WasmPrimitiveList for T {}

impl<T: WasmPrimitive> WasmPrimitiveList for T {}

// === MarshaledTy === //

pub trait MarshaledTy: Sized + 'static {
    type Prim: WasmPrimitive;

    fn into_prim(me: Self) -> Self::Prim;

    fn from_prim(me: Self::Prim) -> Option<Self>;
}

#[macro_export]
macro_rules! forward_marshaled_ty {
    ($ty:ty, get |$getter_me:pat_param $(,)?| $getter:expr, new |$ctor_me:pat_param $(,)?| $ctor:expr $(,)?) => {
        type Prim = <$ty as $crate::MarshaledTy>::Prim;

        fn into_prim($getter_me: Self) -> Self::Prim {
            <$ty as $crate::MarshaledTy>::into_prim($getter)
        }

        fn from_prim(me: Self::Prim) -> $crate::macro_rexp::Option<Self> {
            let $ctor_me = <$ty as $crate::MarshaledTy>::from_prim(me)?;
            $ctor
        }
    };
    ($ty:ty) => {
        type Prim = <$ty as $crate::MarshaledTy>::Prim;

        fn into_prim(me: Self) -> Self::Prim {
            <$ty as $crate::MarshaledTy>::into_prim(me.0)
        }

        fn from_prim(me: Self::Prim) -> $crate::macro_rexp::Option<Self> {
            <$ty as $crate::MarshaledTy>::from_prim(me).map(Self)
        }
    };
}

macro_rules! impl_func_ty {
    ($($ty:ty => $prim:ty),*$(,)?) => {$(
        impl MarshaledTy for $ty {
            type Prim = $prim;

            fn into_prim(me: Self) -> Self::Prim {
                me.into()
            }

            fn from_prim(me: Self::Prim) -> Option<Self> {
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

impl MarshaledTy for bool {
    type Prim = u32;

    fn into_prim(me: Self) -> Self::Prim {
        me as u32
    }

    fn from_prim(me: Self::Prim) -> Option<Self> {
        match me {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
}

// === MarshaledTyList === //

// Core
pub struct ConcretePrimFuncWrapper<T, F, R>(T, F, R);

pub trait PrimFuncWrapper {
    const FUNC: *const ();
}

pub trait MarshaledTyList: Sized + 'static {
    type Prims: WasmPrimitiveList;

    type WrapPrimFuncOnGuest<F, R>: PrimFuncWrapper
    where
        F: ZstFn<Self, Output = R>,
        R: MarshaledTyList;

    fn into_prims(me: Self) -> Self::Prims;

    fn from_prims(me: Self::Prims) -> Option<Self>;
}

// Derivations
impl<T: MarshaledTy, F, R> PrimFuncWrapper for ConcretePrimFuncWrapper<T, F, R>
where
    F: ZstFn<T, Output = R>,
    R: MarshaledTyList,
{
    const FUNC: *const () = {
        let f = |arg| {
            let arg = T::from_prim(arg).unwrap();
            let res = unsafe { F::call_static(arg) };
            R::into_prims(res)
        };
        f as fn(T::Prim) -> R::Prims as *const ()
    };
}

impl<T: MarshaledTy> MarshaledTyList for T {
    type Prims = T::Prim;

    type WrapPrimFuncOnGuest<F, R> = ConcretePrimFuncWrapper<Self, F, R>
    where
        F: ZstFn<Self, Output = R>,
        R: MarshaledTyList;

    fn into_prims(me: Self) -> Self::Prims {
        T::into_prim(me)
    }

    fn from_prims(me: Self::Prims) -> Option<Self> {
        T::from_prim(me)
    }
}

macro_rules! impl_marshaled_res_ty {
    ($($para:ident)*) => {
        #[allow(non_snake_case)]
        impl<$($para: MarshaledTy,)* F, R> PrimFuncWrapper for ConcretePrimFuncWrapper<($($para,)*), F, R>
        where
            F: ZstFn<($($para,)*), Output = R>,
            R: MarshaledTyList,
        {
            const FUNC: *const () = {
                let f = |$($para,)*| {
                    let arg = <($($para,)*)>::from_prims(($($para,)*)).unwrap();
                    let res = unsafe { F::call_static(arg) };
                    R::into_prims(res)
                };

                f as fn($(<$para as MarshaledTy>::Prim,)*) -> R::Prims as *const ()
            };
        }

        impl<$($para: MarshaledTy,)*> MarshaledTyList for ($($para,)*) {
            type Prims = ($(<$para as MarshaledTy>::Prim,)*);

            type WrapPrimFuncOnGuest<F, R> = ConcretePrimFuncWrapper<Self, F, R>
            where
                F: ZstFn<Self, Output = R>,
                R: MarshaledTyList;

            #[allow(clippy::unused_unit, non_snake_case)]
            fn into_prims(($($para,)*): Self) -> Self::Prims {
                ( $(MarshaledTy::into_prim($para),)* )
            }

            #[allow(non_snake_case)]
            fn from_prims(($($para,)*): Self::Prims) -> Option<Self> {
                Some(( $(MarshaledTy::from_prim($para)?,)* ))
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
            pub const fn new(value: $ty) -> Self {
                Self(value.to_le())
            }

            pub const fn get(self) -> $ty {
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

        impl MarshaledTy for $name {
            forward_marshaled_ty!($ty, get |me| me.get(), new |prim| Some(Self::new(prim)));
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
    _ty: PhantomData<fn() -> T>,
    addr: WasmPtrAddr,
}

#[derive(Copy, Clone)]
union WasmPtrAddr {
    addr: LeU32,
    #[cfg(target_arch = "wasm32")]
    func: *const (),
}

impl<T> fmt::Debug for WasmPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.addr().get() as usize as *const T).fmt(f)
    }
}

impl<T> fmt::Pointer for WasmPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.addr().get() as usize as *const T).fmt(f)
    }
}

impl<T> Copy for WasmPtr<T> {}

impl<T> Clone for WasmPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<T> Pod for WasmPtr<T> {}
unsafe impl<T> Zeroable for WasmPtr<T> {}

impl<T> WasmPtr<T> {
    pub const fn new(addr: LeU32) -> Self {
        Self {
            _ty: PhantomData,
            addr: WasmPtrAddr { addr },
        }
    }

    pub fn addr(self) -> LeU32 {
        unsafe { self.addr.addr }
    }
}

impl<T> MarshaledTy for WasmPtr<T> {
    type Prim = u32;

    fn into_prim(me: Self) -> Self::Prim {
        MarshaledTy::into_prim(me.addr().get())
    }

    fn from_prim(me: Self::Prim) -> Option<Self> {
        MarshaledTy::from_prim(me).map(Self::new)
    }
}

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

unsafe impl<T: 'static> Pod for WasmSlice<T> {}
unsafe impl<T: 'static> Zeroable for WasmSlice<T> {}

#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct WasmSliceConverter(u32, u32);

impl<T> MarshaledTy for WasmSlice<T> {
    type Prim = u64;

    fn into_prim(me: Self) -> Self::Prim {
        bytemuck::cast(WasmSliceConverter(me.base.addr().get(), me.len.get()))
    }

    fn from_prim(me: Self::Prim) -> Option<Self> {
        let WasmSliceConverter(base, len) = bytemuck::cast::<_, WasmSliceConverter>(me);

        Some(Self {
            base: WasmPtr::new(LeU32::new(base)),
            len: LeU32::new(len),
        })
    }
}

// WasmStr
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
pub struct WasmStr(pub WasmSlice<u8>);

impl MarshaledTy for WasmStr {
    type Prim = u64;

    fn into_prim(me: Self) -> Self::Prim {
        WasmSlice::into_prim(me.0)
    }

    fn from_prim(me: Self::Prim) -> Option<Self> {
        Some(WasmStr(WasmSlice::from_prim(me).unwrap()))
    }
}

// WasmFunc
pub struct WasmFunc<A, R = ()>
where
    A: MarshaledTyList,
    R: MarshaledTyList,
{
    _ty: PhantomData<(A, R)>,
    addr: WasmPtr<()>,
}

impl<A, R> fmt::Debug for WasmFunc<A, R>
where
    A: MarshaledTyList,
    R: MarshaledTyList,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "fn({}) -> {} @ {:X}",
            type_name::<A>(),
            type_name::<R>(),
            self.addr().get(),
        )
    }
}

impl<A, R> Copy for WasmFunc<A, R>
where
    A: MarshaledTyList,
    R: MarshaledTyList,
{
}

impl<A, R> Clone for WasmFunc<A, R>
where
    A: MarshaledTyList,
    R: MarshaledTyList,
{
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<A, R> Pod for WasmFunc<A, R>
where
    A: MarshaledTyList,
    R: MarshaledTyList,
{
}

unsafe impl<A, R> Zeroable for WasmFunc<A, R>
where
    A: MarshaledTyList,
    R: MarshaledTyList,
{
}

impl<A, R> WasmFunc<A, R>
where
    A: MarshaledTyList,
    R: MarshaledTyList,
{
    pub const fn new(addr: WasmPtr<()>) -> Self {
        Self {
            _ty: PhantomData,
            addr,
        }
    }

    pub fn addr(self) -> LeU32 {
        self.addr.addr()
    }
}

impl<A, R> MarshaledTy for WasmFunc<A, R>
where
    A: MarshaledTyList,
    R: MarshaledTyList,
{
    type Prim = u32;

    fn into_prim(me: Self) -> Self::Prim {
        me.addr().get()
    }

    fn from_prim(me: Self::Prim) -> Option<Self> {
        Some(Self {
            _ty: PhantomData,
            addr: WasmPtr::new(LeU32::new(me)),
        })
    }
}

// WasmWidePtrRaw
pub struct WasmWidePtrRaw<M: 'static, T: 'static = ()> {
    pub base: WasmPtr<T>,
    pub meta: WasmPtr<M>,
}

impl<M, T> fmt::Debug for WasmWidePtrRaw<M, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WasmWidePtr")
            .field("base", &self.base)
            .field("meta", &self.meta)
            .finish()
    }
}

impl<M, T> Copy for WasmWidePtrRaw<M, T> {}

impl<M, T> Clone for WasmWidePtrRaw<M, T> {
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<M, T> Pod for WasmWidePtrRaw<M, T> {}
unsafe impl<M, T> Zeroable for WasmWidePtrRaw<M, T> {}

#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct WasmWidePtrConverter(u32, u32);

impl<M, T> MarshaledTy for WasmWidePtrRaw<M, T> {
    type Prim = u64;

    fn into_prim(me: Self) -> Self::Prim {
        bytemuck::cast(WasmWidePtrConverter(
            me.base.addr().get(),
            me.meta.addr().get(),
        ))
    }

    fn from_prim(me: Self::Prim) -> Option<Self> {
        let WasmWidePtrConverter(base, meta) = bytemuck::cast::<_, WasmWidePtrConverter>(me);

        Some(Self {
            base: WasmPtr::new(LeU32::new(base)),
            meta: WasmPtr::new(LeU32::new(meta)),
        })
    }
}

// WasmDynamic
pub struct WasmDynamic<V: 'static>(pub WasmWidePtrRaw<WasmVtable<V>>);

impl<V> fmt::Debug for WasmDynamic<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WasmDynamic")
            .field("base", &self.0.base)
            .field("meta", &self.0.meta)
            .finish()
    }
}

impl<V> Copy for WasmDynamic<V> {}

impl<V> Clone for WasmDynamic<V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V> MarshaledTy for WasmDynamic<V> {
    forward_marshaled_ty!(WasmWidePtrRaw<WasmVtable<V>>);
}

unsafe impl<V> Pod for WasmDynamic<V> {}
unsafe impl<V> Zeroable for WasmDynamic<V> {}

// WasmDynamic helpers
#[repr(C)]
pub struct WasmVtable<V: 'static> {
    pub dtor: WasmFunc<(WasmPtr<()>, WasmPtr<Self>)>,
    pub vtable: WasmPtr<V>,
    pub needs_drop: LeU32,
}

impl<V> Copy for WasmVtable<V> {}

impl<V> Clone for WasmVtable<V> {
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<V> Pod for WasmVtable<V> {}
unsafe impl<V> Zeroable for WasmVtable<V> {}

pub trait WasmContainer: Sized {
    fn into_raw(self) -> *mut ();

    unsafe fn from_raw(ptr: *mut ()) -> Self;
}

pub trait WasmUnsize<T>: Sized + 'static {
    const TABLE: &'static Self;
}

// === WasmDynamic Impls === //

impl<T> WasmContainer for &'_ T {
    fn into_raw(self) -> *mut () {
        self as *const T as *const () as *mut ()
    }

    unsafe fn from_raw(ptr: *mut ()) -> Self {
        &*(ptr as *const T)
    }
}

impl<T> WasmContainer for &'_ mut T {
    fn into_raw(self) -> *mut () {
        self as *mut T as *mut ()
    }

    unsafe fn from_raw(ptr: *mut ()) -> Self {
        &mut *(ptr as *mut T)
    }
}

#[cfg(feature = "alloc")]
impl<T> WasmContainer for alloc::boxed::Box<T> {
    fn into_raw(self) -> *mut () {
        alloc::boxed::Box::into_raw(self).cast()
    }

    unsafe fn from_raw(ptr: *mut ()) -> Self {
        alloc::boxed::Box::from_raw(ptr.cast())
    }
}

impl<T> WasmUnsize<T> for () {
    const TABLE: &'static Self = &();
}

impl<A, R, F> WasmUnsize<F> for WasmFunc<A, R>
where
    A: MarshaledTyList,
    R: MarshaledTyList,
    F: Send + Sync + Fn(A) -> R,
{
    const TABLE: &'static Self = &WasmFunc::<A, R>::new_guest(&|args| {
        todo!();
    });
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

pub const fn guest_usize_to_u32(v: usize) -> u32 {
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

pub const fn guest_u32_to_usize(v: u32) -> usize {
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
    pub const fn new_guest(ptr: *const T) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = ptr;
            panic!("attempted to call guest function on non-guest platform");
        }

        #[cfg(target_arch = "wasm32")]
        {
            Self {
                _ty: PhantomData,
                addr: WasmPtrAddr { ptr },
            }
        }
    }

    pub fn into_guest(self) -> *mut T {
        guest_u32_to_usize(self.addr().get()) as *mut T
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
            len: LeU32::new(guest_usize_to_u32(slice_len(ptr))),
        }
    }

    pub fn into_guest(self) -> *mut [T] {
        ptr::slice_from_raw_parts_mut(self.base.into_guest(), guest_u32_to_usize(self.len.get()))
    }

    #[cfg(feature = "alloc")]
    pub unsafe fn into_guest_vec(self) -> alloc::vec::Vec<T> {
        alloc::vec::Vec::from_raw_parts(
            self.into_guest() as *mut T,
            guest_u32_to_usize(self.len.get()),
            guest_u32_to_usize(self.len.get()),
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

impl<A, R> WasmFunc<A, R>
where
    A: MarshaledTyList,
    R: MarshaledTyList,
{
    pub const fn new_guest<F: ZstFn<A, Output = R>>(f: &'static F) -> Self {
        let _ = f;
        Self::new(WasmPtr::new_guest(<A::WrapPrimFuncOnGuest<F, R>>::FUNC))
    }

    pub const fn new_guest_raw(f: *const ()) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = f;
            panic!("attempted to call guest function on non-guest platform");
        }

        #[cfg(target_arch = "wasm32")]
        {
            Self {
                _ty: PhantomData,
                addr: WasmFuncAddr { func: f },
            }
        }
    }
}

impl<V> WasmDynamic<V> {
    pub fn new_guest<T>(value: T) -> Self
    where
        T: WasmContainer,
        V: WasmUnsize<T>,
    {
        struct VtableHelper<T, V>(T, V);

        impl<T, V> VtableHelper<T, V>
        where
            T: WasmContainer,
            V: WasmUnsize<T>,
        {
            guest_export! {
                fn dtor<T2, V2>(ptr: WasmPtr<()>, _meta: WasmPtr<WasmVtable<V2>>)
                where [T2: WasmContainer]
                {
                    drop(unsafe { T2::from_raw(ptr.into_guest()) });
                }
            }

            const TABLE: &'static WasmVtable<V> = &WasmVtable {
                dtor: Self::dtor::<T, V>(),
                vtable: WasmPtr::new_guest(V::TABLE),
                needs_drop: LeU32::new(core::mem::needs_drop::<T>() as u32),
            };
        }

        Self(WasmWidePtrRaw {
            base: WasmPtr::new_guest(value.into_raw().cast()),
            meta: WasmPtr::new_guest(VtableHelper::<T, V>::TABLE),
        })
    }
}

// === Generator === //

#[macro_export]
macro_rules! guest_import {
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
                    $($arg_name: <$arg_ty as $crate::MarshaledTy>::Prim),*
                ) $(-> <$res_ty as $crate::MarshaledTy>::Prim)?;
            }

            $crate::MarshaledTyList::from_prims($fn_name(
                $($crate::MarshaledTy::into_prim($arg_name),)*
            ))
            .expect("failed to parse result")
        }
    )*};
}

#[macro_export]
macro_rules! guest_export {
    ($(
        $(#[$attr:meta])*
        $vis:vis fn $fn_name:ident $(<$($generic:ident),*$(,)?>)? ($($arg:ident: $ty:ty),* $(,)?) $(-> $res:ty)?
        $(where [ $($where_clause:tt)* ])? {
            $($body:tt)*
        }
    )*) => {$(
        #[allow(unused_parens, unused)]
        $vis const fn $fn_name $(<$($generic),*>)? () -> WasmFunc<($($ty),*) $(, $res)?>
        $(where $($where_clause)*)?
        {
            $(#[$attr])*
            unsafe extern "C" fn $fn_name $(<$($generic),*>)? ($($arg: <$ty as $crate::MarshaledTy>::Prim),*)
                $(-> <$res as $crate::MarshaledTy>::Prim)?
                $(where $($where_clause)*)?
            {
                fn inner $(<$($generic),*>)? ($($arg: $ty),*) $(-> $res)?
                $(where $($where_clause)*)?
                {
                    $($body)*
                }

                $crate::MarshaledTyList::into_prims(inner::<$($($generic,)*)?>(
                    $($crate::MarshaledTy::from_prim($arg).expect("failed to parse result"),)*
                ))
            }

            $crate::WasmFunc::new_guest_raw($fn_name::<$($($generic,)*)?> as *const ())
        }
    )*};
}
