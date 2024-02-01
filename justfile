run:
	cargo build -p crucible-guest --target wasm32-wasi
	cargo run -p crucible-host target/wasm32-wasi/debug/crucible-guest.wasm
