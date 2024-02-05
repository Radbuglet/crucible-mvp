set positional-arguments

run:
	cargo build -p crucible-guest --target wasm32-wasi
	cargo run -p crucible-host --bin marshal_demo -- target/wasm32-wasi/debug/crucible-guest.wasm

@link target:
	cargo rustc -p crucible-guest --target wasm32-wasi -- -C linker=./my_linker.sh -C linker-flavor=wasm-ld
	cp target/wasm32-wasi/debug/crucible-guest.wasm $1

@compare target_1 target_2:
	cargo run -p crucible-host --bin split_demo -- $1 $2
