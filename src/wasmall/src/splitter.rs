use std::{collections::hash_map, num::Wrapping, ops::Range};

use rustc_hash::FxHashMap;
use wasmparser::{
    BinaryReader, DefinedDataSymbol, FromReader, Linking, LinkingSectionReader, Parser, Payload,
    SymbolInfo,
};

use crate::{
    coder::{WasmallArchive, WasmallWriter},
    reloc::{rewrite_relocated, RelocEntry, RelocSection},
    util::VecExt,
};

pub fn split_module(src: &[u8]) -> anyhow::Result<WasmallArchive> {
    // Collect all payloads ahead of time so we don't have to deal with the somewhat arcane parser API.
    let payloads = {
        let mut payloads = Vec::new();
        for payload in Parser::new(0).parse_all(src) {
            payloads.push(payload?);
        }
        payloads
    };

    // Run a first pass to collect all relocations from the file as well as the locations of data
    // ranges we can turn into blobs.

    // Maps sections to a list of their relocations.
    let mut orig_reloc_map = <Vec<Vec<RelocEntry>>>::new();

    // Maps data segments to a list of ranges associated with symbols.
    let mut data_seg_map = FxHashMap::<u32, Vec<(usize, Range<u32>)>>::default();

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

                            if let SymbolInfo::Data {
                                symbol:
                                    Some(DefinedDataSymbol {
                                        offset,
                                        size,
                                        index,
                                    }),
                                ..
                            } = info
                            {
                                data_seg_map
                                    .entry(index)
                                    .or_default()
                                    .push((sym_idx, offset..(offset + size)));
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

    // Run a second pass to create both the blobs and the split module.
    let mut writer = WasmallWriter::default();
    {
        let mut parser = payloads.iter().peekable();
        let mut section_idx = 0;

        while let Some(payload) = parser.next() {
            match payload {
                Payload::CodeSectionStart { range, .. } => {
                    let section_start = range.start;

                    // Write section header verbatim
                    // TODO

                    // Determine the set of relocations affecting this section
                    let relocations = &orig_reloc_map[section_idx];
                    let mut relocations_idx = 0;

                    // For each function...
                    while let Some(Payload::CodeSectionEntry(func)) = parser.peek() {
                        parser.next();

                        let mut blob = writer.push_blob();

                        // Determine the range of this code entry relative to the section
                        let entry_start = (func.range().start - section_start) as u32;
                        let entry_end = (func.range().end - section_start) as u32;
                        let entry_data = &src[func.range()];

                        // Collect the set of relocations affecting this function
                        let relocations = {
                            while relocations
                                .get(relocations_idx)
                                .is_some_and(|reloc| reloc.offset < entry_start)
                            {
                                relocations_idx += 1;
                            }

                            let start = relocations_idx;

                            while relocations
                                .get(relocations_idx)
                                .is_some_and(|reloc| reloc.offset <= entry_end)
                            {
                                relocations_idx += 1;
                            }

                            &relocations[start..relocations_idx]
                        };

                        // Push the blob's relocations
                        let mut global_to_local_sym_and_value_map =
                            <FxHashMap<u32, (u32, u64)>>::default();

                        let mut local_sym_gen = 0;

                        for reloc in relocations {
                            // Determine the value this relocation takes on.
                            let reloc_ty = reloc.ty.unwrap();
                            let reloc_value = reloc_ty
                                .rewrite_kind()
                                .read(&entry_data[(reloc.offset - entry_start) as usize..])?
                                .as_u64()
                                .wrapping_add_signed(-(reloc.addend.unwrap_or(0) as i64));

                            // Determine whether we can use the old local symbol, generating a new
                            // local symbol if not.
                            let local_sym =
                                match global_to_local_sym_and_value_map.entry(reloc.index) {
                                    hash_map::Entry::Occupied(entry) => {
                                        let entry = entry.into_mut();

                                        if reloc_value != entry.1 {
                                            *entry = (local_sym_gen, reloc_value);
                                            local_sym_gen += 1;
                                        }

                                        entry.0
                                    }
                                    hash_map::Entry::Vacant(entry) => {
                                        let local_sym = local_sym_gen;
                                        local_sym_gen += 1;
                                        entry.insert((local_sym, reloc_value));
                                        local_sym
                                    }
                                };

                            blob.push_reloc(
                                RelocEntry {
                                    offset: reloc.offset - entry_start,
                                    index: local_sym,
                                    ..*reloc
                                },
                                reloc_value,
                            );
                            relocations_idx += 1;
                        }

                        // Complete the blob
                        blob.finish(|buf| {
                            rewrite_relocated(
                                entry_data,
                                buf,
                                relocations.iter().map(|reloc| {
                                    (
                                        (reloc.offset - entry_start) as usize,
                                        reloc.ty.unwrap().rewrite_kind().as_zeroed(),
                                    )
                                }),
                            )
                        })?;
                    }
                }
                Payload::DataCountSection { count, range } => {
                    // TODO
                }

                payload => {
                    if let Some((section_id, section_range)) = payload.as_section() {
                        // TODO: Write section verbatim with filtering
                    }
                }
            }

            if payload.as_section().is_some() {
                section_idx += 1;
            }
        }
    }

    Ok(writer.finish())
}
