client: build-guest
    cargo run --bin crucible-host-client -- ./target/wasm32-unknown-unknown/debug/demo-game.wasm

server: build-guest
    cargo run -p crucible-host-server -- target/wasm32-unknown-unknown/debug/demo-game.wasm

build-guest:
    cargo rustc -p demo-game --target wasm32-unknown-unknown -- -C link-args="--emit-relocs"

roundtrip: build-guest
    mkdir -p private/
    cp target/wasm32-unknown-unknown/debug/demo-game.wasm private/one.wasm
    cargo run -p wasmall --example roundtrip -- private/one.wasm > private/two.wasm
    wasm2wat private/one.wasm > private/one.wat
    wasm2wat private/two.wasm > private/two.wat
    diff private/one.wat private/two.wat > private/diff.txt

compare-save-left: build-guest
    mkdir -p private/
    cp target/wasm32-unknown-unknown/debug/demo-game.wasm private/compare_left.wasm

compare-save-right: build-guest
    mkdir -p private/
    cp target/wasm32-unknown-unknown/debug/demo-game.wasm private/compare_right.wasm

compare:
    cargo run -p wasmall --example compare -- private/compare_left.wasm private/compare_right.wasm
