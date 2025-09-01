use anyhow::Context;
use wasmall_new::{SplitModuleArgs, split_module};

fn main() -> anyhow::Result<()> {
    // Compress it
    let code = std::fs::read(std::env::args().nth(1).context("missing path")?)?;
    let archive = split_module(SplitModuleArgs {
        src: &code,
        truncate_relocations: false,
    })?
    .archive;

    // TODO

    Ok(())
}
