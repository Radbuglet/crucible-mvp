run: build
    cargo run --bin crucible-client -- ./target/wasm32-unknown-unknown/debug/demo-game.wasm

build:
    cargo rustc -p demo-game --target wasm32-unknown-unknown -- -C link-args="--emit-relocs"

server:
    cargo run -p crucible-server

roundtrip: build
    mkdir -p private/
    cargo run -p wasmall --example roundtrip -- target/wasm32-unknown-unknown/debug/demo-game.wasm > private/one.wasm
    wasm2wat target/wasm32-unknown-unknown/debug/demo-game.wasm > private/two.wat
    wasm2wat private/one.wasm > private/one.wat
    diff private/one.wat private/two.wat > private/diff.txt

roundtrip_new: build
    cargo run -p wasmall-new --example roundtrip_new -- target/wasm32-unknown-unknown/debug/demo-game.wasm

compare-save-left: build
    mkdir -p private/
    cp target/wasm32-unknown-unknown/debug/demo-game.wasm private/compare_left.wasm

compare-save-right: build
    mkdir -p private/
    cp target/wasm32-unknown-unknown/debug/demo-game.wasm private/compare_right.wasm

compare:
    cargo run -p wasmall --example compare_sets -- private/compare_left.wasm private/compare_right.wasm
