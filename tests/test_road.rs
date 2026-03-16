mod common;

use swarm_economy::types::Vec2;
use swarm_economy::world::road::RoadGrid;

use common::*;

// ── Traversal increment ────────────────────────────────────────────────────

#[test]
fn traverse_increments_by_0_002() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    assert!(
        grid.road_value(pos).abs() < 1e-6,
        "initial road value should be 0"
    );

    grid.traverse(pos);

    let val = grid.road_value(pos);
    assert!(
        (val - 0.002).abs() < 1e-6,
        "after one traverse, road_value should be 0.002, got {val}"
    );
}

#[test]
fn traverse_accumulates_linearly() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    for _ in 0..50 {
        grid.traverse(pos);
    }

    let val = grid.road_value(pos);
    let expected = 50.0 * 0.002;
    assert!(
        (val - expected).abs() < 1e-5,
        "after 50 traversals, expected {expected}, got {val}"
    );
}

#[test]
fn traverse_100_times_gives_0_2() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(24.0, 24.0);

    for _ in 0..100 {
        grid.traverse(pos);
    }

    let val = grid.road_value(pos);
    assert!(
        (val - 0.2).abs() < 1e-5,
        "after 100 traversals expected 0.2, got {val}"
    );
}

// ── Clamp at 1.0 ──────────────────────────────────────────────────────────

#[test]
fn road_value_clamps_at_1() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    // 500 traversals = 1.0, then 500 more should not exceed 1.0
    for _ in 0..1000 {
        grid.traverse(pos);
    }

    let val = grid.road_value(pos);
    assert!(
        (val - 1.0).abs() < 1e-6,
        "road_value should clamp at 1.0, got {val}"
    );
}

#[test]
fn road_value_at_500_traversals_is_near_1() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    for _ in 0..500 {
        grid.traverse(pos);
    }

    // 500 * 0.002 = 1.0 in exact math, but f32 accumulation may differ slightly.
    let val = grid.road_value(pos);
    assert!(
        (val - 1.0).abs() < 1e-3,
        "500 traversals should yield ~1.0, got {val}"
    );
}

#[test]
fn road_value_after_501_traversals_still_clamped() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    for _ in 0..501 {
        grid.traverse(pos);
    }

    let val = grid.road_value(pos);
    assert!(
        val <= 1.0 + 1e-6,
        "road_value should never exceed 1.0, got {val}"
    );
}

// ── Decay rate ─────────────────────────────────────────────────────────────

#[test]
fn decay_multiplies_by_0_9998() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    grid.traverse(pos);
    let before = grid.road_value(pos);
    assert!((before - 0.002).abs() < 1e-6);

    grid.tick();

    let after = grid.road_value(pos);
    let expected = 0.002 * 0.9998;
    assert!(
        (after - expected).abs() < 1e-8,
        "after one decay tick, expected {expected}, got {after}"
    );
}

#[test]
fn decay_multiple_ticks() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    // Build up some road value
    for _ in 0..100 {
        grid.traverse(pos);
    }

    let initial = grid.road_value(pos);
    assert!((initial - 0.2).abs() < 1e-5);

    // Decay for 100 ticks
    for _ in 0..100 {
        grid.tick();
    }

    let decayed = grid.road_value(pos);
    let expected = 0.2 * 0.9998_f32.powi(100);
    assert!(
        (decayed - expected).abs() < 1e-5,
        "after 100 decay ticks, expected {expected}, got {decayed}"
    );
    assert!(
        decayed < initial,
        "value should decrease after decay"
    );
}

#[test]
fn decay_approaches_zero_over_many_ticks() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    grid.traverse(pos);

    for _ in 0..50000 {
        grid.tick();
    }

    let val = grid.road_value(pos);
    assert!(
        val < 1e-4,
        "after many decay ticks, value should approach 0, got {val}"
    );
}

// ── Speed bonus formula ────────────────────────────────────────────────────

#[test]
fn speed_multiplier_no_road() {
    let grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    let mult = grid.speed_multiplier(pos);
    assert!(
        (mult - 1.0).abs() < 1e-6,
        "no road should give speed_multiplier = 1.0, got {mult}"
    );
}

#[test]
fn speed_multiplier_max_road() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    for _ in 0..500 {
        grid.traverse(pos);
    }

    // road_value = 1.0, so speed_multiplier = 1.0 + 0.6 * 1.0 = 1.6
    let mult = grid.speed_multiplier(pos);
    assert!(
        (mult - 1.6).abs() < 1e-5,
        "max road should give speed_multiplier = 1.6, got {mult}"
    );
}

#[test]
fn speed_multiplier_partial_road() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    for _ in 0..250 {
        grid.traverse(pos);
    }

    // road_value = 0.5, so speed_multiplier = 1.0 + 0.6 * 0.5 = 1.3
    let mult = grid.speed_multiplier(pos);
    assert!(
        (mult - 1.3).abs() < 1e-4,
        "half road should give speed_multiplier ~1.3, got {mult}"
    );
}

#[test]
fn speed_multiplier_formula_matches_1_plus_bonus_times_value() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    for _ in 0..75 {
        grid.traverse(pos);
    }

    let rv = grid.road_value(pos);
    let mult = grid.speed_multiplier(pos);
    let expected = 1.0 + 0.6 * rv;
    assert!(
        (mult - expected).abs() < 1e-6,
        "speed_multiplier should be 1.0 + 0.6 * road_value ({rv}), expected {expected}, got {mult}"
    );
}

// ── No cross-cell contamination ────────────────────────────────────────────

#[test]
fn traverse_does_not_affect_adjacent_cells() {
    let mut grid = make_road_grid();
    // Cell size is 8, so pos (16, 16) maps to cell (2, 2).
    let pos = Vec2::new(16.0, 16.0);

    for _ in 0..100 {
        grid.traverse(pos);
    }

    // The target cell should have value 0.2
    assert!(
        (grid.road_value(pos) - 0.2).abs() < 1e-5,
        "target cell should have value 0.2"
    );

    // Adjacent cells (different grid cells) should still be 0.
    let adjacent_positions = [
        Vec2::new(8.0, 16.0),   // cell (1, 2) — left
        Vec2::new(24.0, 16.0),  // cell (3, 2) — right
        Vec2::new(16.0, 8.0),   // cell (2, 1) — above
        Vec2::new(16.0, 24.0),  // cell (2, 3) — below
        Vec2::new(8.0, 8.0),    // cell (1, 1) — top-left
        Vec2::new(24.0, 24.0),  // cell (3, 3) — bottom-right
    ];

    for adj in &adjacent_positions {
        let val = grid.road_value(*adj);
        assert!(
            val.abs() < 1e-6,
            "adjacent cell at ({}, {}) should be 0, got {val}",
            adj.x,
            adj.y
        );
    }
}

#[test]
fn multiple_cells_independent() {
    let mut grid = make_road_grid();

    let pos_a = Vec2::new(4.0, 4.0);   // cell (0, 0)
    let pos_b = Vec2::new(60.0, 60.0); // cell (7, 7)

    for _ in 0..50 {
        grid.traverse(pos_a);
    }
    for _ in 0..200 {
        grid.traverse(pos_b);
    }

    let val_a = grid.road_value(pos_a);
    let val_b = grid.road_value(pos_b);

    assert!(
        (val_a - 0.1).abs() < 1e-5,
        "cell A should have 0.1, got {val_a}"
    );
    assert!(
        (val_b - 0.4).abs() < 1e-5,
        "cell B should have 0.4, got {val_b}"
    );
}

// ── Out-of-bounds returns zero ─────────────────────────────────────────────

#[test]
fn out_of_bounds_road_value_is_zero() {
    let grid = make_road_grid();
    assert!(
        grid.road_value(Vec2::new(-1.0, 10.0)).abs() < 1e-6,
        "negative x should return 0"
    );
    assert!(
        grid.road_value(Vec2::new(10.0, -1.0)).abs() < 1e-6,
        "negative y should return 0"
    );
    assert!(
        grid.road_value(Vec2::new(200.0, 10.0)).abs() < 1e-6,
        "x beyond world should return 0"
    );
}

#[test]
fn out_of_bounds_speed_multiplier_is_1() {
    let grid = make_road_grid();
    let mult = grid.speed_multiplier(Vec2::new(-5.0, 10.0));
    assert!(
        (mult - 1.0).abs() < 1e-6,
        "out of bounds speed_multiplier should be 1.0, got {mult}"
    );
}

// ── Decay does not affect zero cells ───────────────────────────────────────

#[test]
fn decay_leaves_zero_cells_at_zero() {
    let mut grid = make_road_grid();
    let pos = Vec2::new(16.0, 16.0);

    // Initial value is 0.
    grid.tick();
    let val = grid.road_value(pos);
    assert!(
        val.abs() < 1e-10,
        "decaying a zero cell should remain 0, got {val}"
    );
}

// ── Grid dimensions ────────────────────────────────────────────────────────

#[test]
fn grid_dimensions_correct() {
    let config = mini_road_config();
    let grid = RoadGrid::new(&config, 64, 64);
    // 64 / 8 = 8 columns and rows
    assert_eq!(grid.cols(), 8, "expected 8 columns");
    assert_eq!(grid.rows(), 8, "expected 8 rows");
}

#[test]
fn grid_dimensions_rounding_up() {
    let config = mini_road_config();
    // 65 / 8 = 8.125, should round up to 9
    let grid = RoadGrid::new(&config, 65, 65);
    assert_eq!(grid.cols(), 9, "65/8 should round up to 9 columns");
    assert_eq!(grid.rows(), 9, "65/8 should round up to 9 rows");
}

// ── Positions within same cell share road value ────────────────────────────

#[test]
fn positions_in_same_cell_share_value() {
    let mut grid = make_road_grid();

    // Both positions are within cell (2, 2) with cell_size=8: range [16, 24)
    let pos1 = Vec2::new(16.5, 16.5);
    let pos2 = Vec2::new(23.0, 23.0);

    grid.traverse(pos1);

    let v1 = grid.road_value(pos1);
    let v2 = grid.road_value(pos2);
    assert!(
        (v1 - v2).abs() < 1e-6,
        "positions in same cell should share road value: {v1} vs {v2}"
    );
}
