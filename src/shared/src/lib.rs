#![no_std]

use core::{fmt, marker::PhantomData};

use bytemuck::{Pod, Zeroable};

// === Little Endian Types === //

macro_rules! define_le {
    ($($name:ident $ty:ty),*$(,)?) => {$(
        // It's okay to hash and compare these in the wrong endianess.
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

unsafe impl<T: 'static> Pod for WasmPtr<T> {}
unsafe impl<T: 'static> Zeroable for WasmPtr<T> {}

// WasmSlice
#[repr(transparent)]
pub struct WasmSlice<T: 'static> {
    pub _ty: PhantomData<fn() -> T>,
    pub ptr: LeU64,
}

#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct RawWasmSlice {
    base: LeU32,
    len: LeU32,
}

impl<T> WasmSlice<T> {
    pub fn new_raw(base: WasmPtr<T>, len: LeU32) -> Self {
        bytemuck::cast(RawWasmSlice {
            base: base.addr,
            len,
        })
    }

    pub fn into_raw(self) -> (WasmPtr<T>, LeU32) {
        let RawWasmSlice { base, len } = bytemuck::cast(self);

        (
            WasmPtr {
                _ty: PhantomData,
                addr: base,
            },
            len,
        )
    }
}

impl<T> fmt::Debug for WasmSlice<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (base, len) = self.into_raw();

        f.debug_struct("WasmSlice")
            .field("base", &base)
            .field("len", &len)
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

// WasmStr
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
pub struct WasmStr(pub WasmSlice<u8>);

// === Host === //

#[cfg(feature = "host")]
mod host_ext {
    use super::*;

    impl<T> WasmPtr<T> {
        pub fn new_host(v: u32) -> Self {
            Self {
                _ty: PhantomData,
                addr: LeU32::new(v),
            }
        }
    }

    impl<T> WasmSlice<T> {
        pub fn new_host(v: u64) -> Self {
            Self {
                _ty: PhantomData,
                ptr: LeU64::new(v),
            }
        }
    }

    impl WasmStr {
        pub fn new_host(v: u64) -> Self {
            Self(WasmSlice::new_host(v))
        }
    }
}

// === Guest === //

#[cfg(feature = "guest")]
mod guest_ext {
    use super::*;
    use core::ptr::NonNull;

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
        v as u32
    }

    impl<T> WasmPtr<T> {
        #[cfg(feature = "guest")]
        pub fn new_guest(ptr: *const T) -> Self {
            Self {
                _ty: PhantomData,
                addr: LeU32::new(usize_to_u32(ptr as usize)),
            }
        }
    }

    impl<T> WasmSlice<T> {
        #[cfg(feature = "guest")]
        pub fn new_guest(ptr: *const [T]) -> Self {
            let len = slice_len(ptr);

            Self::new_raw(
                WasmPtr::new_guest(ptr.cast::<T>()),
                LeU32::new(usize_to_u32(len)),
            )
        }
    }

    impl WasmStr {
        #[cfg(feature = "guest")]
        pub fn new_guest(ptr: *const str) -> Self {
            Self(WasmSlice::new_guest(ptr as *const [u8]))
        }
    }
}

// === Raw Structures === //

#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
pub struct DemoStructure {
    pub funnies: WasmSlice<u32>,
}
