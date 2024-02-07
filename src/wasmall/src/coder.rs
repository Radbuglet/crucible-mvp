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
        Self {
            archive: WasmallArchive {
                out_buf: Vec::new(),
                blob_buf: Vec::new(),
                hashes: FxHashMap::default(),
            },
        }
    }

    pub fn push_verbatim<R>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> R) -> R {
        f(&mut self.archive.out_buf)
    }

    pub fn push_blob(&mut self) -> SingleBlobWriter<'_> {
        SingleBlobWriter { writer: self }
    }

    pub fn finish(self) -> WasmallArchive {
        self.archive
    }
}

pub struct SingleBlobWriter<'a> {
    writer: &'a mut WasmallWriter,
}

impl SingleBlobWriter<'_> {
    pub fn push_reloc(&mut self, entry: RelocEntry, value_taken: u32) {
        // nop
    }

    pub fn finish<R>(&mut self, write_normalized: impl FnOnce(&mut Vec<u8>) -> R) -> R {
        write_normalized(&mut self.writer.archive.out_buf)
    }
}
