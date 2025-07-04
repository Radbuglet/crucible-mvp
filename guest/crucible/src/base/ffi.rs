use std::alloc::Layout;

#[unsafe(no_mangle)]
extern "C" fn crucible_mem_alloc(size: usize, align: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, align).unwrap();

    if layout.size() == 0 {
        align as *mut u8
    } else {
        unsafe { std::alloc::alloc(layout) }
    }
}
