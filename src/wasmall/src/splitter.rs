use std::{num::Wrapping, ops::Range};

use wasmparser::{
    BinaryReader, DefinedDataSymbol, FromReader, Linking, LinkingSectionReader, Parser, Payload,
    SymbolInfo,
};

use crate::{reloc::RelocSection, util::VecExt};

pub fn split_module(src: &[u8]) -> anyhow::Result<()> {
    #[derive(Debug, Default)]
    enum SymBlob {
        #[default]
        None,
        Func(u32),
        Data(u32, Range<u32>),
    }

    // Run a first pass to collect all relocations from the file as well as the locations of symbols
    // that we can turn into blobs.
    let mut orig_reloc_map = <Vec<Vec<_>>>::new();
    let mut sym_to_blob_map = <Vec<SymBlob>>::new();
    let mut fn_blob_to_sym = <Vec<Option<usize>>>::new();
    {
        let mut section_index = Wrapping(usize::MAX);

        for payload in Parser::new(0).parse_all(src) {
            let payload = payload?;

            if payload.as_section().is_some() {
                section_index += 1;
            }

            match payload {
                Payload::CustomSection(payload) if payload.name() == "linking" => {
                    let reader = LinkingSectionReader::new(payload.data(), payload.data_offset())?;

                    for subsection in reader.subsections() {
                        let subsection = subsection?;

                        let Linking::SymbolTable(stab) = subsection else {
                            continue;
                        };

                        for (sym_idx, info) in stab.into_iter().enumerate() {
                            let info = info?;

                            match info {
                                SymbolInfo::Func { index: fn_idx, .. } => {
                                    *sym_to_blob_map.ensure_index(sym_idx) = SymBlob::Func(fn_idx);
                                    *fn_blob_to_sym.ensure_index(fn_idx as usize) = Some(sym_idx);
                                }
                                SymbolInfo::Data {
                                    symbol:
                                        Some(DefinedDataSymbol {
                                            offset,
                                            size,
                                            index,
                                        }),
                                    ..
                                } => {
                                    *sym_to_blob_map.ensure_index(sym_idx) =
                                        SymBlob::Data(index, offset..(offset + size));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Payload::CustomSection(payload) if payload.name().starts_with("reloc.") => {
                    let relocs = RelocSection::from_reader(&mut BinaryReader::new_with_offset(
                        payload.data(),
                        payload.data_offset(),
                    ))?;
                    let out_vec = orig_reloc_map.ensure_index(relocs.target_section as usize);

                    for reloc in relocs.entries {
                        out_vec.push(reloc?);
                    }
                }
                _ => {}
            }
        }
    }

    for vec in &mut orig_reloc_map {
        vec.sort_unstable_by(|a, b| a.offset.cmp(&b.offset));
    }

    // Compress the entire module using a meta-format which resolves to relocation-sensitive expansions
    // of blobs.
    // TODO

    Ok(())
}
