use std::fmt;

use anyhow::Context as _;
use late_struct::{LateField, LateInstance, late_field, late_struct};

use crate::utils::wasm::{StoreData, StoreDataMut};

// === RtState === //

pub type RtState = LateInstance<RtStateNs>;

#[non_exhaustive]
pub struct RtStateNs;

late_struct!(RtStateNs => dyn 'static + fmt::Debug);

pub trait RtFieldExt: LateField<RtStateNs> {
    fn get(store: &impl StoreData<Data = RtState>) -> &Self::Value {
        store.data().get::<Self>()
    }

    fn get_mut(store: &mut impl StoreDataMut<Data = RtState>) -> &mut Self::Value {
        store.data_mut().get_mut::<Self>()
    }
}

impl<T: LateField<RtStateNs>> RtFieldExt for T {}

pub trait RtOptFieldExt: LateField<RtStateNs, Value = Option<Self::Inner>> {
    type Inner: 'static;

    fn get_unwrap(store: &impl StoreData<Data = RtState>) -> &Self::Inner {
        Self::get(store).as_ref().unwrap()
    }

    fn get_unwrap_mut(store: &mut impl StoreDataMut<Data = RtState>) -> &mut Self::Inner {
        Self::get_mut(store).as_mut().unwrap()
    }
}

impl<I, T> RtOptFieldExt for T
where
    I: 'static,
    T: LateField<RtStateNs, Value = Option<I>>,
{
    type Inner = I;
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
        store: &mut impl StoreDataMut<Data = RtState>,
        instance: wasmtime::Instance,
    ) -> anyhow::Result<()> {
        let main_memory = instance
            .get_memory(&mut store.as_context_mut(), "memory")
            .context("failed to get main memory")?;

        *store.data_mut().get_mut::<MainMemory>() = Some(main_memory);

        Ok(())
    }

    pub fn data(store: &impl StoreData<Data = RtState>) -> &[u8] {
        MainMemory::get(&store.as_context()).unwrap().data(store)
    }

    pub fn data_mut(store: &mut impl StoreDataMut<Data = RtState>) -> &mut [u8] {
        MainMemory::get(store)
            .unwrap()
            .data_mut(store.as_context_mut())
    }

    pub fn data_state_mut(
        store: &mut impl StoreDataMut<Data = RtState>,
    ) -> (&mut [u8], &mut RtState) {
        MainMemory::get(store)
            .unwrap()
            .data_and_store_mut(store.as_context_mut())
    }
}
