run:
    cargo build --bin demo-game --target wasm32-unknown-unknown
    cargo run --bin crucible-client -- ./target/wasm32-unknown-unknown/debug/demo-game.wasm
