use anyhow::Context;
use wasmall::splitter::split_module;

fn main() -> anyhow::Result<()> {
    let code = std::fs::read(std::env::args().nth(1).context("missing path")?)?;
    split_module(&code, |p| {
        for (i, _) in p.symbols().iter().enumerate() {
            p.map_to(i, i as u32);
        }
        Ok(())
    })
}
