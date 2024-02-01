use crucible_shared::{DemoStructure, WasmPtr, WasmSlice};

#[link(wasm_import_module = "crucible0")]
extern "C" {
    fn read_my_struct(args: WasmPtr<DemoStructure>);
}

fn main() {
    let funnies = [42, 19];
    let demo = DemoStructure {
        funnies: WasmSlice::new_guest(&funnies),
    };

    unsafe {
        read_my_struct(WasmPtr::new_guest(&demo));
    }
}
