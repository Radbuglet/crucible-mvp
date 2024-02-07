use blake3::Hash;
use rustc_hash::FxHashMap;

use crate::reloc::ScalarRewrite;

// === Writer === //

#[derive(Debug, Default)]
pub struct SplitEntryWriter {
    used_hashes: FxHashMap<Hash, u32>,
    buf: Vec<u8>,
}

impl SplitEntryWriter {
    /// Defines a relocation associated to a hash. If the hash was already used in this stream, it
    /// will be replaced by the index of that hash.
    pub fn push_reloc_def(&mut self, hash: Hash, value: ScalarRewrite, only_next_import: bool) {}

    /// Imports a blob by a given hash and updates it with the specified relocation definitions. If
    /// the hash was already used in this stream, it will be replaced by the index of that hash.
    pub fn push_blob_import(&mut self, hash: Hash) {}

    /// Pushes a verbatim section to stream.
    pub fn push_verbatim(&mut self, data: &[u8]) {}
}

// === Reader === //

// TODO
