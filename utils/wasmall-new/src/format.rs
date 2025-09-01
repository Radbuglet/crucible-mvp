use std::{collections::hash_map, ops::Range};

use anyhow::Context as _;
use blake3::Hash;
use rustc_hash::FxHashMap;

use crate::utils::{
    BufWriter, ByteCursor, ByteParse, ByteParseList, LookBackBufWriter, SliceExt as _, VarByteVec,
    VarU32,
};

// === Common === //

pub const INDEX_MAGIC_NUMBER: u64 = u64::from_le_bytes(*b"CruWsIdx");
pub const INDEX_VERSION_NUMBER: u32 = 1;

pub const BLOB_MAGIC_NUMBER: u64 = u64::from_le_bytes(*b"CruWsBlb");
pub const BLOB_VERSION_NUMBER: u32 = 1;

#[derive(Debug)]
pub struct LocalReloc {
    pub index: usize,
    pub offset: usize,
    pub category: RelocCategory,
    pub addend: Option<i64>,
}

impl LocalReloc {
    pub fn write(&self, out: &mut impl BufWriter) {
        todo!()
    }
}

impl ByteParse<'_> for LocalReloc {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'_>) -> anyhow::Result<Self::Out> {
        todo!()
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum RelocCategory {
    Fixed32 = 0,
    Fixed64 = 1,
    Var32 = 2,
    Var64 = 3,
}

impl RelocCategory {
    pub fn from_byte(v: u8) -> anyhow::Result<Self> {
        match v {
            0 => Ok(Self::Fixed32),
            1 => Ok(Self::Fixed64),
            2 => Ok(Self::Var32),
            3 => Ok(Self::Var64),
            _ => Err(anyhow::anyhow!("unknown segment kind {v}")),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ChunkKind {
    Verbatim = 0,
    Blob = 1,
}

impl ChunkKind {
    pub fn from_byte(v: u8) -> anyhow::Result<Self> {
        match v {
            0 => Ok(Self::Verbatim),
            1 => Ok(Self::Blob),
            _ => Err(anyhow::anyhow!("unknown segment kind {v}")),
        }
    }
}

// === Writer === //

#[derive(Debug)]
pub struct WasmallArchive {
    pub index_buf: Vec<u8>,
    pub blob_buf: Vec<u8>,
    pub blobs: FxHashMap<Hash, Range<usize>>,
}

impl Default for WasmallArchive {
    fn default() -> Self {
        let mut index_buf = Vec::new();

        index_buf.write_u64(INDEX_MAGIC_NUMBER);
        index_buf.write_var_u32(INDEX_VERSION_NUMBER);

        Self {
            index_buf,
            blob_buf: Vec::new(),
            blobs: FxHashMap::default(),
        }
    }
}

impl WasmallArchive {
    pub fn push_verbatim<R>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> R) -> R {
        // Write the chunk kind.
        self.index_buf.write_u8(ChunkKind::Verbatim as u8);

        // Write out the chunk.
        self.index_buf.write_sectioned(|writer| f(writer))
    }

    pub fn push_blob(&mut self, symbols: &[u64], relocations: &[LocalReloc], erased_data: &[u8]) {
        // Write the blob.
        let blob_hash = {
            let blob_start = self.blob_buf.len();
            {
                // Write out the magic and version number.
                self.blob_buf.write_u64(BLOB_MAGIC_NUMBER);
                self.blob_buf.write_var_u32(BLOB_VERSION_NUMBER);

                // Write out the `relocations`.
                self.blob_buf.write_sectioned(|writer| {
                    for relocation in relocations {
                        relocation.write(writer);
                    }
                });

                // Write out the erased data.
                self.blob_buf.write_bytes(erased_data);
            }

            // Hash and deduplicate the blob.
            let hash = blake3::hash(&self.blob_buf[blob_start..]);

            match self.blobs.entry(hash) {
                hash_map::Entry::Vacant(entry) => {
                    entry.insert(blob_start..self.blob_buf.len());
                }
                hash_map::Entry::Occupied(_) => {
                    self.blob_buf.truncate(blob_start);
                }
            }

            hash
        };

        // Write the index.
        {
            // Write the chunk kind.
            self.index_buf.write_u8(ChunkKind::Blob as u8);

            self.index_buf.write_sectioned(|writer| {
                // Write the hash.
                writer.write_bytes(blob_hash.as_bytes());

                // Write the symbols.
                for &symbol in symbols {
                    writer.write_var_u64(symbol);
                }
            });
        }
    }
}

// === Reader === //

// Index
#[derive(Debug, Clone)]
pub struct WasmallIndex<'a> {
    chunks: &'a [u8],
}

impl<'a> ByteParse<'a> for WasmallIndex<'a> {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        Ok(Self { chunks: buf.0 })
    }
}

impl<'a> WasmallIndex<'a> {
    pub fn chunks(&self) -> ByteParseList<'a, WasmallModChunk<'a>> {
        ByteParseList::new(ByteCursor(self.chunks))
    }
}

#[derive(Debug, Clone)]
pub enum WasmallModChunk<'a> {
    Verbatim(WasmallModSegVerbatim<'a>),
    Blob(WasmallModSegBlob<'a>),
}

impl<'a> ByteParse<'a> for WasmallModChunk<'a> {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        Ok(
            match buf.lookahead_annotated("module kind", |c| ChunkKind::from_byte(c.read_u8()?))? {
                ChunkKind::Verbatim => Self::Verbatim(WasmallModSegVerbatim::parse(buf)?),
                ChunkKind::Blob => Self::Blob(WasmallModSegBlob::parse(buf)?),
            },
        )
    }
}

#[derive(Debug, Clone)]
pub struct WasmallModSegVerbatim<'a> {
    data: &'a [u8],
}

impl<'a> ByteParse<'a> for WasmallModSegVerbatim<'a> {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        Ok(WasmallModSegVerbatim {
            data: VarByteVec::parse(buf).context("failed to read verbatim segment data")?,
        })
    }
}

impl<'a> WasmallModSegVerbatim<'a> {
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
}

#[derive(Debug, Clone)]
pub struct WasmallModSegBlob<'a> {
    hash: &'a [u8],
    reloc_values: &'a [u8],
}

impl<'a> ByteParse<'a> for WasmallModSegBlob<'a> {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        let hash = buf
            .consume(blake3::OUT_LEN)
            .context("failed to read blob hash")?;

        let reloc_values =
            VarByteVec::parse(buf).context("failed to read blob relocation values")?;

        Ok(Self { hash, reloc_values })
    }
}

impl<'a> WasmallModSegBlob<'a> {
    pub fn hash(&self) -> Hash {
        Hash::from_bytes(self.hash.to_array())
    }

    pub fn reloc_values(&self) -> ByteParseList<'a, VarU32> {
        ByteParseList::new(ByteCursor(self.reloc_values))
    }

    pub fn write(&self, blob: &WasmallBlob<'_>, out: &mut Vec<u8>) -> anyhow::Result<()> {
        todo!()
    }
}

// Blob
#[derive(Debug, Clone)]
pub struct WasmallBlob<'a> {
    relocations: &'a [u8],
    data: &'a [u8],
}

impl<'a> ByteParse<'a> for WasmallBlob<'a> {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        let relocation_count = buf
            .read_var_u32()
            .context("failed to read relocation count")?;

        let relocations = buf.lookahead_annotated("relocation list", |c| {
            c.get_slice_read(|c| {
                for _ in 0..relocation_count {
                    LocalReloc::parse(c)?;
                }

                Ok(())
            })
            .map(|(_, r)| r)
        })?;

        let data = buf.0;

        Ok(Self { relocations, data })
    }
}

impl<'a> WasmallBlob<'a> {
    pub fn relocations(&self) -> ByteParseList<'a, LocalReloc> {
        ByteParseList::new(ByteCursor(self.relocations))
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }
}
