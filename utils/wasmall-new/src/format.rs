use std::{collections::hash_map, ops::Range};

use anyhow::Context as _;
use blake3::Hash;
use rustc_hash::FxHashMap;

use crate::utils::{
    BufWriter, ByteCursor, ByteParse, ByteParseList, Leb128WriteExt as _, SliceExt as _,
    VarByteVec, VarU32,
};

// === Common === //

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
    Fixed32,
    Fixed64,
    Var32,
    Var64,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum SegmentKind {
    Verbatim = 0,
    Blob = 1,
}

impl SegmentKind {
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
    pub out_buf: Vec<u8>,
    pub blob_buf: Vec<u8>,
    pub hashes: FxHashMap<Hash, Range<usize>>,
}

#[derive(Debug, Default)]
pub struct WasmallWriter {
    /// A general purpose buffer storing the data for all fragments needed to assemble the final
    /// module.
    buf: Vec<u8>,

    /// A vector of all segments to be written.
    segments: Vec<Segment>,
}

#[derive(Debug)]
enum Segment {
    Verbatim(Range<usize>),
    Blob {
        /// The byte range in the `blob_buf` of the blob's data.
        blob_range: Range<usize>,

        /// The byte range in the `main_buf` corresponding to the blob expansion's compressed parameters.
        concretes: Range<usize>,
    },
}

impl WasmallWriter {
    pub fn push_verbatim<R>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> R) -> R {
        let start = self.buf.len();
        let res = f(&mut self.buf);

        match self.segments.last_mut() {
            Some(Segment::Verbatim(verbatim)) => {
                verbatim.end = self.buf.len();
            }
            _ => {
                self.segments.push(Segment::Verbatim(start..self.buf.len()));
            }
        }

        res
    }

    pub fn push_blob(&mut self, symbols: &[u64], relocations: &[LocalReloc], erased_data: &[u8]) {
        todo!()
    }

    pub fn finish(self) -> WasmallArchive {
        let mut out_buf = Vec::new();
        let mut blob_buf = Vec::new();
        let mut hashes = FxHashMap::default();

        for segment in self.segments {
            match segment {
                Segment::Verbatim(range) => {
                    out_buf.push(0);
                    out_buf.write_var_u32(u32::try_from(range.len()).unwrap());
                    out_buf.extend_from_slice(&self.buf[range]);
                }
                Segment::Blob {
                    blob_range,
                    concretes,
                } => {
                    out_buf.push(1);
                    let hash = blake3::hash(&self.buf[blob_range.clone()]);
                    out_buf.extend_from_slice(hash.as_bytes());
                    out_buf.extend_from_slice(&self.buf[concretes]);

                    if let hash_map::Entry::Vacant(entry) = hashes.entry(hash) {
                        let start = blob_buf.len();
                        blob_buf.extend_from_slice(&self.buf[blob_range]);
                        entry.insert(start..blob_buf.len());
                    }
                }
            }
        }

        WasmallArchive {
            out_buf,
            blob_buf,
            hashes,
        }
    }
}

// === Reader === //

// Module
#[derive(Debug, Clone)]
pub struct WasmallMod<'a> {
    segments: &'a [u8],
}

impl<'a> ByteParse<'a> for WasmallMod<'a> {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        Ok(Self { segments: buf.0 })
    }
}

impl<'a> WasmallMod<'a> {
    pub fn segments(&self) -> ByteParseList<'a, WasmallModSeg<'a>> {
        ByteParseList::new(ByteCursor(self.segments))
    }
}

#[derive(Debug, Clone)]
pub enum WasmallModSeg<'a> {
    Verbatim(WasmallModSegVerbatim<'a>),
    Blob(WasmallModSegBlob<'a>),
}

impl<'a> ByteParse<'a> for WasmallModSeg<'a> {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        Ok(
            match buf
                .lookahead_annotated("module kind", |c| SegmentKind::from_byte(c.read_u8()?))?
            {
                SegmentKind::Verbatim => Self::Verbatim(WasmallModSegVerbatim::parse(buf)?),
                SegmentKind::Blob => Self::Blob(WasmallModSegBlob::parse(buf)?),
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
