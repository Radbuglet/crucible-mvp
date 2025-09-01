use std::{collections::hash_map, mem, ops::Range};

use anyhow::Context;
use push_fastcdc::{GearConfig, GearState, GearTablesRef};
use rustc_hash::FxHashMap;
use wasmparser::{
    BinaryReader, Parser, Payload, RelocAddendKind, RelocSectionReader, RelocationEntry,
    RelocationType,
};

use crate::{
    format::{LocalReloc, RelocCategory, WasmallArchive, WasmallWriter},
    utils::{ByteCursor, Leb128WriteExt as _, VecExt as _},
};

// === Driver === //

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

pub fn split_module(args: SplitModuleArgs) -> anyhow::Result<SplitModuleResult> {
    let SplitModuleArgs {
        src,
        truncate_relocations,
    } = args;

    // Collect all payloads ahead of time to avoid having to repeatedly check for errors.
    let payloads = {
        let mut payloads = Vec::new();

        for payload in Parser::new(0).parse_all(src) {
            payloads.push(payload?);
        }

        payloads
    };

    // Maps sections to a list of their relocations.
    let mut reloc_map = <Vec<Vec<RelocationEntry>>>::new();

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
                let relocs = RelocSectionReader::new(BinaryReader::new(
                    payload.data(),
                    payload.data_offset(),
                ))?;
                let out_vec = reloc_map.ensure_index(relocs.section_index() as usize);

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

    for vec in &mut reloc_map {
        vec.sort_unstable_by(|a, b| a.offset.cmp(&b.offset));
    }

    // Produce archive.
    let mut parser = payloads.iter().peekable();
    let mut section_idx = 0;
    let mut bytes_truncated = 0;
    let mut writer = WasmallWriter::default();

    while let Some(payload) = parser.next() {
        match payload {
            Payload::Version { range, .. } => {
                writer.push_verbatim(|sink| {
                    sink.extend_from_slice(&src[range.clone()]);
                });
            }
            Payload::CodeSectionEntry(..) => {
                // TODO
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

                // Write the section header.
                writer.push_verbatim::<anyhow::Result<_>>(|sink| {
                    // Write section ID
                    sink.push(section_id);

                    // Write section length
                    sink.write_var_u32(
                        u32::try_from(section_range.len()).context("section is too big")?,
                    );

                    Ok(())
                })?;

                // Erase the section's relocation data.
                let unerased = &src[section_range.clone()];
                let erased = zeroize_relocations(unerased, &reloc_map[section_idx])?;

                let cut_ends = split_hinted_cdc(
                    &erased,
                    reloc_map[section_idx].iter().map(|v| v.relocation_range()),
                );

                let mut cut_start = 0;
                let mut relocations = &reloc_map[section_idx][..];
                let mut chunks = Vec::new();

                for cut_end in cut_ends {
                    let cut_start = mem::replace(&mut cut_start, cut_end);

                    chunks.push(compute_local_relocations(
                        &unerased[cut_start..cut_end],
                        cut_start,
                        &mut relocations,
                    ));
                }
            }
        }

        if payload.as_section().is_some() {
            section_idx += 1;
        }
    }

    Ok(SplitModuleResult {
        archive: writer.finish(),
        bytes_truncated,
    })
}

// === Utils === //

pub fn zeroize_relocations(
    section: &[u8],
    relocations: &[RelocationEntry],
) -> anyhow::Result<Vec<u8>> {
    let mut zeroized = section.to_vec();

    for &rela in relocations {
        let value = zeroized
            .get_mut(rela.relocation_range())
            .context("relocation extends beyond bounds of section")?;

        for v in value {
            *v = 0;
        }
    }

    Ok(zeroized)
}

pub fn split_hinted_cdc(
    mut data: &[u8],
    atomic_regions: impl IntoIterator<Item = Range<usize>>,
) -> Vec<usize> {
    let mut cut_ends = Vec::new();
    let mut cdc = GearState::new();
    let mut atomic_regions = atomic_regions.into_iter().peekable();

    let data_orig_len = data.len();

    loop {
        // Determine length of next chunk.
        let (_, cut) = cdc.push(&GearConfig::STANDARD, GearTablesRef::new(), data);

        let should_break = cut.is_none();
        let mut cut_len = cut.map_or(data.len(), |v| v.len);

        // If the cut lands within an atomic region, extend it to end at the atomic end-point.
        {
            let data_start = data_orig_len - data.len();
            let orig_abs_cut_pos = data_start + cut_len;

            while let Some(atomic_region) = atomic_regions.peek() {
                let atomic_region = atomic_region.clone();

                if atomic_region.start > orig_abs_cut_pos {
                    // Cannot overlap with us.
                    break;
                }

                // This region either contains us, in which case we should adjust our bounds and
                // consume it, or it doesn't contain us, in which case it certainly won't contain
                // stuff after us.
                atomic_regions.next();

                if atomic_region.contains(&orig_abs_cut_pos) {
                    cut_len = atomic_region.end - (data_orig_len - data.len());
                    break;
                }
            }
        }

        // Mark new cut end and advance cursor.
        data = &data[cut_len..];
        cut_ends.push(cut_ends.last().copied().unwrap_or(0) + cut_len);

        if should_break {
            break;
        }
    }

    cut_ends
}

pub fn compute_local_relocations(
    data: &[u8],
    offset: usize,
    relocations: &mut &[RelocationEntry],
) -> anyhow::Result<(Vec<u64>, Vec<LocalReloc>)> {
    // Determine the set of relocations affecting this slice.
    let relocations = {
        let mut relocations_tmp = mem::take(relocations);

        while relocations_tmp
            .first()
            .is_some_and(|v| (v.offset as usize) < offset)
        {
            relocations_tmp = &relocations_tmp[1..];
        }

        let index = relocations_tmp
            .iter()
            .position(|v| v.offset as usize >= offset + data.len())
            .unwrap_or(0);

        let (own_relocations, relocations_tmp) = relocations_tmp.split_at(index);
        *relocations = relocations_tmp;

        own_relocations
    };

    // Rewrite relocations local to the chunk.
    let mut global_to_local = FxHashMap::<(RelocationType, u32), usize>::default();
    let mut local_symbols = Vec::new();
    let mut local_relocs = Vec::new();

    for &(mut relocation) in relocations {
        relocation.offset -= offset as u32;

        let category = {
            use RelocationType::*;

            match relocation.ty {
                FunctionIndexLeb | TableIndexSleb | MemoryAddrLeb | MemoryAddrSleb
                | TypeIndexLeb | GlobalIndexLeb | EventIndexLeb | MemoryAddrRelSleb
                | TableIndexRelSleb | TableNumberLeb | MemoryAddrTlsSleb => RelocCategory::Var32,
                MemoryAddrLeb64 | MemoryAddrSleb64 | TableIndexSleb64 | TableIndexRelSleb64
                | MemoryAddrRelSleb64 | MemoryAddrTlsSleb64 => RelocCategory::Var64,

                TableIndexI32 | MemoryAddrI32 | FunctionOffsetI32 | SectionOffsetI32
                | GlobalIndexI32 | MemoryAddrLocrelI32 | FunctionIndexI32 => RelocCategory::Fixed32,

                MemoryAddrI64 | TableIndexI64 | FunctionOffsetI64 => RelocCategory::Fixed64,
            }
        };

        let value = &data[relocation.relocation_range()];
        let value = match category {
            RelocCategory::Fixed32 => {
                let value = ByteCursor(value).read_u32().unwrap();
                let value = value.wrapping_sub(relocation.addend as u32);

                value as u64
            }
            RelocCategory::Fixed64 => {
                let value = ByteCursor(value).read_u64().unwrap();
                let value = value.wrapping_sub(relocation.addend as u64);

                value as u64
            }
            RelocCategory::Var32 => {
                let value = ByteCursor(value).read_var_u32().unwrap();
                let value = value.wrapping_sub(relocation.addend as u32);

                value as u64
            }
            RelocCategory::Var64 => {
                let value = ByteCursor(value).read_var_u64().unwrap();
                let value = value.wrapping_sub(relocation.addend as u64);

                value as u64
            }
        };

        let local_index = match global_to_local.entry((relocation.ty, relocation.index)) {
            hash_map::Entry::Occupied(entry) => {
                let reloc_idx = *entry.get();
                anyhow::ensure!(local_symbols[reloc_idx] == value);
                reloc_idx
            }
            hash_map::Entry::Vacant(entry) => {
                let reloc_idx = local_symbols.len();
                entry.insert(reloc_idx);
                local_symbols.push(value);
                reloc_idx
            }
        };

        local_relocs.push(LocalReloc {
            index: local_index,
            offset: relocation.offset as usize,
            category,
            addend: (relocation.ty.addend_kind() != RelocAddendKind::None)
                .then_some(relocation.addend),
        });
    }

    Ok((local_symbols, local_relocs))
}
