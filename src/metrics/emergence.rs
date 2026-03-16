/// 15 emergent behavior detectors per spec.
///
/// Each detector takes relevant data from TickSnapshots / world state and
/// returns a detection result with a metric value and whether the behavior
/// is considered "detected" (above threshold).

use std::collections::HashMap;

use crate::agents::merchant::Merchant;
use crate::types::{Commodity, Profession, Vec2};
use crate::world::city::City;
use crate::world::road::RoadGrid;

use super::inequality;
use super::tracker::TickSnapshot;

// ── Detection result ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Detection {
    pub name: &'static str,
    pub detected: bool,
    pub metric_value: f32,
    pub threshold: f32,
    pub description: &'static str,
}

// ── (1) Trade Route Formation ───────────────────────────────────────────────

/// Detect trade route formation by measuring road grid entropy decrease.
/// Lower entropy = traffic concentrated on fewer paths = routes formed.
pub fn detect_trade_route_formation(snapshots: &[TickSnapshot]) -> Detection {
    let name = "trade_route_formation";
    let threshold = 0.15; // 15% entropy decrease

    if snapshots.len() < 100 {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Road grid entropy decrease (route concentration)",
        };
    }

    let early_window = &snapshots[..snapshots.len().min(100)];
    let late_window = &snapshots[snapshots.len().saturating_sub(100)..];

    let early_entropy: f32 =
        early_window.iter().map(|s| s.road_entropy).sum::<f32>() / early_window.len() as f32;
    let late_entropy: f32 =
        late_window.iter().map(|s| s.road_entropy).sum::<f32>() / late_window.len() as f32;

    let decrease = if early_entropy > 0.0 {
        (early_entropy - late_entropy) / early_entropy
    } else {
        0.0
    };

    Detection {
        name,
        detected: decrease > threshold,
        metric_value: decrease,
        threshold,
        description: "Road grid entropy decrease (route concentration)",
    }
}

// ── (2) Market Specialization ───────────────────────────────────────────────

/// Herfindahl index of per-city craft output (warehouse composition).
/// Higher HHI = more specialization.
pub fn detect_market_specialization(cities: &[City]) -> Detection {
    let name = "market_specialization";
    let threshold = 0.4;

    if cities.is_empty() {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Per-city Herfindahl index of warehouse composition",
        };
    }

    let mut hhi_values = Vec::with_capacity(cities.len());
    for city in cities {
        let total: f32 = city.warehouse.values().sum();
        if total <= 0.0 {
            continue;
        }
        let hhi: f32 = city
            .warehouse
            .values()
            .map(|&qty| {
                let share = qty / total;
                share * share
            })
            .sum();
        hhi_values.push(hhi);
    }

    let avg_hhi = if hhi_values.is_empty() {
        0.0
    } else {
        hhi_values.iter().sum::<f32>() / hhi_values.len() as f32
    };

    let cities_above = hhi_values.iter().filter(|&&h| h > threshold).count();

    Detection {
        name,
        detected: cities_above >= 3,
        metric_value: avg_hhi,
        threshold,
        description: "Per-city Herfindahl index of warehouse composition",
    }
}

// ── (3) Price Convergence ───────────────────────────────────────────────────

/// Cross-city price variance decrease over time.
/// For each commodity, compare variance in early vs late snapshots.
pub fn detect_price_convergence(snapshots: &[TickSnapshot]) -> Detection {
    let name = "price_convergence";
    let threshold = 0.3; // 30% decrease in variance

    if snapshots.len() < 200 {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Cross-city price variance decrease",
        };
    }

    let early = &snapshots[..snapshots.len().min(200)];
    let late = &snapshots[snapshots.len().saturating_sub(200)..];

    let mut convergence_count = 0u32;
    let mut total_ratio = 0.0f32;
    let mut checked = 0u32;

    for commodity in Commodity::ALL {
        let early_var = price_variance_across_snapshots(early, commodity);
        let late_var = price_variance_across_snapshots(late, commodity);

        if early_var > 0.01 {
            let ratio = (early_var - late_var) / early_var;
            total_ratio += ratio;
            checked += 1;
            if ratio > threshold {
                convergence_count += 1;
            }
        }
    }

    let avg_ratio = if checked > 0 {
        total_ratio / checked as f32
    } else {
        0.0
    };

    Detection {
        name,
        detected: convergence_count >= 4,
        metric_value: avg_ratio,
        threshold,
        description: "Cross-city price variance decrease",
    }
}

fn price_variance_across_snapshots(snapshots: &[TickSnapshot], commodity: Commodity) -> f32 {
    let prices: Vec<f32> = snapshots
        .iter()
        .filter_map(|s| s.prices.get(&commodity).copied())
        .collect();
    if prices.len() < 2 {
        return 0.0;
    }
    let mean = prices.iter().sum::<f32>() / prices.len() as f32;
    let var = prices.iter().map(|&p| (p - mean).powi(2)).sum::<f32>() / prices.len() as f32;
    var
}

// ── (4) Boom-Bust Cycles ────────────────────────────────────────────────────

/// Detrended price autocorrelation with Ljung-Box test approximation.
/// Looks for significant periodicity in commodity price series.
pub fn detect_boom_bust_cycles(snapshots: &[TickSnapshot]) -> Detection {
    let name = "boom_bust_cycles";
    let threshold = 0.05; // p < 0.05

    if snapshots.len() < 1000 {
        return Detection {
            name,
            detected: false,
            metric_value: 1.0,
            threshold,
            description: "Detrended price autocorrelation (Ljung-Box p-value)",
        };
    }

    let mut best_p = 1.0f32;

    for commodity in Commodity::ALL {
        let series: Vec<f32> = snapshots
            .iter()
            .filter_map(|s| s.prices.get(&commodity).copied())
            .collect();

        if series.len() < 500 {
            continue;
        }

        // Detrend: remove linear trend.
        let detrended = detrend(&series);

        // Compute autocorrelation at lags 500-3000 (or up to series length).
        let max_lag = (detrended.len() / 2).min(3000);
        let min_lag = 500.min(max_lag);

        let p = ljung_box_p_value(&detrended, min_lag, max_lag);
        if p < best_p {
            best_p = p;
        }
    }

    Detection {
        name,
        detected: best_p < threshold,
        metric_value: best_p,
        threshold,
        description: "Detrended price autocorrelation (Ljung-Box p-value)",
    }
}

/// Remove linear trend from a time series.
fn detrend(series: &[f32]) -> Vec<f32> {
    let n = series.len() as f64;
    if n < 2.0 {
        return series.to_vec();
    }

    // Linear regression: y = a + b*x
    let x_mean = (n - 1.0) / 2.0;
    let y_mean: f64 = series.iter().map(|&v| v as f64).sum::<f64>() / n;

    let mut num = 0.0_f64;
    let mut den = 0.0_f64;
    for (i, &y) in series.iter().enumerate() {
        let x = i as f64;
        num += (x - x_mean) * (y as f64 - y_mean);
        den += (x - x_mean).powi(2);
    }

    let b = if den.abs() > 1e-12 { num / den } else { 0.0 };
    let a = y_mean - b * x_mean;

    series
        .iter()
        .enumerate()
        .map(|(i, &y)| (y as f64 - a - b * i as f64) as f32)
        .collect()
}

/// Approximate Ljung-Box test p-value for autocorrelation at given lag range.
/// Returns a rough p-value based on the Q statistic compared to chi-squared.
fn ljung_box_p_value(series: &[f32], min_lag: usize, max_lag: usize) -> f32 {
    let n = series.len();
    if n < max_lag + 1 {
        return 1.0;
    }

    let mean: f64 = series.iter().map(|&v| v as f64).sum::<f64>() / n as f64;
    let var: f64 = series
        .iter()
        .map(|&v| {
            let d = v as f64 - mean;
            d * d
        })
        .sum::<f64>()
        / n as f64;

    if var < 1e-12 {
        return 1.0;
    }

    let mut q = 0.0_f64;
    let mut num_lags = 0usize;

    for lag in min_lag..=max_lag.min(n - 1) {
        let mut autocorr = 0.0_f64;
        for i in 0..(n - lag) {
            autocorr += (series[i] as f64 - mean) * (series[i + lag] as f64 - mean);
        }
        autocorr /= n as f64 * var;

        q += autocorr * autocorr / (n - lag) as f64;
        num_lags += 1;
    }

    q *= n as f64 * (n as f64 + 2.0);

    if num_lags == 0 {
        return 1.0;
    }

    // Approximate p-value using chi-squared survival function.
    // For large degrees of freedom, use normal approximation:
    // Z = (Q - df) / sqrt(2 * df)
    let df = num_lags as f64;
    let z = (q - df) / (2.0 * df).sqrt();

    // Approximate survival function of standard normal.
    let p = 0.5 * erfc(z / std::f64::consts::SQRT_2);
    p as f32
}

/// Complementary error function approximation (Abramowitz & Stegun 7.1.26).
fn erfc(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let poly = t
        * (0.254829592
            + t * (-0.284496736
                + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let result = poly * (-x * x).exp();
    if x >= 0.0 {
        result
    } else {
        2.0 - result
    }
}

// ── (5) Seasonal Price Waves ────────────────────────────────────────────────

/// Seasonal decomposition: measure amplitude of seasonal component in prices.
/// Specifically checks GRAIN price seasonality.
pub fn detect_seasonal_price_waves(
    snapshots: &[TickSnapshot],
    season_length_ticks: u32,
) -> Detection {
    let name = "seasonal_price_waves";
    let threshold = 0.15; // 15% of mean price

    if snapshots.len() < (season_length_ticks * 4) as usize {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Seasonal component amplitude in GRAIN price",
        };
    }

    let series: Vec<f32> = snapshots
        .iter()
        .filter_map(|s| s.prices.get(&Commodity::Grain).copied())
        .collect();

    if series.is_empty() {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Seasonal component amplitude in GRAIN price",
        };
    }

    let mean = series.iter().sum::<f32>() / series.len() as f32;
    if mean <= 0.0 {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Seasonal component amplitude in GRAIN price",
        };
    }

    // Simple seasonal decomposition: compute average by season phase.
    let period = (season_length_ticks * 4) as usize; // full year
    if period == 0 {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Seasonal component amplitude in GRAIN price",
        };
    }

    // Group prices by their position within the year cycle.
    let num_phases = 4; // 4 seasons
    let phase_length = season_length_ticks as usize;
    let mut phase_sums = vec![0.0f32; num_phases];
    let mut phase_counts = vec![0u32; num_phases];

    for (i, &price) in series.iter().enumerate() {
        let phase = (i / phase_length.max(1)) % num_phases;
        phase_sums[phase] += price;
        phase_counts[phase] += 1;
    }

    let phase_avgs: Vec<f32> = phase_sums
        .iter()
        .zip(phase_counts.iter())
        .map(|(&s, &c)| if c > 0 { s / c as f32 } else { mean })
        .collect();

    let max_phase = phase_avgs
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    let min_phase = phase_avgs
        .iter()
        .copied()
        .fold(f32::INFINITY, f32::min);

    let amplitude = (max_phase - min_phase) / mean;

    Detection {
        name,
        detected: amplitude > threshold,
        metric_value: amplitude,
        threshold,
        description: "Seasonal component amplitude in GRAIN price",
    }
}

// ── (6) Wealth Inequality ───────────────────────────────────────────────────

/// Gini coefficient exceeds threshold after sufficient ticks.
pub fn detect_wealth_inequality(snapshots: &[TickSnapshot]) -> Detection {
    let name = "wealth_inequality";
    let threshold = 0.35;

    let gini = snapshots
        .last()
        .map(|s| s.gini_coefficient)
        .unwrap_or(0.0);

    let enough_ticks = snapshots
        .last()
        .map(|s| s.tick >= 5000)
        .unwrap_or(false);

    Detection {
        name,
        detected: enough_ticks && gini > threshold,
        metric_value: gini,
        threshold,
        description: "Gini coefficient of merchant gold",
    }
}

// ── (7) Guild Clustering ────────────────────────────────────────────────────

/// DBSCAN-like spatial clustering per profession.
/// Detected if >= 2 professions produce <= 4 clusters.
pub fn detect_guild_clustering(merchants: &[Merchant]) -> Detection {
    let name = "guild_clustering";
    let threshold = 4.0; // max clusters per profession
    let eps = 80.0; // clustering radius
    let min_pts = 3; // minimum cluster size

    let alive: Vec<&Merchant> = merchants.iter().filter(|m| m.alive).collect();

    let mut professions_with_clusters = 0u32;

    for prof in Profession::ALL {
        let positions: Vec<Vec2> = alive
            .iter()
            .filter(|m| m.profession == prof)
            .map(|m| m.pos)
            .collect();

        if positions.len() < min_pts {
            continue;
        }

        let num_clusters = dbscan_count(&positions, eps, min_pts);
        if num_clusters > 0 && num_clusters <= threshold as usize {
            professions_with_clusters += 1;
        }
    }

    Detection {
        name,
        detected: professions_with_clusters >= 2,
        metric_value: professions_with_clusters as f32,
        threshold,
        description: "Professions with DBSCAN clustering <= 4 clusters",
    }
}

/// Simple DBSCAN implementation returning the number of clusters.
fn dbscan_count(points: &[Vec2], eps: f32, min_pts: usize) -> usize {
    let n = points.len();
    if n == 0 {
        return 0;
    }

    let mut labels = vec![-1i32; n]; // -1 = unvisited
    let mut cluster_id = 0i32;

    for i in 0..n {
        if labels[i] != -1 {
            continue;
        }

        let neighbors = range_query(points, i, eps);
        if neighbors.len() < min_pts {
            labels[i] = 0; // noise
            continue;
        }

        cluster_id += 1;
        labels[i] = cluster_id;
        let mut queue = neighbors;
        let mut qi = 0;

        while qi < queue.len() {
            let j = queue[qi];
            qi += 1;

            if labels[j] == 0 {
                labels[j] = cluster_id; // noise → border
            }
            if labels[j] != -1 {
                continue;
            }

            labels[j] = cluster_id;
            let j_neighbors = range_query(points, j, eps);
            if j_neighbors.len() >= min_pts {
                for &k in &j_neighbors {
                    if !queue.contains(&k) {
                        queue.push(k);
                    }
                }
            }
        }
    }

    cluster_id as usize
}

fn range_query(points: &[Vec2], idx: usize, eps: f32) -> Vec<usize> {
    let p = points[idx];
    let eps_sq = eps * eps;
    (0..points.len())
        .filter(|&i| (points[i] - p).length_squared() <= eps_sq)
        .collect()
}

// ── (8) Caravan-Danger Correlation ──────────────────────────────────────────

/// Pearson r between caravan count and robbery count over time.
pub fn detect_caravan_danger_correlation(snapshots: &[TickSnapshot]) -> Detection {
    let name = "caravan_danger_correlation";
    let threshold = 0.3;

    if snapshots.len() < 100 {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Pearson r between caravan count and robbery rate",
        };
    }

    let x: Vec<f32> = snapshots.iter().map(|s| s.robbery_count as f32).collect();
    let y: Vec<f32> = snapshots.iter().map(|s| s.caravan_count as f32).collect();

    let r = pearson_r(&x, &y);

    Detection {
        name,
        detected: r > threshold,
        metric_value: r,
        threshold,
        description: "Pearson r between caravan count and robbery rate",
    }
}

/// Pearson correlation coefficient.
fn pearson_r(x: &[f32], y: &[f32]) -> f32 {
    let n = x.len().min(y.len());
    if n < 2 {
        return 0.0;
    }

    let x_mean = x[..n].iter().sum::<f32>() / n as f32;
    let y_mean = y[..n].iter().sum::<f32>() / n as f32;

    let mut cov = 0.0_f64;
    let mut var_x = 0.0_f64;
    let mut var_y = 0.0_f64;

    for i in 0..n {
        let dx = x[i] as f64 - x_mean as f64;
        let dy = y[i] as f64 - y_mean as f64;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }

    let denom = (var_x * var_y).sqrt();
    if denom < 1e-12 {
        return 0.0;
    }

    (cov / denom) as f32
}

// ── (9) Information Propagation ─────────────────────────────────────────────

/// Price-knowledge wavefront speed: measure how quickly prices propagate.
/// We detect this by looking at how fast cross-city price correlation increases.
pub fn detect_information_propagation(snapshots: &[TickSnapshot]) -> Detection {
    let name = "information_propagation";
    let threshold = 0.0; // any measurable speed

    if snapshots.len() < 200 {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Price-knowledge wavefront speed",
        };
    }

    // Measure price variance decrease rate: faster convergence = faster info propagation.
    let early = &snapshots[..100.min(snapshots.len())];
    let late = &snapshots[snapshots.len().saturating_sub(100)..];

    let mut total_speed = 0.0f32;
    let mut count = 0u32;

    for commodity in Commodity::ALL {
        let early_var = price_variance_across_snapshots(early, commodity);
        let late_var = price_variance_across_snapshots(late, commodity);

        if early_var > 0.01 {
            let speed = (early_var - late_var) / early_var;
            total_speed += speed;
            count += 1;
        }
    }

    let avg_speed = if count > 0 {
        total_speed / count as f32
    } else {
        0.0
    };

    Detection {
        name,
        detected: avg_speed > threshold,
        metric_value: avg_speed,
        threshold,
        description: "Price-knowledge wavefront speed",
    }
}

// ── (10) Economic Migration ─────────────────────────────────────────────────

/// Density shift toward cities with new resources.
/// Measured by increasing population Gini across cities.
pub fn detect_economic_migration(snapshots: &[TickSnapshot]) -> Detection {
    let name = "economic_migration";
    let threshold = 0.1; // 10% increase in population Gini

    if snapshots.len() < 200 {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Population density shift (Gini increase)",
        };
    }

    let early = &snapshots[..100.min(snapshots.len())];
    let late = &snapshots[snapshots.len().saturating_sub(100)..];

    let early_gini = avg_population_gini(early);
    let late_gini = avg_population_gini(late);

    let shift = late_gini - early_gini;

    Detection {
        name,
        detected: shift > threshold,
        metric_value: shift,
        threshold,
        description: "Population density shift (Gini increase)",
    }
}

fn avg_population_gini(snapshots: &[TickSnapshot]) -> f32 {
    if snapshots.is_empty() {
        return 0.0;
    }
    let sum: f32 = snapshots
        .iter()
        .map(|s| inequality::gini_coefficient(&s.city_populations))
        .sum();
    sum / snapshots.len() as f32
}

// ── (11) Profession Adaptation ──────────────────────────────────────────────

/// Distribution shift on resource change: compare early vs late profession distributions.
pub fn detect_profession_adaptation(snapshots: &[TickSnapshot]) -> Detection {
    let name = "profession_adaptation";
    let threshold = 0.05; // 5% distribution shift

    if snapshots.len() < 200 {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Profession distribution shift over time",
        };
    }

    let early = &snapshots[..100.min(snapshots.len())];
    let late = &snapshots[snapshots.len().saturating_sub(100)..];

    let early_dist = avg_profession_distribution(early);
    let late_dist = avg_profession_distribution(late);

    // Total variation distance.
    let mut tv = 0.0f32;
    for prof in Profession::ALL {
        let e = early_dist.get(&prof).copied().unwrap_or(0.0);
        let l = late_dist.get(&prof).copied().unwrap_or(0.0);
        tv += (e - l).abs();
    }
    tv /= 2.0; // TV distance is half the L1 norm

    Detection {
        name,
        detected: tv > threshold,
        metric_value: tv,
        threshold,
        description: "Profession distribution shift over time",
    }
}

fn avg_profession_distribution(
    snapshots: &[TickSnapshot],
) -> HashMap<Profession, f32> {
    let mut sums: HashMap<Profession, f32> = HashMap::new();
    let count = snapshots.len() as f32;
    if count == 0.0 {
        return sums;
    }
    for s in snapshots {
        for (&prof, &frac) in &s.profession_distribution {
            *sums.entry(prof).or_insert(0.0) += frac;
        }
    }
    for val in sums.values_mut() {
        *val /= count;
    }
    sums
}

// ── (12) City Growth/Decline ────────────────────────────────────────────────

/// Population variance increase across cities over time.
pub fn detect_city_growth_decline(snapshots: &[TickSnapshot]) -> Detection {
    let name = "city_growth_decline";
    let threshold = 0.1; // 10% variance increase

    if snapshots.len() < 200 {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Population variance increase across cities",
        };
    }

    let early = &snapshots[..100.min(snapshots.len())];
    let late = &snapshots[snapshots.len().saturating_sub(100)..];

    let early_var = avg_population_variance(early);
    let late_var = avg_population_variance(late);

    let increase = if early_var > 0.01 {
        (late_var - early_var) / early_var
    } else if late_var > 0.01 {
        1.0
    } else {
        0.0
    };

    Detection {
        name,
        detected: increase > threshold,
        metric_value: increase,
        threshold,
        description: "Population variance increase across cities",
    }
}

fn avg_population_variance(snapshots: &[TickSnapshot]) -> f32 {
    if snapshots.is_empty() {
        return 0.0;
    }
    let sum: f32 = snapshots.iter().map(|s| variance(&s.city_populations)).sum();
    sum / snapshots.len() as f32
}

fn variance(values: &[f32]) -> f32 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    values.iter().map(|&v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32
}

// ── (13) Bandit Avoidance ───────────────────────────────────────────────────

/// Traffic drop near bandit camps.
/// Measured by comparing road values near camp positions vs overall average.
pub fn detect_bandit_avoidance(
    roads: &RoadGrid,
    bandit_positions: &[Vec2],
) -> Detection {
    let name = "bandit_avoidance";
    let threshold = 0.2; // 20% less traffic near bandits

    if bandit_positions.is_empty() {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Traffic reduction near bandit camps",
        };
    }

    let cells = roads.raw_cells();
    let total: f32 = cells.iter().sum();
    let total_cells = cells.len();
    let avg_road = if total_cells > 0 {
        total / total_cells as f32
    } else {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Traffic reduction near bandit camps",
        };
    };

    if avg_road < 1e-6 {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Traffic reduction near bandit camps",
        };
    }

    // Sample road values near bandit positions.
    let check_radius = 100.0;
    let cell_size = roads.cell_size();
    let cols = roads.cols();
    let rows = roads.rows();

    let mut near_sum = 0.0f32;
    let mut near_count = 0u32;

    for &bpos in bandit_positions {
        let col_min = ((bpos.x - check_radius).max(0.0) / cell_size) as usize;
        let col_max = (((bpos.x + check_radius) / cell_size) as usize).min(cols.saturating_sub(1));
        let row_min = ((bpos.y - check_radius).max(0.0) / cell_size) as usize;
        let row_max = (((bpos.y + check_radius) / cell_size) as usize).min(rows.saturating_sub(1));

        for r in row_min..=row_max {
            for c in col_min..=col_max {
                let cx = (c as f32 + 0.5) * cell_size;
                let cy = (r as f32 + 0.5) * cell_size;
                let dist = ((cx - bpos.x).powi(2) + (cy - bpos.y).powi(2)).sqrt();
                if dist <= check_radius {
                    near_sum += cells[r * cols + c];
                    near_count += 1;
                }
            }
        }
    }

    let near_avg = if near_count > 0 {
        near_sum / near_count as f32
    } else {
        avg_road
    };

    let reduction = (avg_road - near_avg) / avg_road;

    Detection {
        name,
        detected: reduction > threshold,
        metric_value: reduction,
        threshold,
        description: "Traffic reduction near bandit camps",
    }
}

// ── (14) Tax Competition ────────────────────────────────────────────────────

/// Negative correlation between tax rate and trade volume across cities.
pub fn detect_tax_competition(cities: &[City]) -> Detection {
    let name = "tax_competition";
    let threshold = -0.3; // negative correlation

    if cities.len() < 3 {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Tax-volume negative correlation across cities",
        };
    }

    let taxes: Vec<f32> = cities.iter().map(|c| c.tax_rate).collect();
    let volumes: Vec<f32> = cities.iter().map(|c| c.trade_volume).collect();

    let r = pearson_r(&taxes, &volumes);

    Detection {
        name,
        detected: r < threshold,
        metric_value: r,
        threshold,
        description: "Tax-volume negative correlation across cities",
    }
}

// ── (15) Supply Chain Depth ─────────────────────────────────────────────────

/// Average commodity touch count (distinct agents handling a commodity).
/// Approximated by counting average number of transactions per commodity.
pub fn detect_supply_chain_depth(merchants: &[Merchant]) -> Detection {
    let name = "supply_chain_depth";
    let threshold = 2.0;

    let alive: Vec<&Merchant> = merchants.iter().filter(|m| m.alive).collect();
    if alive.is_empty() {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Avg commodity touch count > 2.0",
        };
    }

    // Count distinct agents that have handled each commodity (via ledger).
    let mut commodity_agents: HashMap<Commodity, std::collections::HashSet<u32>> = HashMap::new();
    for m in &alive {
        for tx in &m.ledger {
            commodity_agents
                .entry(tx.commodity)
                .or_default()
                .insert(tx.buyer_id);
            commodity_agents
                .entry(tx.commodity)
                .or_default()
                .insert(tx.seller_id);
        }
    }

    if commodity_agents.is_empty() {
        return Detection {
            name,
            detected: false,
            metric_value: 0.0,
            threshold,
            description: "Avg commodity touch count > 2.0",
        };
    }

    let avg_touch: f32 = commodity_agents
        .values()
        .map(|agents| agents.len() as f32)
        .sum::<f32>()
        / commodity_agents.len() as f32;

    Detection {
        name,
        detected: avg_touch > threshold,
        metric_value: avg_touch,
        threshold,
        description: "Avg commodity touch count > 2.0",
    }
}

// ── Run All Detectors ───────────────────────────────────────────────────────

/// Run all 15 emergence detectors and return results.
pub fn run_all_detectors(
    snapshots: &[TickSnapshot],
    merchants: &[Merchant],
    cities: &[City],
    roads: &RoadGrid,
    bandit_positions: &[Vec2],
    season_length_ticks: u32,
) -> Vec<Detection> {
    vec![
        detect_trade_route_formation(snapshots),
        detect_market_specialization(cities),
        detect_price_convergence(snapshots),
        detect_boom_bust_cycles(snapshots),
        detect_seasonal_price_waves(snapshots, season_length_ticks),
        detect_wealth_inequality(snapshots),
        detect_guild_clustering(merchants),
        detect_caravan_danger_correlation(snapshots),
        detect_information_propagation(snapshots),
        detect_economic_migration(snapshots),
        detect_profession_adaptation(snapshots),
        detect_city_growth_decline(snapshots),
        detect_bandit_avoidance(roads, bandit_positions),
        detect_tax_competition(cities),
        detect_supply_chain_depth(merchants),
    ]
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detrend_removes_linear() {
        let series: Vec<f32> = (0..100).map(|i| i as f32 * 2.0 + 10.0).collect();
        let detrended = detrend(&series);
        let mean: f32 = detrended.iter().sum::<f32>() / detrended.len() as f32;
        assert!(
            mean.abs() < 0.1,
            "detrended mean should be ~0, got {mean}"
        );
    }

    #[test]
    fn pearson_r_perfect_positive() {
        let x: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let y: Vec<f32> = (0..100).map(|i| i as f32 * 2.0 + 1.0).collect();
        let r = pearson_r(&x, &y);
        assert!((r - 1.0).abs() < 0.01, "expected r~1.0, got {r}");
    }

    #[test]
    fn pearson_r_no_correlation() {
        let x: Vec<f32> = (0..1000).map(|i| (i as f32 * 7.13).sin()).collect();
        let y: Vec<f32> = (0..1000).map(|i| (i as f32 * 3.71).cos()).collect();
        let r = pearson_r(&x, &y);
        assert!(r.abs() < 0.2, "expected r~0, got {r}");
    }

    #[test]
    fn dbscan_finds_clusters() {
        let mut points = Vec::new();
        // Cluster 1: around (0, 0)
        for i in 0..10 {
            points.push(Vec2::new(i as f32, 0.0));
        }
        // Cluster 2: around (200, 200)
        for i in 0..10 {
            points.push(Vec2::new(200.0 + i as f32, 200.0));
        }
        let n = dbscan_count(&points, 50.0, 3);
        assert_eq!(n, 2);
    }

    #[test]
    fn dbscan_all_noise() {
        let points = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1000.0, 1000.0),
        ];
        let n = dbscan_count(&points, 10.0, 3);
        assert_eq!(n, 0);
    }

    #[test]
    fn erfc_values() {
        // erfc(0) = 1.0
        assert!((erfc(0.0) - 1.0).abs() < 0.01);
        // erfc(large) ≈ 0
        assert!(erfc(5.0) < 0.01);
    }

    #[test]
    fn variance_computation() {
        let vals = vec![2.0, 4.0, 6.0, 8.0];
        let v = variance(&vals);
        // Mean = 5.0, var = (9+1+1+9)/4 = 5.0
        assert!((v - 5.0).abs() < 0.01, "got {v}");
    }
}
