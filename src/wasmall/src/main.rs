use anyhow::Context;
use wasmall::splitter::split_module;

fn main() -> anyhow::Result<()> {
    let code = std::fs::read(std::env::args().nth(1).context("missing path")?)?;
    let archive = split_module(&code)?;

    dbg!(archive.out_buf.len());
    dbg!(code.len());

    Ok(())
}
