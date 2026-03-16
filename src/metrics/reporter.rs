/// JSON summary reporter matching the spec output format.

use std::collections::HashMap;

use serde::Serialize;

use crate::agents::merchant::Merchant;
use crate::types::Vec2;
use crate::world::city::City;
use crate::world::road::RoadGrid;

use super::emergence;
use super::tracker::MetricsTracker;

// ── JSON Report Structure ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SimulationReport {
    pub config: ReportConfig,
    pub total_trade_volume: u32,
    pub total_gold_circulation: f32,
    pub gini_coefficient_final: f32,
    pub population_final: u32,
    pub bankruptcies: u32,
    pub robberies: u32,
    pub caravans_formed: u32,
    pub price_convergence_ratio: f32,
    pub num_active_trade_routes: u32,
    pub route_entropy_over_time: Vec<f32>,
    pub specialization_herfindahl_avg: f32,
    pub avg_trade_profit_margin: f32,
    pub profession_distribution_final: HashMap<String, f32>,
    pub emergence_detections: Vec<EmergenceResult>,
}

#[derive(Debug, Serialize)]
pub struct ReportConfig {
    pub ticks: u32,
    pub merchants: u32,
    pub seed: u64,
}

#[derive(Debug, Serialize)]
pub struct EmergenceResult {
    pub name: String,
    pub detected: bool,
    pub metric_value: f32,
    pub threshold: f32,
}

// ── Report Generation ───────────────────────────────────────────────────────

/// Generate a full simulation report from tracker data and world state.
pub fn generate_report(
    tracker: &MetricsTracker,
    merchants: &[Merchant],
    cities: &[City],
    roads: &RoadGrid,
    bandit_positions: &[Vec2],
    season_length_ticks: u32,
    ticks: u32,
    initial_merchants: u32,
    seed: u64,
) -> SimulationReport {
    let snapshots = &tracker.snapshots;

    // Final Gini.
    let gini_final = snapshots
        .last()
        .map(|s| s.gini_coefficient)
        .unwrap_or(0.0);

    // Final population.
    let population_final = merchants.iter().filter(|m| m.alive).count() as u32;

    // Total gold circulation.
    let total_gold = snapshots
        .last()
        .map(|s| s.total_gold)
        .unwrap_or(0.0);

    // Price convergence ratio.
    let price_convergence = emergence::detect_price_convergence(snapshots);

    // Route entropy over time: sample at 3 evenly-spaced points.
    let route_entropy_over_time = if snapshots.len() >= 3 {
        let step = snapshots.len() / 3;
        vec![
            snapshots[step - 1].road_entropy,
            snapshots[2 * step - 1].road_entropy,
            snapshots[snapshots.len() - 1].road_entropy,
        ]
    } else {
        snapshots.iter().map(|s| s.road_entropy).collect()
    };

    // Specialization: average Herfindahl.
    let spec = emergence::detect_market_specialization(cities);

    // Active trade routes: count road cells above a threshold.
    let route_threshold = 0.3;
    let num_active_trade_routes = roads
        .raw_cells()
        .iter()
        .filter(|&&v| v > route_threshold)
        .count() as u32;

    // Average profit margin from trader ledgers.
    let avg_trade_profit_margin = compute_avg_profit_margin(merchants);

    // Final profession distribution.
    let mut profession_distribution_final: HashMap<String, f32> = HashMap::new();
    if let Some(last) = snapshots.last() {
        for (&prof, &frac) in &last.profession_distribution {
            profession_distribution_final.insert(format!("{:?}", prof).to_lowercase(), frac);
        }
    }

    // Run all emergence detectors.
    let detections = emergence::run_all_detectors(
        snapshots,
        merchants,
        cities,
        roads,
        bandit_positions,
        season_length_ticks,
    );

    let emergence_detections: Vec<EmergenceResult> = detections
        .into_iter()
        .map(|d| EmergenceResult {
            name: d.name.to_string(),
            detected: d.detected,
            metric_value: round2(d.metric_value),
            threshold: round2(d.threshold),
        })
        .collect();

    SimulationReport {
        config: ReportConfig {
            ticks,
            merchants: initial_merchants,
            seed,
        },
        total_trade_volume: tracker.cumulative_trades,
        total_gold_circulation: round2(total_gold),
        gini_coefficient_final: round2(gini_final),
        population_final,
        bankruptcies: tracker.cumulative_bankruptcies,
        robberies: tracker.cumulative_robberies,
        caravans_formed: tracker.cumulative_caravans_formed,
        price_convergence_ratio: round2(price_convergence.metric_value),
        num_active_trade_routes,
        route_entropy_over_time: route_entropy_over_time.into_iter().map(round2).collect(),
        specialization_herfindahl_avg: round2(spec.metric_value),
        avg_trade_profit_margin: round2(avg_trade_profit_margin),
        profession_distribution_final,
        emergence_detections,
    }
}

/// Output the report as a JSON string.
pub fn report_to_json(report: &SimulationReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn compute_avg_profit_margin(merchants: &[Merchant]) -> f32 {
    let alive: Vec<&Merchant> = merchants
        .iter()
        .filter(|m| m.alive && !m.ledger.is_empty())
        .collect();

    if alive.is_empty() {
        return 0.0;
    }

    let mut total_margin = 0.0f32;
    let mut count = 0u32;

    for m in &alive {
        for tx in &m.ledger {
            if tx.price > 0.0 {
                // Estimate margin from price vs base commodity price.
                let base = match tx.commodity.tier() {
                    0 => 5.0,
                    1 => 15.0,
                    2 => 35.0,
                    _ => 80.0,
                };
                let margin = (tx.price - base) / base;
                total_margin += margin;
                count += 1;
            }
        }
    }

    if count > 0 {
        total_margin / count as f32
    } else {
        0.0
    }
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round2_works() {
        assert_eq!(round2(0.446), 0.45);
        assert_eq!(round2(0.444), 0.44);
        assert_eq!(round2(1.0), 1.0);
    }
}
