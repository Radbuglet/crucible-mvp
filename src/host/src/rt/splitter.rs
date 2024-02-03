use std::{future::Future, mem::size_of, ops::Range};

use anyhow::Context;
use blake3::{hash, Hash};
use hashbrown::hash_map;

use crate::util::map::FxHashMap;

const HASHES_SECTION_NAME: &str = "csplitter0_hashes";

// === Split === //

#[derive(Debug)]
pub struct WasmSplitResult {
    pub stripped: Vec<u8>,
    pub functions_buf: Vec<u8>,
    pub functions_map: FxHashMap<Hash, Range<usize>>,
}

pub fn split_wasm(data: &[u8]) -> WasmSplitResult {
    use wasmparser::Payload::*;

    let parser = wasmparser::Parser::new(0);
    let mut writer = wasm_encoder::Module::new();

    let mut functions_buf = Vec::new();
    let mut functions_map = FxHashMap::default();
    let mut hashes_data = Vec::<u8>::new();

    // Extract all function code from the binary
    for payload in parser.parse_all(data) {
        let payload = payload.unwrap();

        // If we found a code section, add it to the code hashes
        if let CodeSectionEntry(body) = &payload {
            let data = &data[body.range()];
            let hash = hash(data);

            // If this is a new section, add it to the buffer.
            if let hash_map::Entry::Vacant(entry) = functions_map.entry(hash) {
                let start = functions_buf.len();
                functions_buf.extend_from_slice(data);
                entry.insert(start..functions_buf.len());
            }

            // Mark the entry in the table
            hashes_data.extend_from_slice(hash.as_bytes());
        }

        // Write non-filtered sections into the output module
        if !matches!(
            payload,
            // If this payload corresponds to a real section...
            Version { .. } | End(_) | CodeSectionEntry(_) |
			// ...and we don't want to filter it out...
			CodeSectionStart { .. } | CustomSection(_),
        ) {
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
        data: hashes_data.as_slice().into(),
    });

    WasmSplitResult {
        stripped: writer.finish(),
        functions_buf,
        functions_map,
    }
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
