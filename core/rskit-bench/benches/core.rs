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

// `rskit-errors` is a hot crate: an `AppError` is constructed on every failure
// path. These benches make construction and response conversion measurable.
fn bench_app_error_new(c: &mut Criterion) {
    use rskit::errors::{AppError, ErrorCode};
    c.bench_function("app_error_new", |b| {
        b.iter(|| {
            let err = AppError::new(
                std::hint::black_box(ErrorCode::NotFound),
                std::hint::black_box("resource not found"),
            );
            std::hint::black_box(err.code())
        });
    });
}

fn bench_app_error_to_problem_detail(c: &mut Criterion) {
    use rskit::errors::{AppError, ErrorCode, ProblemDetail};
    let err = AppError::new(ErrorCode::InvalidInput, "bad field")
        .with_detail("field", "email")
        .with_detail("reason", "format");
    c.bench_function("app_error_to_problem_detail", |b| {
        b.iter(|| {
            let pd = ProblemDetail::from(std::hint::black_box(&err));
            std::hint::black_box(pd.status)
        });
    });
}

criterion_group!(
    benches,
    bench_btree_map_insert,
    bench_btree_map_get,
    bench_app_error_new,
    bench_app_error_to_problem_detail,
);
criterion_main!(benches);
