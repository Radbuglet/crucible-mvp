use std::{future::Future, mem::size_of, ops::Range};

use anyhow::Context;
use blake3::{hash, Hash};
use wasmparser::BinaryReader;

use crate::util::map::FxHashMap;

// === Shared === //

const HASHES_SECTION_NAME: &str = "csplitter0_hashes";

fn rewrite_relocated<Rewriter>(
    buf: &[u8],
    writer: &mut Vec<u8>,
    replacements: impl IntoIterator<Item = (usize, Rewriter)>,
) -> anyhow::Result<()>
where
    Rewriter: FnOnce(&[u8], &mut Vec<u8>) -> anyhow::Result<usize>,
{
    // Invariant: `buf_cursor` is always less than or equal to the `buf` length.
    let mut buf_cursor = 0;

    for (reloc_start, replace_with) in replacements {
        // While there are still relocations affecting bytes in our at the end of our buffer...
        debug_assert!(reloc_start >= buf_cursor);

        if reloc_start > buf.len() {
            break;
        }

        // Push the bytes up until the start of the relocation.
        writer.extend_from_slice(&buf[buf_cursor..reloc_start]);

        // Push the new relocation bytes.
        let reloc_end = reloc_start + replace_with(&buf[reloc_start..], writer)?;

        // Bump the `buf_cursor`
        buf_cursor = reloc_end;
    }

    // Ensure that we write the remaining bytes of our buffer.
    writer.extend_from_slice(&buf[buf_cursor..]);

    Ok(())
}

// === Split === //

#[derive(Debug)]
pub struct WasmSplitResult {
    pub stripped: Vec<u8>,
    pub functions_buf: Vec<u8>,
    pub functions_map: FxHashMap<Hash, Range<usize>>,
}

pub fn split_wasm(data: &[u8]) -> anyhow::Result<WasmSplitResult> {
    use wasmparser::Payload::*;

    let parser = wasmparser::Parser::new(0);

    // Run a first pass of the parser, collecting the ranges of each section affected by a relocation.
    // We use these relocations to determine parts of the function code should be zeroed during hashing,
    // leaving their specific values to be supplied by the stripped binary hashes table.
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    enum RelocationSize {
        U32,
        VarU32,
        VarI32,
    }

    let mut all_relocations = FxHashMap::<u32, Vec<(u32, RelocationSize)>>::default();

    for payload in parser.clone().parse_all(data) {
        let payload = payload?;

        let CustomSection(cs) = &payload else {
            continue;
        };

        if !cs.name().starts_with("reloc.") {
            continue;
        }

        // Format: https://github.com/WebAssembly/tool-conventions/blob/4dd47d204df0c789c23d246bc4496631b5c199c4/Linking.md#relocation-sections
        let mut reader = BinaryReader::new_with_offset(cs.data(), cs.data_offset());
        let section = reader.read_var_u32()?;
        let count = reader.read_var_u32()?;

        let offsets = all_relocations.entry(section).or_default();

        for i in 0..count {
            let start_offset = reader.original_position();
            let ty = reader.read_u8()?;
            let offset = reader.read_var_u32()?;
            let _index = reader.read_var_u32()?;

            let (ty_size, expecting_addend) = match ty {
                // R_WASM_FUNCTION_INDEX_LEB
                0 => (RelocationSize::VarU32, false),
                // R_WASM_TABLE_INDEX_SLEB
                1 => (RelocationSize::VarI32, false),
                // R_WASM_TABLE_INDEX_I32
                2 => (RelocationSize::U32, false),
                // R_WASM_MEMORY_ADDR_LEB
                3 => (RelocationSize::VarU32, true),
                // R_WASM_MEMORY_ADDR_SLEB
                4 => (RelocationSize::VarI32, true),
                // R_WASM_MEMORY_ADDR_I32
                5 => (RelocationSize::U32, true),
                // R_WASM_TYPE_INDEX_LEB
                6 => (RelocationSize::VarU32, false),
                // R_WASM_GLOBAL_INDEX_LEB
                7 => (RelocationSize::VarU32, false),
                // R_WASM_FUNCTION_OFFSET_I32
                8 => (RelocationSize::U32, true),
                // R_WASM_SECTION_OFFSET_I32
                9 => (RelocationSize::U32, true),
                // R_WASM_EVENT_INDEX_LEB
                10 => (RelocationSize::VarU32, false),
                // R_WASM_GLOBAL_INDEX_I32
                13 => (RelocationSize::U32, false),
                _ => anyhow::bail!(
                    "unknown relocation kind {ty} at offset {start_offset} (index {i})"
                ),
            };

            if expecting_addend {
                let _addend = reader.read_var_u32()?;
            }

            offsets.push((offset, ty_size));
        }
    }

    for list in all_relocations.values_mut() {
        list.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
    }

    // Now, run a second pass building both the stripped binary and the hashed function collection.

    // The writer for the stripped binary.
    let mut writer = wasm_encoder::Module::new();

    // A buffer of all hashed function descriptors glued back to back.
    let mut functions_buf = Vec::new();

    // A map from function hashes to the range of bytes describing them in the `functions_buf`
    let mut functions_map = FxHashMap::default();

    // A buffer with all the data needed for the function hashes section.
    let mut func_segment_builder = Vec::<u8>::new();

    // A counter for the current real section.
    let mut next_section_idx = 0u32;

    // The remaining list of relocations to apply on the current section.
    let mut section_relocations = all_relocations.get(&0).map_or(&[][..], |v| v.as_slice());

    // The byte offset to the start of the section, which is the offset of the first byte after the
    // size.
    let mut section_start = 0;

    for payload in parser.parse_all(data) {
        let payload = payload?;

        // Determine whether this payload actually corresponds to a real section in the binary. Versions
        // are part of the header, `End` sections should essential be EOFs, and `CodeSectionEntry` is
        // an element of the actual `CodeSectionStart` section.
        let is_real = !matches!(payload, Version { .. } | End(_) | CodeSectionEntry(_));

        // Update section state
        if is_real {
            section_start = payload.as_section().unwrap().1.start;
            section_relocations = all_relocations
                .get(&next_section_idx)
                .map_or(&[][..], |v| v.as_slice());
            next_section_idx += 1;
        }

        // If we found a code section, add it to the code hashes
        if let CodeSectionEntry(body) = &payload {
            let body_rel_start = body.range().start - section_start;

            // Consume through the `section_relocations` buffer until we can see this code section.
            while section_relocations
                .first()
                .filter(|(loc, _)| (*loc as usize) < body_rel_start)
                .is_some()
            {
                section_relocations = &section_relocations[1..];
            }

            // Write the function data with all the relocations applied.
            let start_in_func_buf = functions_buf.len();
            rewrite_relocated(
                &data[body.range()],
                &mut functions_buf,
                section_relocations.iter().map(|(offset, size)| {
                    (
                        *offset as usize - body_rel_start,
                        move |buf: &[u8], writer: &mut Vec<_>| match size {
                            RelocationSize::U32 => {
                                anyhow::ensure!(buf.len() >= 4);
                                writer.extend_from_slice(&[0, 0, 0, 0]);
                                Ok(4)
                            }
                            RelocationSize::VarU32 | RelocationSize::VarI32 => {
                                let mut len = 0;
                                for ch in buf.iter().take(5) {
                                    len += 1;
                                    if ch & 0x80 == 0 {
                                        break;
                                    }
                                }

                                writer.push(0);
                                Ok(len)
                            }
                        },
                    )
                }),
            )?;

            let function_data = &mut functions_buf[start_in_func_buf..];

            // Hash the function and add it into the hashes section.
            let hash = hash(function_data);
            functions_map.insert(hash, start_in_func_buf..functions_buf.len());

            // TODO: Include relocation locations
            func_segment_builder.extend_from_slice(hash.as_bytes());
        }

        // Write non-filtered sections into the output module
        if is_real && !matches!(payload, CodeSectionStart { .. } | CustomSection(_),) {
            // Write it to the output stream verbatim
            struct VerbatimSection<'a> {
                id: u8,
                data: &'a [u8],
            }

            impl wasm_encoder::Section for VerbatimSection<'_> {
                fn id(&self) -> u8 {
                    self.id
                }
            }

            impl wasm_encoder::Encode for VerbatimSection<'_> {
                fn encode(&self, sink: &mut Vec<u8>) {
                    sink.extend_from_slice(self.data);
                }
            }

            let (id, range) = payload.as_section().unwrap();
            let data = &data[range];
            writer.section(&VerbatimSection { id, data });
        }
    }

    // Add the function hashes section to the buffer
    writer.section(&wasm_encoder::CustomSection {
        name: HASHES_SECTION_NAME.into(),
        data: func_segment_builder.as_slice().into(),
    });

    Ok(WasmSplitResult {
        stripped: writer.finish(),
        functions_buf,
        functions_map,
    })
}

// === Merge === //

pub struct CodeReceiver<'a> {
    hashes: &'a [Hash],
    buf: &'a mut Vec<u8>,
    len: &'a mut u32,
}

impl<'a> CodeReceiver<'a> {
    pub fn hashes(&self) -> &'a [Hash] {
        self.hashes
    }

    pub fn write_code_data(&mut self, data: &[u8]) {
        assert!(*self.len <= self.hashes.len() as u32);

        self.buf.extend(data);
        *self.len += 1;
    }
}

pub async fn merge_wasm<F>(data: &mut Vec<u8>, f: F) -> anyhow::Result<()>
where
    F: FnOnce(CodeReceiver<'_>) -> Box<dyn Future<Output = anyhow::Result<()>> + '_>,
{
    // Scan for the hash section
    let parser = wasmparser::Parser::new(0);
    let mut hashes = None;

    for payload in parser.parse_all(data) {
        let wasmparser::Payload::CustomSection(payload) = payload? else {
            continue;
        };

        if payload.name() != HASHES_SECTION_NAME {
            continue;
        }

        anyhow::ensure!(hashes.is_none(), "more than one splitter section specified");
        anyhow::ensure!(
            payload.data().len() % size_of::<Hash>() == 0,
            "splitter hashes section has an invalid size"
        );

        hashes = Some(unsafe {
            // FIXME: blake3::Hash is not `repr(transparent)`
            std::slice::from_raw_parts(
                payload.data().as_ptr().cast::<Hash>(),
                payload.data().len() / size_of::<Hash>(),
            )
        });
    }

    // N.B. we know `hashes`' length is less than u32::MAX because the section is limited to
    // `u32::MAX` bytes.
    let Some(hashes) = hashes else {
        return Ok(());
    };

    // If present, begin writing the function section, referencing:
    // https://webassembly.github.io/spec/core/binary/modules.html#binary-codesec
    //
    //
    // We begin by writing the one-byte section ID.
    data.push(wasm_encoder::SectionId::Code.into());

    // Then, we pad some space for the section size, which is a u32.
    let code_size_offset = data.len();
    data.extend([0, 0, 0, 0]);

    // We also need to pad space for the function length, which is another u32.
    let code_len_offset = data.len();
    data.extend([0, 0, 0, 0]);

    // Now, let's populate this vector.
    let mut len = 0;
    Box::into_pin(f(CodeReceiver {
        hashes,
        buf: data,
        len: &mut len,
    }))
    .await?;

    // Finally, let's fill out the fields we left as placeholders.

    // The section begins with `code_len` as its first byte.
    let bytes_in_section =
        u32::try_from(data.len() - code_len_offset).context("code section is too big")?;

    data[code_size_offset..][..4].copy_from_slice(&bytes_in_section.to_le_bytes());
    data[code_len_offset..][..4].copy_from_slice(&len.to_le_bytes());

    Ok(())
}
