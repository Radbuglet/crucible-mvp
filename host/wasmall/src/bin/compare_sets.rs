use anyhow::Context;
use rustc_hash::FxHashSet;
use wasmall::splitter::{SplitModuleArgs, split_module};

fn main() -> anyhow::Result<()> {
    let left_src = std::fs::read(std::env::args().nth(1).context("missing left path")?)?;
    let right_src = std::fs::read(std::env::args().nth(2).context("missing right path")?)?;

    let left_mod = split_module(SplitModuleArgs {
        src: &left_src,
        truncate_relocations: true,
    })?;
    let right_mod = split_module(SplitModuleArgs {
        src: &right_src,
        truncate_relocations: true,
    })?;

    let left_set = FxHashSet::from_iter(left_mod.archive.hashes.keys().copied());
    let right_set = FxHashSet::from_iter(right_mod.archive.hashes.keys().copied());

    for diff in left_set.symmetric_difference(&right_set) {
        if left_set.contains(diff) {
            println!("- {diff}");
        } else {
            println!("+ {diff}");
        }
    }

    let left_src_len_post_trunc = left_src.len() - left_mod.bytes_truncated;
    let right_src_len_post_trunc = right_src.len() - right_mod.bytes_truncated;

    println!(
        "Left compression size: {} / {} ({}%), delta: {}",
        left_mod.archive.out_buf.len(),
        left_src_len_post_trunc,
        (left_mod.archive.out_buf.len() as f64
            / (left_src.len() - left_mod.bytes_truncated) as f64)
            * 100.,
        left_mod.archive.out_buf.len() as isize - left_src_len_post_trunc as isize,
    );

    println!(
        "Right compression size: {} / {} ({}%), delta: {}",
        right_mod.archive.out_buf.len(),
        right_src_len_post_trunc,
        (right_mod.archive.out_buf.len() as f64
            / (right_src.len() - right_mod.bytes_truncated) as f64)
            * 100.,
        right_mod.archive.out_buf.len() as isize - right_src_len_post_trunc as isize,
    );

    Ok(())
}
