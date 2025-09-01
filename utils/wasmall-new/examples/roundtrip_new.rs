use anyhow::Context;
use wasmall_new::{
    encode::{SplitModuleArgs, split_module},
    format::WasmallIndex,
    utils::{ByteCursor, ByteParse as _, OffsetTracker},
};

fn main() -> anyhow::Result<()> {
    // Compress it.
    let code = std::fs::read(std::env::args().nth(1).context("missing path")?)?;
    let archive = split_module(SplitModuleArgs {
        src: &code,
        truncate_relocations: true,
    })?
    .archive;

    dbg!(archive.blob_buf.len(), archive.index_buf.len(), code.len());

    // Decompress it.
    let mut _guard = OffsetTracker::new(&archive.index_buf);
    let reader = WasmallIndex::parse(&mut ByteCursor(&archive.index_buf))?;

    for chunk in reader.chunks() {
        let chunk = chunk?;
    }

    Ok(())
}
