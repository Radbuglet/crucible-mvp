use std::{collections::hash_map, num::Wrapping, ops::Range};

use blake3::{hash, Hash};
use rustc_hash::FxHashMap;
use wasmparser::{
    BinaryReader, DefinedDataSymbol, FromReader, Linking, LinkingSectionReader, Parser, Payload,
    SymbolInfo,
};

use crate::{
    reloc::{rewrite_relocated, RelocEntry, RelocSection},
    util::VecExt,
};

pub fn split_module(src: &[u8]) -> anyhow::Result<()> {
    // Collect all payloads ahead of time so we don't have to deal with the somewhat arcane parser
    // API
    let mut payloads = Vec::new();

    for payload in Parser::new(0).parse_all(src) {
        payloads.push(payload?);
    }

    // Run a first pass to collect all relocations from the file as well as the locations of symbols
    // that we can turn into blobs.

    // Maps sections to a list of their relocations.
    let mut orig_reloc_map = <Vec<Vec<RelocEntry>>>::new();

    // Maps function indices to their symbols.
    let mut func_sym_map = FxHashMap::<u32, u32>::default();

    // Maps data segments to ranges associated with symbols.
    let mut data_seg_map = FxHashMap::<u32, Vec<(u32, Range<u32>)>>::default();

    {
        let mut section_index = Wrapping(usize::MAX);

        for payload in &payloads {
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
                                    func_sym_map.insert(fn_idx, sym_idx as u32);
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
                                    data_seg_map
                                        .entry(index)
                                        .or_default()
                                        .push((index, offset..(offset + size)));
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
                        let reloc = reloc?;
                        anyhow::ensure!(reloc.ty.is_some());
                        out_vec.push(reloc);
                    }
                }
                _ => {}
            }
        }

        for vec in &mut orig_reloc_map {
            vec.sort_unstable_by(|a, b| a.offset.cmp(&b.offset));
        }

        for ranges in data_seg_map.values_mut() {
            ranges.sort_unstable_by(|(_, a), (_, b)| a.start.cmp(&b.start))
        }
    }

    // Resolve blobs for each of the symbol-wrapped and hash them.

    // Stores relocation-erased blobs.
    let mut blob_buf = Vec::new();

    // Maps symbol indices to their corresponding blob.
    let mut symbol_blobs = FxHashMap::<u32, (Range<usize>, Hash)>::default();

    // Maps symbol indices to their values.
    let mut sym_values = FxHashMap::<u32, Option<u64>>::default();

    {
        let mut parser = payloads.iter().peekable();
        let mut section_idx = 0;
        let mut func_idx = 0;

        while let Some(payload) = parser.next() {
            match payload {
                Payload::CodeSectionStart { range, .. } => {
                    let section_start = range.start;

                    // Determine the set of relocations affecting this section
                    let mut relocations = orig_reloc_map[section_idx].iter().peekable();

                    // For each function...
                    while let Some(Payload::CodeSectionEntry(func)) = parser.peek() {
                        parser.next();

                        // Determine the range of this code entry relative to the section
                        let entry_start = (func.range().start - section_start) as u32;
                        let entry_end = (func.range().end - section_start) as u32;
                        let entry_data = &src[func.range()];

                        // Skip to the first relocation affecting this entry
                        while relocations
                            .peek()
                            .is_some_and(|reloc| reloc.offset < entry_start)
                        {
                            relocations.next();
                        }

                        // Rewrite the function with erased relocations
                        let blob_start = blob_buf.len();
                        rewrite_relocated(
                            entry_data,
                            &mut blob_buf,
                            relocations.clone().map(|reloc| {
                                (
                                    (reloc.offset - entry_start) as usize,
                                    reloc.ty.unwrap().rewrite_kind().as_zeroed(),
                                )
                            }),
                        )?;
                        let blob_range = blob_start..blob_buf.len();
                        let blob = &blob_buf[blob_range.clone()];

                        // Check whether there exists a value mapped to all relocations for a specific
                        // symbol index.
                        for reloc in relocations.clone() {
                            if reloc.offset >= entry_end {
                                break;
                            }

                            let reloc_value = reloc
                                .ty
                                .unwrap()
                                .rewrite_kind()
                                // N.B. this is panic-safe since the relocation rewrite pre-validates
                                // the relocation table.
                                .read(&entry_data[((reloc.offset - entry_start) as usize)..])
                                .unwrap();

                            let reloc_value = reloc_value.as_u64();

                            match sym_values.entry(reloc.index) {
                                hash_map::Entry::Occupied(entry) => {
                                    let entry = entry.into_mut();
                                    if let Some(inner) = *entry {
                                        if inner != reloc_value {
                                            println!(
                                                "Multiple different values assigned to symbol {}: {} (ty {:?})",
                                                reloc.index, inner, reloc.ty.unwrap(),
                                            );
                                            *entry = None;
                                        }
                                    }

                                    if entry.is_none() {
                                        println!(
                                            "Multiple different values assigned to symbol {}: {} (ty {:?})",
                                            reloc.index, reloc_value, reloc.ty.unwrap(),
                                        );
                                    }
                                }
                                hash_map::Entry::Vacant(entry) => {
                                    entry.insert(Some(reloc_value));
                                }
                            }
                        }

                        // Hash that blob
                        symbol_blobs.insert(func_idx, (blob_range, hash(blob)));

                        func_idx += 1;
                    }
                }
                Payload::DataCountSection { count, range } => {
                    // TODO
                }

                _ => {}
            }

            if payload.as_section().is_some() {
                section_idx += 1;
            }
        }
    }

    // dbg!(&symbol_blobs);

    Ok(())
}
