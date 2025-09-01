use std::io::Write as _;

use anyhow::Context;
use wasmall_new::{
    encode::{SplitModuleArgs, split_module},
    format::{WasmallBlob, WasmallIndex, WasmallModChunk},
    utils::{ByteCursor, ByteParse as _, OffsetTracker},
};

fn main() -> anyhow::Result<()> {
    // Compress it.
    let code = std::fs::read(std::env::args().nth(1).context("missing path")?)?;
    let archive = split_module(SplitModuleArgs {
        src: &code,
        truncate_relocations: false,
    })?
    .archive;

    dbg!(archive.blob_buf.len(), archive.index_buf.len(), code.len());

    // Decompress it.
    let _guard = OffsetTracker::new(&archive.index_buf);
    let reader = WasmallIndex::parse(&mut ByteCursor(&archive.index_buf))?;

    let mut out = Vec::new();

    for chunk in reader.chunks() {
        let chunk = chunk?;

        match chunk {
            WasmallModChunk::Verbatim(chunk) => {
                out.extend_from_slice(chunk.data());
            }
            WasmallModChunk::Blob(chunk) => {
                let blob = &archive.blob_buf[archive.blobs[&chunk.hash()].clone()];

                let _guard = OffsetTracker::new(blob);

                chunk.write(&WasmallBlob::parse(&mut ByteCursor(blob))?, &mut out)?;
            }
        }
    }

    std::io::stdout().write_all(&out)?;

    Ok(())
}
