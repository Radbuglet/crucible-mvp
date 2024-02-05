run:
	cargo build -p crucible-guest --target wasm32-wasi
	cargo run -p crucible-host --bin marshal_demo -- target/wasm32-wasi/debug/crucible-guest.wasm

explore:
	cargo rustc -p crucible-guest --target wasm32-wasi -- -C linker=./my_linker.sh -C linker-flavor=wasm-ld
	cargo run -p crucible-host --bin split_demo -- target/wasm32-wasi/debug/crucible-guest.wasm
