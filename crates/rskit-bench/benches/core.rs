//! Core throughput benchmarks for rskit.
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_registry_insert(c: &mut Criterion) {
    use rskit::registry::TypedRegistry;
    c.bench_function("registry_insert_1000", |b| {
        b.iter(|| {
            let reg: TypedRegistry<u32, String> = TypedRegistry::new();
            for i in 0u32..1000 {
                reg.insert(i, format!("value-{i}"));
            }
        });
    });
}

fn bench_registry_get(c: &mut Criterion) {
    use rskit::registry::TypedRegistry;
    let reg: TypedRegistry<u32, String> = TypedRegistry::new();
    for i in 0u32..1000 {
        reg.insert(i, format!("value-{i}"));
    }
    c.bench_function("registry_get_hit", |b| {
        b.iter(|| {
            let _ = reg.get(&500);
        });
    });
}

criterion_group!(benches, bench_registry_insert, bench_registry_get);
criterion_main!(benches);
