use std::{env, fmt};

use anyhow::Context;
use humansize::{DECIMAL, format_size};
use wasmall::encode::{SplitModuleArgs, split_module};

fn main() -> anyhow::Result<()> {
    let args = env::args().collect::<Vec<String>>();
    let args = args.iter().map(|v| v.as_str()).collect::<Vec<_>>();
    let args = args.as_slice();

    let &[_bin_name, old_bin, new_bin] = args else {
        anyhow::bail!("invalid usage");
    };

    let old_bin = std::fs::read(old_bin).context("failed to read old binary")?;
    let new_bin = std::fs::read(new_bin).context("failed to read new binary")?;

    let old_bin = split_module(SplitModuleArgs {
        src: &old_bin,
        truncate_relocations: true,
        truncate_debug: true,
    })?
    .archive;

    let new_bin = split_module(SplitModuleArgs {
        src: &new_bin,
        truncate_relocations: true,
        truncate_debug: true,
    })?
    .archive;

    let mut sum = 0usize;
    let mut account = |name: &dyn fmt::Display, sz: usize| {
        eprintln!("{name}: {}", format_size(sz, DECIMAL));
        sum += sz;
    };

    account(&"index", new_bin.index_buf.len());

    for (new_hash, new_range) in new_bin.blobs.iter() {
        if old_bin.blobs.contains_key(new_hash) {
            continue;
        }

        account(&new_hash, new_range.len());
    }

    eprintln!();
    eprintln!("Total size: {}", format_size(sum, DECIMAL));

    Ok(())
}
