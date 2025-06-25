use std::mem;

use anyhow::Context;
use bytemuck::Pod;

pub trait MemoryExt {
    fn mem_elem_mut<T: Pod>(&mut self, addr: u32, len: u32) -> anyhow::Result<&mut [T]>;
}

impl MemoryExt for [u8] {
    fn mem_elem_mut<T: Pod>(&mut self, addr: u32, len: u32) -> anyhow::Result<&mut [T]> {
        let bytes = self
            .get_mut(addr as usize..)
            .context("memory base address too large")?
            .get_mut(
                ..mem::size_of::<T>()
                    .checked_mul(len as usize)
                    .context("arithmetic overflow during addressing")?,
            )
            .context("read past bounds of memory")?;

        let bytes =
            bytemuck::try_cast_slice_mut::<u8, T>(bytes).map_err(|v| anyhow::anyhow!("{v}"))?;

        Ok(bytes)
    }
}
