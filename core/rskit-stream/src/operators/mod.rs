/// Shared accumulation-buffer helpers for `windowing` and `rate`.
mod buffer;
/// Stream combining operators: `merge` and `concat`.
pub mod combine;
/// Concurrent processing operators: `rparallel` and `rfan_out`.
pub mod concurrent;
/// Trailing-edge rate-limiting operators: `rdebounce`, `rdebounce_batch`, `rthrottle`.
pub mod rate;
/// Transformation operators: `rmap`, `rfilter`, `rtap`, and `rreduce`.
pub mod transform;
/// Leading-edge windowing operators: `rbatch`, `rsliding_window`, `rtumbling_window`.
pub mod windowing;
