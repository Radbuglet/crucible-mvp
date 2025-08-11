use std::{marker::PhantomData, ptr::NonNull};

use anyhow::Context;
use arid::World;
use wasmlink::{
    BUILTIN_CLOSURE_INVOKE, BUILTIN_MEM_ALLOC, FfiPtr, GuestboundViewOf, HostContext,
    HostMarshalError, HostboundViewOf, Marshal, Port, Strategy,
};

// === WslStoreState === //

#[derive(Default)]
pub struct WslStoreState {
    world: Option<NonNull<World>>,
    exports: Option<WslExports>,
}

struct WslExports {
    memory: wasmtime::Memory,
    mem_alloc: wasmtime::TypedFunc<(u32, u32), u32>,
    closure_invoke: wasmtime::TypedFunc<(u64, u32), ()>,
}

pub trait WslStoreExt {
    fn setup_exports(&mut self, instance: wasmtime::Instance) -> anyhow::Result<()>;

    fn root<R>(&mut self, world: &mut World, f: impl FnOnce(WslContext<'_>) -> R) -> R;
}

impl WslStoreExt for wasmtime::Store<WslStoreState> {
    fn setup_exports(&mut self, instance: wasmtime::Instance) -> anyhow::Result<()> {
        let exports = WslExports {
            memory: instance
                .get_memory(&mut *self, "memory")
                .context("failed to find guest memory export")?,
            mem_alloc: instance
                .get_typed_func(&mut *self, BUILTIN_MEM_ALLOC)
                .with_context(|| format!("failed to find {BUILTIN_MEM_ALLOC}"))?,
            closure_invoke: instance
                .get_typed_func(&mut *self, BUILTIN_CLOSURE_INVOKE)
                .with_context(|| format!("failed to find {BUILTIN_MEM_ALLOC}"))?,
        };

        self.data_mut().exports = Some(exports);

        Ok(())
    }

    fn root<R>(&mut self, world: &mut World, f: impl FnOnce(WslContext<'_>) -> R) -> R {
        assert!(self.data().world.is_none());

        self.data_mut().world = Some(NonNull::from(world));

        let mut me = scopeguard::guard(self, |me| {
            me.data_mut().world = None;
        });

        f(WslContext(WslContextInner::Root(&mut me)))
    }
}

// === WslContext === //

pub struct WslContext<'a>(WslContextInner<'a>);

enum WslContextInner<'a> {
    Root(&'a mut wasmtime::Store<WslStoreState>),
    Call(wasmtime::Caller<'a, WslStoreState>),
}

impl WslContext<'_> {
    pub fn wr(&self) {}

    pub fn w(&mut self) {}

    fn cx(&self) -> wasmtime::StoreContext<'_, WslStoreState> {
        match &self.0 {
            WslContextInner::Root(store) => store.into(),
            WslContextInner::Call(caller) => caller.into(),
        }
    }

    fn cx_mut(&mut self) -> wasmtime::StoreContextMut<'_, WslStoreState> {
        match &mut self.0 {
            WslContextInner::Root(store) => store.into(),
            WslContextInner::Call(caller) => caller.into(),
        }
    }

    fn exports(&self) -> &WslExports {
        self.cx()
            .data()
            .exports
            .as_ref()
            .expect("exports never initialized with `WslStoreExt::setup_exports")
    }
}

impl HostContext for WslContext<'_> {
    fn guest_memory(&self) -> &[u8] {
        self.exports().memory.data(self.cx())
    }

    fn guest_memory_mut(&mut self) -> &mut [u8] {
        self.exports().memory.clone().data_mut(self.cx_mut())
    }

    fn alloc(&mut self, align: u32, size: u32) -> Result<FfiPtr<()>, HostMarshalError> {
        self.exports()
            .mem_alloc
            .clone()
            .call(self.cx_mut(), (align, size))
            .map(FfiPtr::new)
            .map_err(|_| HostMarshalError("failed to allocate memory on guest"))
    }

    fn invoke(&mut self, id: u64, boxed_arg: u32) -> Result<(), HostMarshalError> {
        self.exports()
            .closure_invoke
            .clone()
            .call(self.cx_mut(), (id, boxed_arg))
            .map_err(|_| HostMarshalError("failed to invoke guest closure"))
    }
}

// === Function Definitions === //

pub fn define<I, O, F>(
    linker: &mut wasmtime::Linker<WslStoreState>,
    port: Port<I, O>,
    func: F,
) -> anyhow::Result<()>
where
    I: Marshal,
    O: Marshal,
    F: 'static
        + Send
        + Sync
        + for<'t> Fn(
            WslContext<'_>,
            HostboundViewOf<I>,
            Returner<'t, O>,
        ) -> anyhow::Result<RetVal<'t>>,
{
    linker.func_wrap(
        port.module(),
        port.func_name(),
        move |cx: wasmtime::Caller<'_, WslStoreState>,
              in_addr: u32,
              out_addr: u32|
              -> anyhow::Result<()> {
            let cx = WslContext(WslContextInner::Call(cx));
            let in_view = <I::Strategy>::decode_hostbound(&cx, FfiPtr::new(in_addr))?;
            let returner = Returner {
                _ty: PhantomData,
                _invariant: PhantomData,
                out_addr,
            };

            func(cx, in_view, returner).map(|_| ())
        },
    )?;

    Ok(())
}

pub struct Returner<'t, O: Marshal> {
    _ty: PhantomData<fn(O) -> O>,
    _invariant: PhantomData<fn(&'t ()) -> &'t ()>,
    out_addr: u32,
}

impl<'t, O: Marshal> Returner<'t, O> {
    pub fn finish(
        self,
        mut cx: WslContext<'_>,
        value: &GuestboundViewOf<'_, O>,
    ) -> anyhow::Result<RetVal<'t>> {
        <O::Strategy>::encode_guestbound(&mut cx, FfiPtr::new(self.out_addr), value)?;

        Ok(RetVal {
            _invariant: PhantomData,
        })
    }
}

pub struct RetVal<'t> {
    _invariant: PhantomData<fn(&'t ()) -> &'t ()>,
}
