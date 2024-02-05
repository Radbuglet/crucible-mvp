use crucible_shared::{DemoStructure, WasmPtr, WasmSlice, WasmStr};

#[link(wasm_import_module = "crucible0")]
extern "C" {
    fn read_my_struct(args: WasmPtr<DemoStructure>);

    fn set_name(name: WasmStr);

    fn log_strings(names: WasmSlice<WasmStr>);

}

fn main() {
    let funnies = [42, 19];
    let demo = DemoStructure {
        funnies: WasmSlice::new_guest(&funnies),
    };

    unsafe {
        read_my_struct(WasmPtr::new_guest(&demo));
        set_name(WasmStr::new_guest(format!("whee woo {}", 1 + 1).as_str()));
        log_strings(WasmSlice::new_guest(&[
            WasmStr::new_guest(format!("whee {}", 2 + 1).as_str()),
            WasmStr::new_guest(format!("woo {}", 3 + 1).as_str()),
        ]));
    }

    dbg!(woo as usize);
}

fn woo() {}
