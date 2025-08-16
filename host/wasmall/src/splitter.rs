use std::collections::hash_map;

use anyhow::Context;
use rustc_hash::FxHashMap;
use wasmparser::{Parser, Payload};

use crate::{
    coder::{WasmallArchive, WasmallWriter},
    reloc::{RelocEntry, RelocSection},
    util::{ByteCursor, ByteParse, Leb128WriteExt, OffsetTracker, VecExt, len_of},
};

#[derive(Debug)]
pub struct SplitModuleArgs<'a> {
    pub src: &'a [u8],
    pub truncate_relocations: bool,
}

#[derive(Debug)]
pub struct SplitModuleResult {
    pub archive: WasmallArchive,
    pub bytes_truncated: usize,
}

pub fn split_module(args: SplitModuleArgs<'_>) -> anyhow::Result<SplitModuleResult> {
    let SplitModuleArgs {
        src,
        truncate_relocations,
    } = args;

    let _guard = OffsetTracker::new(src);

    // Collect all payloads ahead of time to avoid having to repeatedly check for errors.
    let payloads = {
        let mut payloads = Vec::new();

        for payload in Parser::new(0).parse_all(src) {
            payloads.push(payload?);
        }

        payloads
    };

    // Run a first pass to collect all relocations from the file. This also lets us entire that
    // relocations are even available to us in the first place.

    // Maps sections to a list of their relocations.
    let mut orig_reloc_map = <Vec<Vec<RelocEntry>>>::new();

    let mut has_linking_section = false;

    for payload in &payloads {
        match payload {
            Payload::CustomSection(payload) if payload.name() == "linking" => {
                // In order to distinguish object files from executable WebAssembly modules the
                // linker can check for the presence of the "linking" custom section which must
                // exist in all object files.
                has_linking_section = true;
            }
            Payload::CustomSection(payload) if payload.name().starts_with("reloc.") => {
                let relocs = RelocSection::parse(&mut ByteCursor(payload.data()))?;
                let out_vec = orig_reloc_map.ensure_index(relocs.target_section as usize);

                for reloc in relocs.entries() {
                    out_vec.push(reloc?);
                }
            }
            _ => {}
        }
    }

    if !has_linking_section {
        anyhow::bail!("WASM module lacks relocation information");
    }

    for vec in &mut orig_reloc_map {
        vec.sort_unstable_by(|a, b| a.offset.cmp(&b.offset));
    }

    // Run a second pass to create both the blobs and the split module.
    let mut writer = WasmallWriter::default();
    let mut bytes_truncated = 0;
    {
        // Write the file's sections
        let mut parser = payloads.iter().peekable();
        let mut section_idx = 0;

        while let Some(payload) = parser.next() {
            match payload {
                Payload::Version { range, .. } => {
                    writer.push_verbatim(|sink| {
                        sink.extend_from_slice(&src[range.clone()]);
                    });
                }
                Payload::CodeSectionStart { range, count, .. } => {
                    let section_start = range.start;

                    assert_eq!(
                        ByteCursor(&src[section_start..]).read_var_u32().unwrap(),
                        *count
                    );

                    // Write section header verbatim
                    writer.push_verbatim::<anyhow::Result<_>>(|sink| {
                        // Write section ID
                        sink.push(10);

                        // Write section length. Note that the `range` already contains the `count`
                        // field.
                        sink.write_var_u32(
                            u32::try_from(range.len()).context("code section is too big")?,
                        );

                        // Write the count field
                        sink.write_var_u32(*count);

                        Ok(())
                    })?;

                    // Determine the set of relocations affecting this section
                    let relocations = &orig_reloc_map[section_idx];
                    let mut relocations_idx = 0;

                    // For each function...
                    while let Some(Payload::CodeSectionEntry(func)) = parser.peek() {
                        parser.next();

                        // Extend the range byte view to include the size field in the function
                        let func_range = func.range();
                        let size_field_byte_count =
                            len_of(|w| w.write_var_u32(func_range.len() as u32));

                        let func_range = (func_range.start - size_field_byte_count)..func_range.end;

                        // Determine the range of this code entry relative to the section start
                        let entry_start = (func_range.start - section_start) as u32;
                        let entry_end = (func_range.end - section_start) as u32;
                        let entry_data = &src[func_range];

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

                        // Transform the blob's globally-indexed relocations into locally-indexed
                        // relocations for the `WasmallWriter`.
                        let mut local_relocations = Vec::new();
                        let mut local_relocation_values = Vec::new();
                        {
                            // Map global symbol indexes to their blob-local index. Note that a global
                            // symbol may be assigned to multiple different blob-local indices over
                            // the course of a blob because, sometimes, the relocation system lies.
                            let mut global_to_local_sym_map = <FxHashMap<u32, usize>>::default();

                            for reloc in relocations {
                                // Determine the value this relocation takes on.
                                let reloc_ty = reloc.ty;
                                let reloc_value = reloc_ty
                                    .rewrite_kind()
                                    .read(&mut ByteCursor(
                                        &entry_data[(reloc.offset - entry_start) as usize..],
                                    ))?
                                    // Undo the addend.
                                    .as_u32_neg_offset(reloc.addend.unwrap_or(0));

                                // Determine whether we can use the old local symbol, generating a
                                // new local symbol if not.
                                let local_sym = match global_to_local_sym_map.entry(reloc.index) {
                                    hash_map::Entry::Occupied(entry) => {
                                        let entry = entry.into_mut();
                                        let entry_value = local_relocation_values[*entry];

                                        if reloc_value != entry_value {
                                            *entry = local_relocation_values.len();
                                            local_relocation_values.push(reloc_value);
                                        }

                                        *entry
                                    }
                                    hash_map::Entry::Vacant(entry) => {
                                        let entry_idx = local_relocation_values.len();
                                        entry.insert(entry_idx);
                                        local_relocation_values.push(reloc_value);
                                        entry_idx
                                    }
                                };

                                let local_sym = u32::try_from(local_sym).unwrap();

                                local_relocations.push(RelocEntry {
                                    offset: reloc.offset - entry_start,
                                    index: local_sym,
                                    ..*reloc
                                });
                            }
                        }

                        // Complete the blob
                        writer.push_blob(&local_relocations, &local_relocation_values, entry_data);
                    }
                }
                payload => {
                    let Some((section_id, section_range)) = payload.as_section() else {
                        match payload {
                            // Processed by another case.
                            Payload::Version { .. } => unreachable!(),

                            // Processed by another case.
                            Payload::CodeSectionEntry(..) => unreachable!(),

                            // Nothing to do.
                            Payload::End(_) => {}

                            // These are the only virtual payloads the parser can emit.
                            _ => unreachable!(),
                        }

                        continue;
                    };

                    // Truncate out relocations because they're no longer needed.
                    if let Payload::CustomSection(cs) = payload
                        && (cs.name() == "linking" || cs.name() == "reloc.")
                        && truncate_relocations
                    {
                        bytes_truncated += section_range.len();
                        continue;
                    }

                    writer.push_verbatim::<anyhow::Result<_>>(|sink| {
                        // Write section ID
                        sink.push(section_id);

                        // Write section length
                        sink.write_var_u32(
                            u32::try_from(section_range.len()).context("section is too big")?,
                        );

                        // Write section data
                        sink.extend_from_slice(&src[section_range]);

                        Ok(())
                    })?;
                }
            }

            if payload.as_section().is_some() {
                section_idx += 1;
            }
        }
    }

    Ok(SplitModuleResult {
        archive: writer.finish(),
        bytes_truncated,
    })
}
