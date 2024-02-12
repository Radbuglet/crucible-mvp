use std::io::Write;

use anyhow::Context;
use wasmall::{
    coder::{WasmallBlob, WasmallMod, WasmallModSeg},
    splitter::split_module,
    util::{ByteCursor, ByteParse, OffsetTracker},
};

fn main() -> anyhow::Result<()> {
    // Compress it
    let code = std::fs::read(std::env::args().nth(1).context("missing path")?)?;
    let archive = split_module(&code)?.archive;

    // Decompress it
    let _guard = OffsetTracker::new(&archive.out_buf);
    let parsed = WasmallMod::parse(&mut ByteCursor(&archive.out_buf))?;
    let mut writer = Vec::new();

    for segment in parsed.segments() {
        match segment? {
            WasmallModSeg::Verbatim(segment) => {
                writer.extend_from_slice(segment.data());
            }
            WasmallModSeg::Blob(segment) => {
                segment.write(
                    &WasmallBlob::parse(&mut ByteCursor(
                        &archive.blob_buf[archive.hashes[&segment.hash()].clone()],
                    ))?,
                    &mut writer,
                )?;
            }
        }
    }

    std::io::stdout().write_all(&writer)?;

    Ok(())
}
