//! HTTP router dispatch benchmarks.
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_router_smoke(c: &mut Criterion) {
    c.bench_function("router_build", |b| {
        b.iter(|| {
            axum::Router::<()>::new()
                .route("/healthz", axum::routing::get(|| async { "ok" }))
                .route("/api/v1/users", axum::routing::get(|| async { "[]" }))
        });
    });
}

criterion_group!(benches, bench_router_smoke);
criterion_main!(benches);
