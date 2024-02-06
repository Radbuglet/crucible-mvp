use rustc_hash::FxHashMap;
use wasmparser::{
    BinaryReader, FromReader, Linking, LinkingSectionReader, Parser, Payload, SymbolInfo,
};

use crate::{reloc::RelocSection, util::VecExt};

pub fn split_module(
    src: &[u8],
    partition: impl FnOnce(&mut ModulePartitioner<'_>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    // Run a first pass to collect all the symbols and relocations from the file.
    let mut orig_sym_map = Vec::new();
    let mut orig_fn_idx_to_sym = Vec::<Option<_>>::new();
    let mut orig_reloc_map = FxHashMap::<u32, Vec<_>>::default();

    {
        let mut did_find_linking = false;

        for payload in Parser::new(0).parse_all(src) {
            match payload? {
                Payload::CustomSection(payload) if payload.name() == "linking" => {
                    did_find_linking = true;

                    let reader = LinkingSectionReader::new(payload.data(), payload.data_offset())?;

                    for subsection in reader.subsections() {
                        let subsection = subsection?;

                        if let Linking::SymbolTable(stab) = subsection {
                            for (sym, info) in stab.into_iter().enumerate() {
                                let info = info?;
                                orig_sym_map.push(info);

                                match info {
                                    SymbolInfo::Func { index, .. } => {
                                        let entry = orig_fn_idx_to_sym.ensure_index(index as usize);

                                        if let Some(orig) = entry {
                                            println!(
                                                "Double assignment: {:?}, {:?}",
                                                match &orig_sym_map[*orig] {
                                                    SymbolInfo::Func { name, .. } => name,
                                                    _ => unreachable!(),
                                                },
                                                match &orig_sym_map[sym] {
                                                    SymbolInfo::Func { name, .. } => name,
                                                    _ => unreachable!(),
                                                }
                                            );
                                        }
                                        *entry = Some(sym);
                                    }
                                    SymbolInfo::Data {
                                        symbol: Some(..), ..
                                    } => {}
                                    SymbolInfo::Data {
                                        name, symbol: None, ..
                                    } => println!("{name:?}"),
                                    SymbolInfo::Global { .. } => {}
                                    SymbolInfo::Section { .. } => {}
                                    SymbolInfo::Event { .. } => {}
                                    SymbolInfo::Table { .. } => {}
                                }
                            }
                        }
                    }
                }
                Payload::CustomSection(payload) if payload.name().starts_with("reloc.") => {
                    let relocs = RelocSection::from_reader(&mut BinaryReader::new_with_offset(
                        payload.data(),
                        payload.data_offset(),
                    ))?;
                    let out_vec = orig_reloc_map.entry(relocs.target_section).or_default();

                    for reloc in relocs.entries {
                        out_vec.push(reloc?);
                    }
                }
                _ => {}
            }
        }

        anyhow::ensure!(
            did_find_linking,
            "failed to split module; no linking section specified"
        );

        for vec in orig_reloc_map.values_mut() {
            vec.sort_unstable_by(|a, b| a.offset.cmp(&b.offset));
        }
    }

    // Ensure that every single object is covered by the symbol map since, otherwise, we can't split
    // them.
    for (i, hehe) in orig_fn_idx_to_sym.iter().enumerate() {
        if hehe.is_none() {
            println!("{i} has no symbol!");
        }
    }

    // Now, partition each exported symbol into a submodule.
    let mut sym_to_mod_map = (0..orig_sym_map.len())
        .map(|_| (u32::MAX, 0))
        .collect::<Vec<_>>();

    {
        let mut partitioner = ModulePartitioner {
            module_map: Vec::new(),
            unmapped_syms: orig_sym_map.len(),
            orig_syms: &orig_sym_map,
            sym_to_mod_map: &mut sym_to_mod_map,
        };

        partition(&mut partitioner)?;
        assert_eq!(partitioner.unmapped_syms, 0);
    }

    Ok(())
}

#[derive(Debug)]
pub struct ModulePartitioner<'a> {
    module_map: Vec<u32>,
    unmapped_syms: usize,
    orig_syms: &'a [SymbolInfo<'a>],
    sym_to_mod_map: &'a mut [(u32, u32)],
}

impl<'a> ModulePartitioner<'a> {
    pub fn symbols(&self) -> &'a [SymbolInfo<'a>] {
        self.orig_syms
    }

    pub fn map_to(&mut self, symbol: usize, module: u32) {
        let module_idx = self.module_map.ensure_index(module as usize);
        let entry = &mut self.sym_to_mod_map[symbol];
        assert_eq!(entry.0, u32::MAX);

        *entry = (module, *module_idx);
        *module_idx += 1;
        self.unmapped_syms -= 1;
    }
}
