use anyhow::Context;
use derive_where::derive_where;

#[derive(Debug)]
#[derive_where(Default)]
pub struct GuestArena<T> {
    slots: Vec<Option<T>>,
    free: Vec<u32>,
}

impl<T> GuestArena<T> {
    pub fn next_handle(&self) -> anyhow::Result<u32> {
        match self.free.last() {
            Some(handle) => Ok(*handle),
            None => self.last_slot_handle(),
        }
    }

    pub fn add(&mut self, value: T) -> anyhow::Result<u32> {
        if let Some(handle) = self.free.pop() {
            self.slots[(handle - 1) as usize] = Some(value);

            return Ok(handle);
        }

        let idx = self.last_slot_handle()?;
        self.slots.push(Some(value));

        Ok(idx)
    }

    fn last_slot_handle(&self) -> anyhow::Result<u32> {
        self.slots
            .len()
            .checked_add(1)
            .and_then(|v| u32::try_from(v).ok())
            .context("too many slots")
    }

    fn handle_to_idx(handle: u32) -> anyhow::Result<usize> {
        handle
            .checked_sub(1)
            .context("zero is never a valid handle into the arena")
            .map(|v| v as usize)
    }

    pub fn remove(&mut self, handle: u32) -> anyhow::Result<T> {
        let value = self
            .slots
            .get_mut(Self::handle_to_idx(handle)?)
            .context("handle is past arena length")?
            .take()
            .context("slot is empty")?;

        self.free.push(handle);

        Ok(value)
    }

    pub fn get(&self, handle: u32) -> anyhow::Result<&T> {
        self.slots
            .get(Self::handle_to_idx(handle)?)
            .and_then(|v| v.as_ref())
            .context("handle is invalid")
    }

    pub fn get_mut(&mut self, handle: u32) -> anyhow::Result<&mut T> {
        self.slots
            .get_mut(Self::handle_to_idx(handle)?)
            .and_then(|v| v.as_mut())
            .context("handle is invalid")
    }
}
