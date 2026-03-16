use std::collections::HashMap;

use crate::agents::merchant::Merchant;
use crate::market::order_book::OrderBook;
use crate::types::{Commodity, Profession, Season};
use crate::world::city::City;
use crate::world::road::RoadGrid;

use super::inequality;

// ── Snapshot ────────────────────────────────────────────────────────────────

/// Full metrics snapshot for a single tick.
#[derive(Debug, Clone)]
pub struct TickSnapshot {
    pub tick: u32,
    pub season: Season,

    // Aggregate
    pub total_gold: f32,
    pub trade_volume: f32,
    pub alive_merchants: u32,
    pub caravan_count: u32,
    pub robbery_count: u32,
    pub bankruptcy_count: u32,

    // Per-commodity prices (avg across all cities that traded).
    pub prices: HashMap<Commodity, f32>,

    // Per-city populations.
    pub city_populations: Vec<f32>,

    // Profession distribution (fraction of alive merchants).
    pub profession_distribution: HashMap<Profession, f32>,

    // Road grid entropy (Shannon entropy of road cell values).
    pub road_entropy: f32,

    // Gini coefficient of merchant gold.
    pub gini_coefficient: f32,
}

// ── MetricsTracker ──────────────────────────────────────────────────────────

/// Records time-series data per tick for analysis and reporting.
pub struct MetricsTracker {
    pub snapshots: Vec<TickSnapshot>,
    /// Running total of trades across all ticks.
    pub cumulative_trades: u32,
    /// Running total of robberies across all ticks.
    pub cumulative_robberies: u32,
    /// Running total of bankruptcies across all ticks.
    pub cumulative_bankruptcies: u32,
    /// Running total of caravans formed across all ticks.
    pub cumulative_caravans_formed: u32,

    max_snapshots: usize,
}

impl MetricsTracker {
    pub fn new(max_snapshots: usize) -> Self {
        Self {
            snapshots: Vec::new(),
            cumulative_trades: 0,
            cumulative_robberies: 0,
            cumulative_bankruptcies: 0,
            cumulative_caravans_formed: 0,
            max_snapshots,
        }
    }

    /// Record a snapshot for the current tick.
    ///
    /// `trades_this_tick`, `robberies_this_tick`, `bankruptcies_this_tick`, and
    /// `caravans_formed_this_tick` are incremental counts for this tick only.
    pub fn record(
        &mut self,
        tick: u32,
        season: Season,
        merchants: &[Merchant],
        cities: &[City],
        order_books: &[OrderBook],
        roads: &RoadGrid,
        caravan_count: u32,
        trades_this_tick: u32,
        robberies_this_tick: u32,
        bankruptcies_this_tick: u32,
        caravans_formed_this_tick: u32,
    ) {
        self.cumulative_trades += trades_this_tick;
        self.cumulative_robberies += robberies_this_tick;
        self.cumulative_bankruptcies += bankruptcies_this_tick;
        self.cumulative_caravans_formed += caravans_formed_this_tick;

        let alive: Vec<&Merchant> = merchants.iter().filter(|m| m.alive).collect();
        let alive_count = alive.len() as u32;

        // Total gold.
        let total_gold: f32 = alive.iter().map(|m| m.gold).sum();

        // Trade volume: sum of all city trade_volumes.
        let trade_volume: f32 = cities.iter().map(|c| c.trade_volume).sum();

        // Per-commodity avg prices across cities.
        let prices = compute_avg_prices(order_books);

        // City populations.
        let city_populations: Vec<f32> = cities.iter().map(|c| c.population).collect();

        // Profession distribution.
        let profession_distribution = compute_profession_distribution(&alive);

        // Road entropy.
        let road_entropy = compute_road_entropy(roads);

        // Gini coefficient.
        let gold_values: Vec<f32> = alive.iter().map(|m| m.gold.max(0.0)).collect();
        let gini_coefficient = inequality::gini_coefficient(&gold_values);

        let snapshot = TickSnapshot {
            tick,
            season,
            total_gold,
            trade_volume,
            alive_merchants: alive_count,
            caravan_count,
            robbery_count: robberies_this_tick,
            bankruptcy_count: bankruptcies_this_tick,
            prices,
            city_populations,
            profession_distribution,
            road_entropy,
            gini_coefficient,
        };

        self.snapshots.push(snapshot);

        if self.snapshots.len() > self.max_snapshots {
            self.snapshots.remove(0);
        }
    }

    /// Get the latest snapshot, if any.
    pub fn latest(&self) -> Option<&TickSnapshot> {
        self.snapshots.last()
    }

    /// Price time series for a specific commodity across all recorded ticks.
    pub fn price_series(&self, commodity: Commodity) -> Vec<(u32, f32)> {
        self.snapshots
            .iter()
            .filter_map(|s| s.prices.get(&commodity).map(|&p| (s.tick, p)))
            .collect()
    }

    /// Road entropy time series.
    pub fn road_entropy_series(&self) -> Vec<(u32, f32)> {
        self.snapshots
            .iter()
            .map(|s| (s.tick, s.road_entropy))
            .collect()
    }

    /// Gini coefficient time series.
    pub fn gini_series(&self) -> Vec<(u32, f32)> {
        self.snapshots
            .iter()
            .map(|s| (s.tick, s.gini_coefficient))
            .collect()
    }

    /// Population series for a specific city index.
    pub fn city_population_series(&self, city_idx: usize) -> Vec<(u32, f32)> {
        self.snapshots
            .iter()
            .filter_map(|s| {
                s.city_populations
                    .get(city_idx)
                    .map(|&p| (s.tick, p))
            })
            .collect()
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Compute average last-known price per commodity across all city order books.
fn compute_avg_prices(order_books: &[OrderBook]) -> HashMap<Commodity, f32> {
    let mut sums: HashMap<Commodity, (f32, u32)> = HashMap::new();
    for book in order_books {
        for commodity in Commodity::ALL {
            if let Some(price) = book.last_price(commodity) {
                let entry = sums.entry(commodity).or_insert((0.0, 0));
                entry.0 += price;
                entry.1 += 1;
            }
        }
    }
    sums.into_iter()
        .map(|(c, (sum, count))| (c, sum / count as f32))
        .collect()
}

/// Compute profession distribution as fractions.
fn compute_profession_distribution(alive: &[&Merchant]) -> HashMap<Profession, f32> {
    let total = alive.len().max(1) as f32;
    let mut counts: HashMap<Profession, u32> = HashMap::new();
    for m in alive {
        *counts.entry(m.profession).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(p, c)| (p, c as f32 / total))
        .collect()
}

/// Shannon entropy of the road grid cell values, treating non-zero cells
/// as a probability distribution.
///
/// Higher entropy = more uniform road usage (no clear routes).
/// Lower entropy = traffic concentrated on fewer cells (trade routes formed).
fn compute_road_entropy(roads: &RoadGrid) -> f32 {
    let cells = roads.raw_cells();
    let total: f64 = cells.iter().map(|&v| v as f64).sum();
    if total <= 0.0 {
        return 0.0;
    }

    let mut entropy = 0.0_f64;
    for &val in cells {
        if val > 0.0 {
            let p = val as f64 / total;
            entropy -= p * p.ln();
        }
    }

    entropy as f32
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn road_entropy_uniform_is_high() {
        // Uniform distribution should have high entropy.
        let cells = vec![1.0; 100];
        let total: f64 = cells.iter().map(|&v| v as f64).sum();
        let mut entropy = 0.0_f64;
        for &val in &cells {
            if val > 0.0 {
                let p = val as f64 / total;
                entropy -= p * p.ln();
            }
        }
        assert!(entropy > 4.0, "uniform entropy should be high, got {entropy}");
    }

    #[test]
    fn road_entropy_concentrated_is_low() {
        // All traffic in one cell.
        let mut cells = vec![0.0; 100];
        cells[0] = 1.0;
        let total: f64 = cells.iter().map(|&v| v as f64).sum();
        let mut entropy = 0.0_f64;
        for &val in &cells {
            if val > 0.0 {
                let p = val as f64 / total;
                entropy -= p * p.ln();
            }
        }
        assert!(
            entropy.abs() < 0.01,
            "concentrated entropy should be ~0, got {entropy}"
        );
    }

    #[test]
    fn road_entropy_empty_is_zero() {
        assert_eq!(compute_road_entropy_from_cells(&[0.0; 50]), 0.0);
    }

    fn compute_road_entropy_from_cells(cells: &[f32]) -> f32 {
        let total: f64 = cells.iter().map(|&v| v as f64).sum();
        if total <= 0.0 {
            return 0.0;
        }
        let mut entropy = 0.0_f64;
        for &val in cells {
            if val > 0.0 {
                let p = val as f64 / total;
                entropy -= p * p.ln();
            }
        }
        entropy as f32
    }
}
