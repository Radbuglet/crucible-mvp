run: build
    cargo run --bin crucible-client -- ./target/wasm32-unknown-unknown/debug/demo-game.wasm

build:
    # TODO: add back `-- -C link-args="-r" -C link-dead-code`?
    cargo rustc -p demo-game --target wasm32-unknown-unknown -- -C link-args="--emit-relocs"

roundtrip: build
    mkdir -p private/
    cargo run -p wasmall --bin roundtrip -- target/wasm32-unknown-unknown/debug/demo-game.wasm > private/one.wasm
    wasm2wat target/wasm32-unknown-unknown/debug/demo-game.wasm > private/two.wat
    wasm2wat private/one.wasm > private/one.wat
    diff private/one.wat private/two.wat > private/diff.txt

compare-save-left: build
    mkdir -p private/
    cp target/wasm32-unknown-unknown/debug/demo-game.wasm private/compare_left.wasm

compare-save-right: build
    mkdir -p private/
    cp target/wasm32-unknown-unknown/debug/demo-game.wasm private/compare_right.wasm

compare:
    cargo run -p wasmall --bin compare_sets -- private/compare_left.wasm private/compare_right.wasm
