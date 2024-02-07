use std::ops::Range;

use blake3::Hash;
use rustc_hash::FxHashMap;

use crate::reloc::RelocEntry;

// === Writer === //

#[derive(Debug)]
pub struct WasmallArchive {
    pub out_buf: Vec<u8>,
    pub blob_buf: Vec<u8>,
    pub hashes: FxHashMap<Hash, Range<usize>>,
}

#[derive(Debug)]
pub struct WasmallWriter {
    archive: WasmallArchive,
}

impl Default for WasmallWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmallWriter {
    pub fn new() -> Self {
        todo!();
    }

    pub fn push_verbatim(&mut self, data: &[u8]) {
        todo!();
    }

    pub fn push_blob(&mut self) -> SingleBlobWriter<'_> {
        todo!();
    }

    pub fn finish(self) -> WasmallArchive {
        self.archive
    }
}

pub struct SingleBlobWriter<'a> {
    writer: &'a mut WasmallWriter,
}

impl SingleBlobWriter<'_> {
    pub fn push_reloc(&mut self, entry: RelocEntry, value_taken: u64) {
        todo!();
    }

    pub fn finish(
        &mut self,
        write_normalized: impl FnOnce(&mut Vec<u8>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        todo!();
        Ok(())
    }
}
