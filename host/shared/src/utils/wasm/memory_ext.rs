use std::mem;

use anyhow::Context;
use bytemuck::Pod;

pub trait MemoryExt {
    fn mem_elem<T: Pod>(&self, addr: u32, len: u32) -> anyhow::Result<&[T]>;

    fn mem_elem_mut<T: Pod>(&mut self, addr: u32, len: u32) -> anyhow::Result<&mut [T]>;

    fn mem_arr<T: Pod, const N: usize>(&self, addr: u32) -> anyhow::Result<&[T; N]>;

    fn mem_arr_mut<T: Pod, const N: usize>(&mut self, addr: u32) -> anyhow::Result<&mut [T; N]>;
}

impl MemoryExt for [u8] {
    fn mem_elem<T: Pod>(&self, addr: u32, len: u32) -> anyhow::Result<&[T]> {
        let bytes = self
            .get(addr as usize..)
            .context("memory base address too large")?
            .get(
                ..mem::size_of::<T>()
                    .checked_mul(len as usize)
                    .context("arithmetic overflow during addressing")?,
            )
            .context("read past bounds of memory")?;

        let bytes = bytemuck::try_cast_slice::<u8, T>(bytes).map_err(|v| anyhow::anyhow!("{v}"))?;

        Ok(bytes)
    }

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

    fn mem_arr<T: Pod, const N: usize>(&self, addr: u32) -> anyhow::Result<&[T; N]> {
        self.mem_elem(addr, N as u32)
            .map(|v| &bytemuck::cast_slice::<T, [T; N]>(v)[0])
    }

    fn mem_arr_mut<T: Pod, const N: usize>(&mut self, addr: u32) -> anyhow::Result<&mut [T; N]> {
        self.mem_elem_mut(addr, N as u32)
            .map(|v| &mut bytemuck::cast_slice_mut::<T, [T; N]>(v)[0])
    }
}
