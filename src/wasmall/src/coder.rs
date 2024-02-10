use std::{collections::hash_map, ops::Range};

use anyhow::Context;
use blake3::{hash, Hash};
use rustc_hash::FxHashMap;

use crate::{
    reloc::{rewrite_relocated, RelocEntry, Rewriter},
    util::{
        len_of, ByteCursor, ByteParse, ByteParseList, Leb128WriteExt, LenCounter, SliceExt,
        VarByteVec, VarU32,
    },
};

// === Common === //

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

    pub fn push_blob(&mut self, relocations: &[(RelocEntry, u32)], data: &[u8]) {
        // Create the blob's data
        let blob_range = {
            let blob_start = self.buf.len();

            // Write relocations
            self.buf.write_var_u32(relocations.len() as u32);

            rewrite_relocated(
                data,
                &mut LenCounter::default(),
                self,
                relocations.iter().map(|(reloc, _)| {
                    (
                        reloc.offset as usize,
                        move |buf: &mut ByteCursor, writer: &mut LenCounter, cx: &mut Self| {
                            // Write relocation type.
                            cx.buf.push(reloc.ty.unwrap() as u8);

                            // Write relocation index
                            cx.buf.write_var_u32(writer.0 as u32);

                            // Write relocation offset
                            cx.buf.write_var_u32(reloc.offset);

                            // Write relocation addend
                            if let Some(addend) = reloc.addend {
                                cx.buf.write_var_i32(addend);
                            }

                            reloc
                                .ty
                                .unwrap()
                                .rewrite_kind()
                                .as_zeroed()
                                .rewrite(buf, writer, cx)
                                .unwrap();

                            Ok(())
                        },
                    )
                }),
            )
            .unwrap();

            // Push zeroed blob body data into blob buffer
            rewrite_relocated(
                data,
                &mut self.buf,
                &mut (),
                relocations.iter().map(|(reloc, _)| {
                    (
                        reloc.offset as usize,
                        reloc.ty.unwrap().rewrite_kind().as_zeroed(),
                    )
                }),
            )
            .unwrap();

            blob_start..self.buf.len()
        };

        // Write the concretes
        let concretes = {
            let concretes_start = self.buf.len();

            // Write count
            let byte_size = len_of(|c| {
                for (_, v) in relocations {
                    c.write_var_u32(*v);
                }
            });
            self.buf.write_var_u32(byte_size as u32);

            // Write values
            for (_, v) in relocations {
                self.buf.write_var_u32(*v);
            }

            concretes_start..self.buf.len()
        };

        self.segments.push(Segment::Blob {
            blob_range,
            concretes,
        });
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
                    let hash = hash(&self.buf[blob_range.clone()]);
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
    pub fn segments(&self) -> ByteParseList<'a, WasmallModSegment<'a>> {
        ByteParseList::new(ByteCursor(self.segments))
    }
}

#[derive(Debug, Clone)]
pub enum WasmallModSegment<'a> {
    Verbatim(ModuleSegmentVerbatim<'a>),
    Blob(ModuleSegmentBlob<'a>),
}

impl<'a> ByteParse<'a> for WasmallModSegment<'a> {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        Ok(
            match buf
                .lookahead_annotated("module kind", |c| SegmentKind::from_byte(c.read_u8()?))?
            {
                SegmentKind::Verbatim => Self::Verbatim(ModuleSegmentVerbatim::parse(buf)?),
                SegmentKind::Blob => Self::Blob(ModuleSegmentBlob::parse(buf)?),
            },
        )
    }
}

#[derive(Debug, Clone)]
pub struct ModuleSegmentVerbatim<'a> {
    data: &'a [u8],
}

impl<'a> ByteParse<'a> for ModuleSegmentVerbatim<'a> {
    type Out = Self;

    fn parse_naked(buf: &mut ByteCursor<'a>) -> anyhow::Result<Self::Out> {
        Ok(ModuleSegmentVerbatim {
            data: VarByteVec::parse(buf).context("failed to read verbatim segment data")?,
        })
    }
}

impl<'a> ModuleSegmentVerbatim<'a> {
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
}

#[derive(Debug, Clone)]
pub struct ModuleSegmentBlob<'a> {
    hash: &'a [u8],
    reloc_values: &'a [u8],
}

impl<'a> ByteParse<'a> for ModuleSegmentBlob<'a> {
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

impl<'a> ModuleSegmentBlob<'a> {
    pub fn hash(&self) -> Hash {
        Hash::from_bytes(self.hash.to_array())
    }

    pub fn reloc_values(&self) -> ByteParseList<'a, VarU32> {
        ByteParseList::new(ByteCursor(self.reloc_values))
    }
}
