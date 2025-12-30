//! Benchmarks for compression hot paths.
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sevenz_rust2::perf::BufferPool;

fn bench_buffer_pool(c: &mut Criterion) {
    let pool = BufferPool::new(8, 64 * 1024);
    c.bench_function("buffer_pool_get", |b| {
        b.iter(|| black_box(pool.get()));
    });
}

criterion_group!(benches, bench_buffer_pool);
criterion_main!(benches);
