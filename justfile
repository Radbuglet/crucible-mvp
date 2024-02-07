analyze:
	just build-guest
	cargo run -p wasmall -- target/wasm32-wasi/debug/example-guest.wasm

build-guest:
	cargo rustc --target wasm32-wasi -p example-guest -- -C link-args="-r" -C link-dead-code
