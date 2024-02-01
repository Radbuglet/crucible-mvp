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
    LeI32 i32,
    LeU32 u32,
    LeI64 i64,
    LeU64 u64,
}

// === Pointers === //

#[derive(Pod, Zeroable)]
#[repr(transparent)]
pub struct WasmPtr<T> {
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

impl<T> WasmPtr<T> {
    #[cfg(feature = "guest")]
    pub fn new_guest(ptr: *const T) -> Self {
        Self {
            _ty: PhantomData,
            addr: LeU32::new(ptr as u32),
        }
    }
}

#[repr(C)]
pub struct WasmSlice<T> {
    pub start: WasmPtr<T>,
    pub len: LeU32,
}

impl<T> fmt::Debug for WasmSlice<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WasmSlice")
            .field("start", &self.start)
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

impl<T> WasmSlice<T> {
    #[cfg(feature = "guest")]
    pub fn new_guest(ptr: *const [T]) -> Self {
        use core::ptr::NonNull;

        let ptr = NonNull::new(ptr as *mut [T]).unwrap(); // N.B. pointers to slices can never be null

        Self {
            start: WasmPtr::new_guest(ptr.as_ptr().cast::<T>()),
            len: LeU32::new(ptr.len() as u32),
        }
    }
}

// === Raw Structures === //

#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
pub struct DemoStructure {
    pub funnies: WasmSlice<u32>,
}
