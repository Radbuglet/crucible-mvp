build-guest:
	cargo rustc --target wasm32-wasi -p example-guest -- -C link-args="-r" -C link-dead-code

roundtrip:
	just build-guest
	cargo run -p wasmall --bin roundtrip -- target/wasm32-wasi/debug/example-guest.wasm > private/one.wasm
	wasm2wat target/wasm32-wasi/debug/example-guest.wasm > private/two.wat
	wasm2wat private/one.wasm > private/one.wat

compare-save-left:
	just build-guest && cp target/wasm32-wasi/debug/example-guest.wasm private/compare_left.wasm

compare-save-right:
	just build-guest && cp target/wasm32-wasi/debug/example-guest.wasm private/compare_right.wasm

compare:
	cargo run -p wasmall --bin compare_sets -- private/compare_left.wasm private/compare_right.wasm
