use std::{collections::hash_map, ops::Range};

use blake3::{hash, Hash};
use rustc_hash::FxHashMap;

use crate::{
    reloc::{rewrite_relocated, RelocEntry, Rewriter},
    util::{Leb128WriteExt, LenCounter},
};

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
                        move |buf: &[u8], writer: &mut LenCounter, cx: &mut Self| {
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

                            Ok(reloc
                                .ty
                                .unwrap()
                                .rewrite_kind()
                                .as_zeroed()
                                .rewrite(buf, writer, cx)
                                .unwrap())
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
                    out_buf.write_var_u64(range.len() as u64);
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
