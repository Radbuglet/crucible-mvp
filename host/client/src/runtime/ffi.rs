use derive_where::derive_where;
use late_struct::late_field;

use crate::{
    runtime::base::{RtFieldExt as _, RtOptFieldExt as _, RtStateNs},
    utils::{memory::MemoryExt, wasmtime::StoreDataMut},
};

use super::base::{MainMemory, RtState};

#[derive_where(Debug)]
pub struct RtFfi {
    #[derive_where(skip)]
    alloc: wasmtime::TypedFunc<(u32, u32), u32>,
}

late_field!(RtFfi[RtStateNs] => Option<RtFfi>);

impl RtFfi {
    pub fn init(
        store: &mut wasmtime::Store<RtState>,
        instance: wasmtime::Instance,
    ) -> anyhow::Result<()> {
        *Self::get_mut(&mut *store) = Some(Self {
            alloc: instance.get_typed_func(&mut *store, "crucible_mem_alloc")?,
        });

        Ok(())
    }

    pub fn alloc(
        store: &mut impl StoreDataMut<Data = RtState>,
        size: u32,
        align: u32,
    ) -> anyhow::Result<u32> {
        let ptr = Self::get_unwrap(&*store)
            .alloc
            .clone()
            .call(&mut *store, (size, align))?;

        if ptr == 0 {
            anyhow::bail!("failed to allocate {size} byte(s) with alignment {align}");
        }

        Ok(ptr)
    }

    pub fn alloc_str(
        store: &mut impl StoreDataMut<Data = RtState>,
        text: &str,
    ) -> anyhow::Result<(u32, u32)> {
        let base = Self::alloc(store, text.len() as u32, 1)?;

        MainMemory::data_mut(store)
            .mem_elem_mut(base, text.len() as u32)?
            .copy_from_slice(text.as_bytes());

        Ok((base, text.len() as u32))
    }

    pub fn alloc_opt_str(
        store: &mut impl StoreDataMut<Data = RtState>,
        text: Option<&str>,
    ) -> anyhow::Result<(u32, u32)> {
        let Some(text) = text else {
            return Ok((0, 0));
        };

        Self::alloc_str(store, text)
    }
}
