use std::{collections::hash_map, ops::Range};

use anyhow::Context as _;
use blake3::Hash;
use rustc_hash::FxHashMap;

use crate::utils::{
    BufRewriter, BufWriter, ByteCursor, ByteParse, ByteParseList, LookBackBufWriter, VarI64,
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
        out.write_var_u32(self.index as u32);
        out.write_var_u32(self.offset as u32);
        out.write_u8(self.category as u8 | ((self.addend.is_some() as u8) << 7));

        if let Some(addend) = self.addend {
            out.write_var_i64(addend);
        }
    }
}

impl ByteParse<'_> for LocalReloc {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'_>) -> anyhow::Result<Self::Out> {
        let index = buf.read_var_u32().context("failed to read index")?;
        let offset = buf.read_var_u32().context("failed to read offset")?;
        let category_and_has_addend = buf.read_u8().context("failed to read category")?;
        let category = category_and_has_addend & !(1 << 7);
        let category = RelocCategory::from_byte(category)?;
        let has_addend = category_and_has_addend & (1 << 7) != 0;
        let addend = if has_addend {
            Some(buf.read_var_i64()?)
        } else {
            None
        };

        Ok(Self {
            index: index as usize,
            offset: offset as usize,
            category,
            addend,
        })
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

    pub fn length(self) -> usize {
        match self {
            RelocCategory::Fixed32 => 4,
            RelocCategory::Fixed64 => 8,
            RelocCategory::Var32 => 5,
            RelocCategory::Var64 => 10,
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
        // Check magic.
        anyhow::ensure!(
            buf.read_u64().context("failed to read magic number")? == INDEX_MAGIC_NUMBER,
            "invalid magic number"
        );

        // Check version.
        let ver = buf.read_var_u32()?;
        anyhow::ensure!(ver == INDEX_VERSION_NUMBER, "mismatched version number");

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
        buf.lookahead_annotated("chunk", |c| {
            let kind = ChunkKind::from_byte(c.read_u8()?)?;
            let len = c.read_u32()?;
            let data = c.consume(len as usize)?;

            Ok(match kind {
                ChunkKind::Verbatim => {
                    Self::Verbatim(WasmallModSegVerbatim::parse(&mut ByteCursor(data))?)
                }
                ChunkKind::Blob => Self::Blob(WasmallModSegBlob::parse(&mut ByteCursor(data))?),
            })
        })
    }
}

#[derive(Debug, Clone)]
pub struct WasmallModSegVerbatim<'a> {
    data: &'a [u8],
}

impl<'a> ByteParse<'a> for WasmallModSegVerbatim<'a> {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        Ok(WasmallModSegVerbatim { data: buf.0 })
    }
}

impl<'a> WasmallModSegVerbatim<'a> {
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
}

#[derive(Debug, Clone)]
pub struct WasmallModSegBlob<'a> {
    hash: blake3::Hash,
    symbols: &'a [u8],
}

impl<'a> ByteParse<'a> for WasmallModSegBlob<'a> {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        let hash = blake3::Hash::from_bytes(buf.consume_arr()?);

        Ok(Self {
            hash,
            symbols: buf.0,
        })
    }
}

impl<'a> WasmallModSegBlob<'a> {
    pub fn hash(&self) -> Hash {
        self.hash
    }

    pub fn symbols(&self) -> ByteParseList<'a, VarI64> {
        ByteParseList::new(ByteCursor(self.symbols))
    }

    pub fn write(&self, blob: &WasmallBlob<'_>, out: &mut Vec<u8>) -> anyhow::Result<()> {
        // Collect symbols
        let mut symbols = Vec::new();

        for symbol in self.symbols() {
            symbols.push(symbol?);
        }

        // Copy out erased version of blob.
        let start = out.len();
        out.extend_from_slice(blob.data());

        let out_range = &mut out[start..];

        // Apply relocations.
        for reloc in blob.relocations() {
            let reloc = reloc?;

            let out_range = out_range
                .get_mut(reloc.offset..(reloc.offset + reloc.category.length()))
                .context("relocation range is not within blob")?;

            let mut out_range = BufRewriter(out_range);

            let value = *symbols
                .get(reloc.index)
                .context("relocation symbol not given value")?;

            let addend = reloc.addend.unwrap_or(0);

            match reloc.category {
                RelocCategory::Fixed32 => {
                    out_range.write_u32((value as u32).wrapping_add(addend as u32));
                }
                RelocCategory::Fixed64 => {
                    out_range.write_u64((value as u64).wrapping_add(addend as u64));
                }
                RelocCategory::Var32 => {
                    out_range.write_var_u32_full((value as u32).wrapping_add(addend as u32));
                }
                RelocCategory::Var64 => {
                    out_range.write_var_u64_full((value as u64).wrapping_add(addend as u64));
                }
            }
        }

        Ok(())
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
        anyhow::ensure!(buf.read_u64()? == BLOB_MAGIC_NUMBER);
        anyhow::ensure!(buf.read_var_u32()? == BLOB_VERSION_NUMBER);

        let relocations = buf.read_u32()?;
        let relocations = buf.consume(relocations as usize)?;
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
