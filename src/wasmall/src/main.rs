use anyhow::Context;
use wasmall::{
    coder::{WasmallMod, WasmallModSeg},
    splitter::split_module,
    util::{ByteCursor, ByteParse, OffsetTracker},
};

fn main() -> anyhow::Result<()> {
    let code = std::fs::read(std::env::args().nth(1).context("missing path")?)?;
    let archive = split_module(&code)?;

    dbg!(archive.out_buf.len());

    let _guard = OffsetTracker::new(&archive.out_buf);
    let parsed = WasmallMod::parse(&mut ByteCursor(&archive.out_buf))?;

    for segment in parsed.segments() {
        match segment? {
            WasmallModSeg::Verbatim(segment) => {
                println!("Found verbatim; length = {}", segment.data().len())
            }
            WasmallModSeg::Blob(segment) => println!("Found blob; hash: {}", segment.hash()),
        }
    }

    Ok(())
}
