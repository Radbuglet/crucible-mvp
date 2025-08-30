use std::fs;

use criterion::{Criterion, criterion_group, criterion_main};
use fastcdc::v2020::FastCDC;
use push_fastcdc::{GearConfig, GearState, GearTablesRef};

pub fn criterion_benchmark(c: &mut Criterion) {
    let read_result = fs::read("test/SekienAkashita.jpg").unwrap();

    c.bench_function("reference", |b| {
        b.iter(|| {
            let cdc = FastCDC::new(&read_result, 4096, 16384, 65535);

            for _ in cdc {}
        });
    });

    c.bench_function("push", |b| {
        b.iter(|| {
            let config = GearConfig::new(4096, 16384, 65535);
            let tables = GearTablesRef::new();

            let mut state = GearState::new();

            let mut remaining = read_result.as_slice();

            while !remaining.is_empty() {
                let (read, _) = state.push(&config, tables, remaining);
                remaining = &remaining[read..];
            }
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
