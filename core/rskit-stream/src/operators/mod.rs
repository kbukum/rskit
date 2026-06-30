/// Stream combining operators: `merge` and `concat`.
pub mod combine;
/// Concurrent processing operators: `rparallel` and `rfan_out`.
pub mod concurrent;
/// Transformation operators: `rmap`, `rfilter`, `rtap`, and `rreduce`.
pub mod transform;
/// Time-based windowing operators: `rbatch`, `rdebounce`, `rthrottle`, `rtumbling_window`.
pub mod windowing;
