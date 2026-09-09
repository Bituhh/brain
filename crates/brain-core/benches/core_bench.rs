//! Criterion bench skeleton (Req 15.2, ENG-11). Real benchmarks land as
//! each subsystem does; this stub keeps `cargo bench` wired from Step 1.

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_version(c: &mut Criterion) {
    c.bench_function("version", |b| b.iter(brain_core::version));
}

criterion_group!(benches, bench_version);
criterion_main!(benches);
