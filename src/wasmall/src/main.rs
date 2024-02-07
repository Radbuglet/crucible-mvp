use anyhow::Context;
use wasmall::splitter::split_module;

fn main() -> anyhow::Result<()> {
    let code = std::fs::read(std::env::args().nth(1).context("missing path")?)?;
    let res = split_module(&code)?;
    assert!(code == res.out_buf);

    Ok(())
}
