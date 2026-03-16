mod common;

use std::time::Instant;

use swarm_economy::config::{ChannelConfig, ReputationChannels, ReputationConfig};
use swarm_economy::types::{ReputationChannel, Vec2};
use swarm_economy::world::reputation::ReputationGrid;

use common::mini_reputation_config;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a ReputationGrid from the mini test config (cell_size=8, 64x64 world).
fn grid() -> ReputationGrid {
    common::make_reputation_grid()
}

/// Convert a world position to its raw_channel flat-array index.
fn cell_index(grid: &ReputationGrid, pos: Vec2) -> usize {
    let col = (pos.x / grid.cell_size()) as usize;
    let row = (pos.y / grid.cell_size()) as usize;
    row * grid.cols() + col
}

/// World position of the center of cell (col, row).
fn cell_center(grid: &ReputationGrid, col: usize, row: usize) -> Vec2 {
    let cs = grid.cell_size();
    Vec2::new((col as f32 + 0.5) * cs, (row as f32 + 0.5) * cs)
}

// ── 1. Deposit value ──────────────────────────────────────────────────────────

#[test]
fn deposit_value_reads_back() {
    let mut g = grid();
    let pos = Vec2::new(20.0, 20.0); // cell (2,2)
    g.deposit(ReputationChannel::Profit, pos, 0.5);

    let idx = cell_index(&g, pos);
    let val = g.raw_channel(ReputationChannel::Profit)[idx];
    assert!(
        (val - 0.5).abs() < 1e-6,
        "expected ~0.5 after deposit, got {val}"
    );
}

// ── 2. Additive stacking clamped at 1.0 ──────────────────────────────────────

#[test]
fn deposit_additive_clamped_at_one() {
    let mut g = grid();
    let pos = Vec2::new(20.0, 20.0);
    g.deposit(ReputationChannel::Profit, pos, 0.8);
    g.deposit(ReputationChannel::Profit, pos, 0.5);

    let idx = cell_index(&g, pos);
    let val = g.raw_channel(ReputationChannel::Profit)[idx];
    assert!(
        (val - 1.0).abs() < 1e-6,
        "expected clamped 1.0, got {val}"
    );
}

// ── 3. Diffusion mass conservation ±1% ──────────────────────────────────────

#[test]
fn diffusion_mass_conservation() {
    // Use a larger grid so boundary effects are minimal.
    let config = ReputationConfig {
        cell_size: 4,
        channels: ReputationChannels {
            profit: ChannelConfig {
                decay: 1.0, // no decay so we isolate diffusion mass
                diffusion_sigma: 0.6,
                color: [255, 255, 255],
            },
            demand: ChannelConfig {
                decay: 1.0,
                diffusion_sigma: 0.6,
                color: [255, 255, 255],
            },
            danger: ChannelConfig {
                decay: 1.0,
                diffusion_sigma: 0.6,
                color: [255, 255, 255],
            },
            opportunity: ChannelConfig {
                decay: 1.0,
                diffusion_sigma: 0.6,
                color: [255, 255, 255],
            },
        },
    };
    let mut g = ReputationGrid::new(&config, 128, 128);

    // Deposit several points in the interior (away from edges).
    g.deposit(ReputationChannel::Profit, Vec2::new(48.0, 48.0), 0.9);
    g.deposit(ReputationChannel::Profit, Vec2::new(60.0, 60.0), 0.7);
    g.deposit(ReputationChannel::Profit, Vec2::new(72.0, 50.0), 0.4);

    let sum_before: f32 = g.raw_channel(ReputationChannel::Profit).iter().sum();
    assert!(sum_before > 0.0, "no signal deposited");

    g.tick();

    let sum_after: f32 = g.raw_channel(ReputationChannel::Profit).iter().sum();

    let relative_error = ((sum_after - sum_before) / sum_before).abs();
    assert!(
        relative_error < 0.01,
        "mass conservation violated: before={sum_before}, after={sum_after}, error={relative_error}"
    );
}

// ── 4. Decay formula ─────────────────────────────────────────────────────────

#[test]
fn decay_reduces_value() {
    let mut g = grid();
    let pos = Vec2::new(32.0, 32.0);
    g.deposit(ReputationChannel::Profit, pos, 1.0);
    let idx = cell_index(&g, pos);

    let before = g.raw_channel(ReputationChannel::Profit)[idx];
    g.tick();
    let after = g.raw_channel(ReputationChannel::Profit)[idx];

    // After tick: diffusion spreads the value around, then decay (0.99) is applied.
    // The center cell gets the Gaussian-weighted sum of neighbours times decay.
    // It must be strictly less than the original.
    assert!(
        after < before,
        "value should decrease after tick: before={before}, after={after}"
    );

    // The decay factor is 0.99 so the value must be less than before * 0.99
    // (diffusion also spreads mass away from center).
    assert!(
        after <= before * 0.99 + 1e-6,
        "after={after} should be <= before*0.99={}", before * 0.99
    );
}

// ── 5. Bilinear interpolation ────────────────────────────────────────────────

#[test]
fn bilinear_sample_at_cell_center_returns_deposited_value() {
    let mut g = grid();
    // Deposit at cell (3,3).
    let deposit_pos = Vec2::new(24.0, 24.0); // cell col=3, row=3
    g.deposit(ReputationChannel::Danger, deposit_pos, 0.7);

    // Sample at the cell center: (3+0.5)*8 = 28.0
    let center = cell_center(&g, 3, 3);
    let val = g.sample(ReputationChannel::Danger, center);
    assert!(
        (val - 0.7).abs() < 1e-4,
        "bilinear sample at cell center should return deposited value, got {val}"
    );
}

#[test]
fn bilinear_interpolation_between_cells() {
    let mut g = grid();
    // Deposit 1.0 in cell (3,3) and 0.0 in cell (4,3).
    g.deposit(ReputationChannel::Profit, Vec2::new(24.0, 24.0), 1.0);

    // Sample halfway between centers: ((3+0.5)*8 + (4+0.5)*8)/2 = (28+36)/2 = 32
    let midpoint = Vec2::new(32.0, 28.0);
    let val = g.sample(ReputationChannel::Profit, midpoint);

    // Should be roughly 0.5 (linear interpolation between 1.0 and 0.0).
    assert!(
        (val - 0.5).abs() < 0.1,
        "bilinear interpolation at midpoint should be ~0.5, got {val}"
    );
}

// ── 6. Channel independence ──────────────────────────────────────────────────

#[test]
fn channel_independence() {
    let mut g = grid();
    g.deposit(ReputationChannel::Profit, Vec2::new(24.0, 24.0), 1.0);

    let danger_sum: f32 = g.raw_channel(ReputationChannel::Danger).iter().sum();
    let demand_sum: f32 = g.raw_channel(ReputationChannel::Demand).iter().sum();
    let opportunity_sum: f32 = g.raw_channel(ReputationChannel::Opportunity).iter().sum();

    assert!(
        danger_sum.abs() < 1e-6,
        "Danger channel should be unaffected, sum={danger_sum}"
    );
    assert!(
        demand_sum.abs() < 1e-6,
        "Demand channel should be unaffected, sum={demand_sum}"
    );
    assert!(
        opportunity_sum.abs() < 1e-6,
        "Opportunity channel should be unaffected, sum={opportunity_sum}"
    );
}

// ── 7. Performance: 1000 ticks on 64x64 grid < 2s ───────────────────────────

#[test]
fn performance_1000_ticks_64x64() {
    let config = mini_reputation_config();
    // 64x64 world with cell_size=8 → 8x8 grid cells.
    // Use a bigger world to get a 64x64 grid: 64*8 = 512.
    let big_config = ReputationConfig {
        cell_size: 8,
        ..config
    };
    let mut g = ReputationGrid::new(&big_config, 512, 512);
    assert_eq!(g.cols(), 64);
    assert_eq!(g.rows(), 64);

    // Seed some data.
    for i in 0..10 {
        let x = (i * 40 + 20) as f32;
        let y = (i * 30 + 20) as f32;
        g.deposit(ReputationChannel::Profit, Vec2::new(x, y), 0.8);
        g.deposit(ReputationChannel::Danger, Vec2::new(x + 10.0, y), 0.6);
    }

    let start = Instant::now();
    for _ in 0..1000 {
        g.tick();
    }
    let elapsed = start.elapsed();

    // In debug builds this runs slower; release builds should easily beat 2s.
    // Allow up to 10s in debug to avoid CI flakiness.
    let limit = if cfg!(debug_assertions) { 10.0 } else { 2.0 };
    assert!(
        elapsed.as_secs_f64() < limit,
        "1000 ticks took {:.3}s, expected < {limit}s",
        elapsed.as_secs_f64()
    );
}
