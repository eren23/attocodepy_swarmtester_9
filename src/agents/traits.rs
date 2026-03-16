use rand::Rng;
use rand_distr::{Distribution, Normal};

use crate::types::MerchantTraits;

/// Clamp a value into [0.0, 1.0].
#[inline]
fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

impl MerchantTraits {
    /// Generate random traits using a normal distribution (mean 0.5, σ 0.15)
    /// clamped to [0, 1].
    ///
    /// # Trait behavior modulation
    ///
    /// * **risk_tolerance** (0–1):
    ///   - Low: strongly avoids DANGER reputation zones, takes safer routes,
    ///     flees from bandits at greater distance.
    ///   - High: ignores DANGER signals, takes shorter but riskier paths,
    ///     only flees when bandits are very close.
    ///
    /// * **greed** (0–1):
    ///   - Low: takes safe, modest-margin trades; diversifies inventory across
    ///     2–3 commodities; content with steady income.
    ///   - High: chases maximum profit margins; goes all-in on single commodity;
    ///     biases toward PROFIT reputation signals over DEMAND.
    ///
    /// * **sociability** (0–1):
    ///   - Low: rarely joins caravans, shares gossip infrequently, prefers
    ///     solo travel.
    ///   - High: eagerly forms/joins caravans, shares price info freely,
    ///     gossip exchange probability scales with this value.
    ///
    /// * **loyalty** (0–1):
    ///   - Low: freely migrates between cities seeking opportunity, no home
    ///     bias in route selection.
    ///   - High: stays near home city, trades primarily with home city,
    ///     returns home to rest rather than nearest city.
    pub fn random(rng: &mut impl Rng) -> Self {
        let normal = Normal::new(0.5, 0.15).unwrap();
        Self {
            risk_tolerance: clamp01(normal.sample(rng)),
            greed: clamp01(normal.sample(rng)),
            sociability: clamp01(normal.sample(rng)),
            loyalty: clamp01(normal.sample(rng)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn random_traits_in_range() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..1000 {
            let t = MerchantTraits::random(&mut rng);
            assert!((0.0..=1.0).contains(&t.risk_tolerance));
            assert!((0.0..=1.0).contains(&t.greed));
            assert!((0.0..=1.0).contains(&t.sociability));
            assert!((0.0..=1.0).contains(&t.loyalty));
        }
    }

    #[test]
    fn random_traits_approximately_centered() {
        let mut rng = StdRng::seed_from_u64(123);
        let n = 5000;
        let mut sum_risk = 0.0f32;
        let mut sum_greed = 0.0f32;
        for _ in 0..n {
            let t = MerchantTraits::random(&mut rng);
            sum_risk += t.risk_tolerance;
            sum_greed += t.greed;
        }
        let mean_risk = sum_risk / n as f32;
        let mean_greed = sum_greed / n as f32;
        assert!((mean_risk - 0.5).abs() < 0.05, "mean risk_tolerance = {mean_risk}");
        assert!((mean_greed - 0.5).abs() < 0.05, "mean greed = {mean_greed}");
    }

    #[test]
    fn deterministic_with_same_seed() {
        let a = MerchantTraits::random(&mut StdRng::seed_from_u64(99));
        let b = MerchantTraits::random(&mut StdRng::seed_from_u64(99));
        assert_eq!(a, b);
    }
}
