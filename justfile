run:
	cargo build --release -p crucible-guest --target wasm32-wasi
	cargo run --release -p crucible-host target/wasm32-wasi/release/crucible-guest.wasm
