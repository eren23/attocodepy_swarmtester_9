/// Gini coefficient, Lorenz curve, and wealth distribution histogram.

/// Compute the Gini coefficient for a slice of non-negative values.
/// Returns 0.0 for perfect equality, approaching 1.0 for maximal inequality.
/// Returns 0.0 for empty or all-zero inputs.
pub fn gini_coefficient(values: &[f32]) -> f32 {
    let n = values.len();
    if n < 2 {
        return 0.0;
    }

    let mut sorted: Vec<f32> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let total: f32 = sorted.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }

    // Gini = (2 * sum(i * x_i) - (n+1) * sum(x_i)) / (n * sum(x_i))
    let weighted_sum: f64 = sorted
        .iter()
        .enumerate()
        .map(|(i, &x)| (i as f64 + 1.0) * x as f64)
        .sum();
    let n_f = n as f64;
    let total_f = total as f64;

    let gini = (2.0 * weighted_sum - (n_f + 1.0) * total_f) / (n_f * total_f);
    gini.max(0.0) as f32
}

/// Compute the Lorenz curve as a Vec of (cumulative population fraction, cumulative wealth fraction).
/// The first point is (0.0, 0.0) and the last is (1.0, 1.0).
pub fn lorenz_curve(values: &[f32]) -> Vec<(f32, f32)> {
    if values.is_empty() {
        return vec![(0.0, 0.0), (1.0, 1.0)];
    }

    let mut sorted: Vec<f32> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let total: f64 = sorted.iter().map(|&x| x as f64).sum();
    if total <= 0.0 {
        return vec![(0.0, 0.0), (1.0, 1.0)];
    }

    let n = sorted.len() as f64;
    let mut curve = Vec::with_capacity(sorted.len() + 1);
    curve.push((0.0, 0.0));

    let mut cum_wealth = 0.0_f64;
    for (i, &val) in sorted.iter().enumerate() {
        cum_wealth += val as f64;
        let pop_frac = (i as f64 + 1.0) / n;
        let wealth_frac = cum_wealth / total;
        curve.push((pop_frac as f32, wealth_frac as f32));
    }

    curve
}

/// Compute a wealth distribution histogram with the given number of bins.
/// Returns (bin_edges, counts) where bin_edges has len = num_bins + 1.
pub fn wealth_histogram(values: &[f32], num_bins: usize) -> (Vec<f32>, Vec<u32>) {
    if values.is_empty() || num_bins == 0 {
        return (vec![], vec![]);
    }

    let min_val = values
        .iter()
        .copied()
        .fold(f32::INFINITY, f32::min);
    let max_val = values
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);

    // Avoid zero-width bins.
    let range = if (max_val - min_val).abs() < 1e-6 {
        1.0
    } else {
        max_val - min_val
    };

    let bin_width = range / num_bins as f32;
    let mut edges = Vec::with_capacity(num_bins + 1);
    for i in 0..=num_bins {
        edges.push(min_val + bin_width * i as f32);
    }

    let mut counts = vec![0u32; num_bins];
    for &val in values {
        let bin = ((val - min_val) / bin_width) as usize;
        let bin = bin.min(num_bins - 1); // clamp last value into final bin
        counts[bin] += 1;
    }

    (edges, counts)
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gini_perfect_equality() {
        let values = vec![100.0, 100.0, 100.0, 100.0];
        assert!((gini_coefficient(&values)).abs() < 0.01);
    }

    #[test]
    fn gini_perfect_inequality() {
        // One person has everything.
        let values = vec![0.0, 0.0, 0.0, 1000.0];
        assert!(gini_coefficient(&values) > 0.7);
    }

    #[test]
    fn gini_moderate() {
        let values = vec![10.0, 20.0, 30.0, 40.0];
        let g = gini_coefficient(&values);
        assert!(g > 0.1 && g < 0.3, "got {g}");
    }

    #[test]
    fn gini_empty() {
        assert_eq!(gini_coefficient(&[]), 0.0);
    }

    #[test]
    fn gini_single() {
        assert_eq!(gini_coefficient(&[42.0]), 0.0);
    }

    #[test]
    fn lorenz_equal() {
        let curve = lorenz_curve(&[50.0, 50.0, 50.0, 50.0]);
        assert_eq!(curve.first(), Some(&(0.0, 0.0)));
        assert_eq!(curve.last(), Some(&(1.0, 1.0)));
        // For equal distribution, the curve should be close to the diagonal.
        for &(pop, wealth) in &curve {
            assert!((pop - wealth).abs() < 0.02);
        }
    }

    #[test]
    fn lorenz_empty() {
        let curve = lorenz_curve(&[]);
        assert_eq!(curve.len(), 2);
    }

    #[test]
    fn histogram_basic() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let (edges, counts) = wealth_histogram(&values, 4);
        assert_eq!(edges.len(), 5);
        assert_eq!(counts.len(), 4);
        let total: u32 = counts.iter().sum();
        assert_eq!(total, 5);
    }

    #[test]
    fn histogram_all_same() {
        let values = vec![10.0, 10.0, 10.0];
        let (edges, counts) = wealth_histogram(&values, 3);
        assert_eq!(edges.len(), 4);
        let total: u32 = counts.iter().sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn histogram_empty() {
        let (edges, counts) = wealth_histogram(&[], 5);
        assert!(edges.is_empty());
        assert!(counts.is_empty());
    }
}
