use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    mem,
    ops::Deref,
};

use derive_where::derive_where;

use crate::{
    FfiPtr, GuestboundOf, GuestboundViewOf, HostContext, Marshal, Strategy, StrategyOf,
    utils::{align_of_u32, is_wasm, size_of_u32},
};

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
    pub fn new_mut(f: impl 'static + FnMut(GuestboundOf<I>)) -> Self {
        let f = RefCell::new(f);
        Self::new(move |v| f.borrow_mut()(v))
    }

    #[must_use]
    pub fn new_once(f: impl 'static + FnOnce(GuestboundOf<I>)) -> Self {
        let f = Cell::new(Some(f));
        Self::new(move |v| f.take().expect("closure can only be called once")(v))
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
    fn new_unmanaged(f: impl 'static + Fn(GuestboundOf<I>)) -> Self {
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
