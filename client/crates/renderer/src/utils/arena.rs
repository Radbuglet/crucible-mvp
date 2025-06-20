use anyhow::Context;

#[derive(Debug, Clone)]
pub struct Arena<T> {
    values: Vec<Option<T>>,
    free: Vec<u32>,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    pub const fn new() -> Self {
        Self {
            values: Vec::new(),
            free: Vec::new(),
        }
    }

    pub fn add(&mut self, value: T) -> anyhow::Result<u32> {
        if let Some(handle) = self.free.pop() {
            self.values[Self::handle_to_idx_unchecked(handle)] = Some(value);
            return Ok(handle);
        }

        let idx = self.values.len();
        let handle = Self::idx_to_handle(idx)?;
        self.values.push(Some(value));

        Ok(handle)
    }

    pub fn remove(&mut self, handle: u32) -> anyhow::Result<T> {
        let idx = Self::handle_to_idx(handle)?;

        self.values
            .get_mut(idx)
            .context("handle was past length of arena")?
            .take()
            .context("slot was double-freed")
    }

    pub fn get(&self, handle: u32) -> anyhow::Result<&T> {
        self.values
            .get(Self::handle_to_idx(handle)?)
            .context("handle was past length of arena")?
            .as_ref()
            .context("attempted to access free slot")
    }

    pub fn get_mut(&mut self, handle: u32) -> anyhow::Result<&mut T> {
        self.values
            .get_mut(Self::handle_to_idx(handle)?)
            .context("handle was past length of arena")?
            .as_mut()
            .context("attempted to access free slot")
    }

    fn handle_to_idx_unchecked(handle: u32) -> usize {
        (handle - 1) as usize
    }

    fn handle_to_idx(handle: u32) -> anyhow::Result<usize> {
        let idx = handle.checked_sub(1).context(
            "attempted to access arena with handle with value `0`, which is never valid",
        )?;

        Ok(idx as usize)
    }

    fn idx_to_handle(idx: usize) -> anyhow::Result<u32> {
        idx.checked_add(1)
            .and_then(|v| u32::try_from(v).ok())
            .context("too many elements allocated in arena")
    }
}
