use anyhow::Context;
use crucible_host::rt::splitter::{merge_wasm, split_wasm};
use hashbrown::HashSet;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let module_data_1 = std::fs::read(std::env::args().nth(1).context("missing 1st module path")?)?;
    let split_data_1 = split_wasm(&module_data_1)?;

    let module_data_2 = std::fs::read(std::env::args().nth(2).context("missing 2nd module path")?)?;
    let split_data_2 = split_wasm(&module_data_2)?;

    let funcs1 = split_data_1.functions_map.keys().collect::<HashSet<_>>();
    let funcs2 = split_data_2.functions_map.keys().collect::<HashSet<_>>();

    println!(
        "Sizes: {} {}",
        split_data_1.stripped.len(),
        split_data_2.stripped.len()
    );

    println!("Differences:");
    for diff in funcs1.symmetric_difference(&funcs2) {
        if funcs1.contains(diff) {
            println!("- {diff}");
        } else {
            println!("+ {diff}");
        }
    }

    let mut new_wasm = split_data_1.stripped.clone();
    merge_wasm(&mut new_wasm, |[], mut receiver| {
        let functions_buf = &split_data_1.functions_buf;
        let functions_map = &split_data_1.functions_map;

        Box::new(async move {
            for hash in receiver.hashes() {
                receiver.write_code_data(&functions_buf[functions_map[hash].clone()]);
            }

            Ok(())
        })
    })
    .await?;

    std::fs::write("out.wasm", &new_wasm)?;

    Ok(())
}
