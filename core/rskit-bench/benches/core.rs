//! Core throughput benchmarks for rskit.
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_btree_map_insert(c: &mut Criterion) {
    use std::collections::BTreeMap;
    c.bench_function("btree_map_insert_1000", |b| {
        b.iter(|| {
            let mut map: BTreeMap<u32, String> = BTreeMap::new();
            for i in 0u32..1000 {
                map.insert(i, format!("value-{i}"));
            }
        });
    });
}

fn bench_btree_map_get(c: &mut Criterion) {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<u32, String> = BTreeMap::new();
    for i in 0u32..1000 {
        map.insert(i, format!("value-{i}"));
    }
    c.bench_function("btree_map_get_hit", |b| {
        b.iter(|| {
            let _ = map.get(&500);
        });
    });
}

criterion_group!(benches, bench_btree_map_insert, bench_btree_map_get);
criterion_main!(benches);
