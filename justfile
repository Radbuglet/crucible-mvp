analyze:
	just build-guest
	cargo run -p wasmall -- target/wasm32-wasi/debug/example-guest.wasm

build-guest:
	cargo rustc --target wasm32-wasi -p example-guest -- -C link-args="-r" -C link-dead-code

roundtrip:
	just analyze > private/one.wasm
	xxd private/one.wasm > private/one.txt
	xxd target/wasm32-wasi/debug/example-guest.wasm > private/two.txt
