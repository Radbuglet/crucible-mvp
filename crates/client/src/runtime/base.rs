use std::fmt;

use anyhow::Context as _;
use late_struct::{LateField, LateInstance, late_field, late_struct};

// === RtState === //

pub type RtState = LateInstance<RtStateNs>;

#[non_exhaustive]
pub struct RtStateNs;

late_struct!(RtStateNs => dyn 'static + fmt::Debug);

pub trait RtFieldExt: LateField<RtStateNs> {
    fn get<'a>(store: impl Into<wasmtime::StoreContext<'a, RtState>>) -> &'a Self::Value {
        store.into().data().get::<Self>()
    }

    fn get_mut(store: &mut impl StoreDataMut<RtState>) -> &mut Self::Value {
        store.data_mut().get_mut::<Self>()
    }
}

impl<T: LateField<RtStateNs>> RtFieldExt for T {}

pub trait StoreDataMut<T: 'static> {
    fn data_mut(&mut self) -> &mut T;
}

impl<T> StoreDataMut<T> for wasmtime::Store<T> {
    fn data_mut(&mut self) -> &mut T {
        self.data_mut()
    }
}

impl<T> StoreDataMut<T> for wasmtime::Caller<'_, T> {
    fn data_mut(&mut self) -> &mut T {
        self.data_mut()
    }
}

impl<T> StoreDataMut<T> for wasmtime::StoreContextMut<'_, T> {
    fn data_mut(&mut self) -> &mut T {
        self.data_mut()
    }
}

// === RtModule === //

pub trait RtModule {
    fn define(linker: &mut wasmtime::Linker<RtState>) -> anyhow::Result<()>;
}

// === MainMemory === //

#[non_exhaustive]
pub struct MainMemory;

late_field!(MainMemory[RtStateNs] => Option<wasmtime::Memory>);

impl MainMemory {
    pub fn init(
        mut store: impl wasmtime::AsContextMut<Data = RtState>,
        instance: wasmtime::Instance,
    ) -> anyhow::Result<()> {
        let mut store = store.as_context_mut();

        let main_memory = instance
            .get_memory(&mut store, "memory")
            .context("failed to get main memory")?;

        *store.data_mut().get_mut::<MainMemory>() = Some(main_memory);

        Ok(())
    }

    pub fn data(store: &impl wasmtime::AsContext<Data = RtState>) -> &[u8] {
        MainMemory::get(store).unwrap().data(store)
    }

    pub fn data_mut(store: &mut impl wasmtime::AsContextMut<Data = RtState>) -> &mut [u8] {
        let mut store = store.as_context_mut();
        MainMemory::get(&mut store).unwrap().data_mut(store)
    }

    pub fn data_state_mut(
        store: &mut impl wasmtime::AsContextMut<Data = RtState>,
    ) -> (&mut [u8], &mut RtState) {
        let mut store = store.as_context_mut();
        MainMemory::get(&mut store)
            .unwrap()
            .data_and_store_mut(store)
    }
}
