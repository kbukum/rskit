//! Deterministic randomness context handed to evaluators.
//!
//! An [`EvalContext`] carries a seed derived from the run seed and the sample identifier, so evaluator randomness is reproducible and independent of the order in which samples complete under concurrency. Evaluators obtain a value-stable RNG through [`EvalContext::rng`]; the RNG algorithm is fixed (see [`RNG_ALGORITHM`]) and recorded in run provenance so a seed maps to the same sequence across rebuilds.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rskit_util::hash::ContentHasher;

/// Stable RNG algorithm used to seed evaluators, recorded in run provenance.
///
/// A named algorithm (rather than `rand`'s `StdRng`, whose sequence may change across releases) keeps a seed reproducible at the value level across rebuilds.
pub const RNG_ALGORITHM: &str = "rand_chacha:ChaCha8Rng";

/// Per-sample deterministic randomness handed to an evaluator.
///
/// The backing seed is derived from the run seed and the sample identifier via [`EvalContext::for_sample`], so evaluator randomness stays reproducible and independent of concurrent completion order. [`EvalContext::rng`] yields a fresh, value-stable [`ChaCha8Rng`].
#[derive(Debug, Clone, Copy)]
pub struct EvalContext {
    seed: u64,
}

impl EvalContext {
    /// Creates a context whose RNG is seeded directly with `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// Derives a per-sample context from the run seed and a sample identifier.
    ///
    /// The derivation folds both inputs with length-prefixed framing, so two runs with the same run seed and sample id produce the same per-sample seed regardless of dataset ordering or concurrency.
    #[must_use]
    pub fn for_sample(run_seed: u64, sample_id: &str) -> Self {
        Self {
            seed: derive_seed(run_seed, sample_id),
        }
    }

    /// The deterministic seed backing this context.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Returns a fresh RNG seeded deterministically from [`EvalContext::seed`].
    ///
    /// The algorithm is fixed (see [`RNG_ALGORITHM`]), so the same seed yields an identical sequence across rebuilds.
    #[must_use]
    pub fn rng(&self) -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(self.seed)
    }
}

/// Folds the run seed and sample id with length-prefixed framing into a stable `u64` seed.
fn derive_seed(run_seed: u64, sample_id: &str) -> u64 {
    let mut hasher = ContentHasher::new();
    hasher.update_framed(b"seed", &run_seed.to_le_bytes());
    hasher.update_framed(b"sample", sample_id.as_bytes());
    let hex = hasher.finalize_hex();
    u64::from_str_radix(&hex[..16], 16).unwrap_or(run_seed)
}

#[cfg(test)]
mod tests {
    use rand::RngCore;

    use super::*;

    #[test]
    fn same_seed_yields_identical_sequence() {
        let ctx = EvalContext::new(7);
        let mut a = ctx.rng();
        let mut b = ctx.rng();
        assert_eq!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn per_sample_seeds_are_stable_and_distinct() {
        assert_eq!(
            EvalContext::for_sample(7, "alpha").seed(),
            EvalContext::for_sample(7, "alpha").seed()
        );
        assert_ne!(
            EvalContext::for_sample(7, "alpha").seed(),
            EvalContext::for_sample(7, "beta").seed()
        );
        assert_ne!(
            EvalContext::for_sample(7, "alpha").seed(),
            EvalContext::for_sample(8, "alpha").seed()
        );
    }
}
