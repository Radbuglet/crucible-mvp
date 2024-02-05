use anyhow::Context;
use crucible_host::rt::splitter::split_wasm;

fn main() -> anyhow::Result<()> {
    let module_data = std::fs::read(std::env::args().nth(1).context("missing module path")?)?;

    let split_data = split_wasm(&module_data);

    println!(
        "Compression ratio: {}",
        (split_data.stripped.len() as f64 / module_data.len() as f64) * 100.
    );

    Ok(())
}
