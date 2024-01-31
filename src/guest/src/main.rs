use std::time::Instant;

#[link(wasm_import_module = "crucible0")]
extern "C" {
    fn do_stuff();

    fn send_ipc(base: usize, len: usize);
}

fn main() {
    // Experiment 1
    let mut data = Vec::new();
    let time = Instant::now();

    for _ in 0..10_000 {
        data.push((420u64, 69u64));
        std::hint::black_box(&mut data);
    }

    unsafe { send_ipc(data.as_ptr() as usize, data.len()) };

    eprintln!("Elapsed 1: {:?}", time.elapsed());

    // Experiment 2
    let time = Instant::now();

    for _ in 0..10_000 {
        unsafe { do_stuff() };
    }

    eprintln!("Elapsed 2: {:?}", time.elapsed());

    // Experiment 3
    let time = Instant::now();
    let do_stuff = std::hint::black_box(|| {});

    for _ in 0..10_000 {
        do_stuff();
    }

    eprintln!("Elapsed 2: {:?}", time.elapsed());
}
