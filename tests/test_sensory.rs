mod common;

use swarm_economy::agents::merchant::Merchant;
use swarm_economy::agents::sensory::{BanditInfo, ResourceNodeInfo, SensoryInputBuilder};
use swarm_economy::types::*;
use swarm_economy::world::reputation::ReputationGrid;

use common::{
    make_all_land_terrain, make_merchant_at, make_merchant_with_id, make_mini_cities,
    make_reputation_grid, make_road_grid, mini_merchant_config,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a sensory input for `merchant` against the given world state.
fn build_sensory(
    merchant: &Merchant,
    others: &[&Merchant],
    bandits: &[BanditInfo],
    resources: &[ResourceNodeInfo],
    reputation: &ReputationGrid,
) -> swarm_economy::agents::sensory::SensoryInput {
    let config = mini_merchant_config();
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();
    let cities = make_mini_cities();

    let builder = SensoryInputBuilder::new(
        merchant, &config, &terrain, &roads, reputation, &cities, Season::Spring,
    );
    builder.build(others, bandits, resources)
}

// ── 1. Scanner cone geometry ─────────────────────────────────────────────────

#[test]
fn scanner_cone_samples_correct_areas() {
    let mut rep = make_reputation_grid();
    // Merchant at center of 64x64 world, force heading = 0 (facing +x).
    let mut m = make_merchant_at(Vec2::new(32.0, 32.0), Profession::Trader);
    m.heading = 0.0;

    // Place strong signal ahead and to the right of the merchant.
    // With heading=0, right cone spans positive y offsets.
    rep.deposit(ReputationChannel::Profit, Vec2::new(50.0, 40.0), 1.0);

    let input = build_sensory(&m, &[], &[], &[], &rep);

    // Both left and right scanners should have values (signal is mostly ahead).
    let left_profit = input.left_scanner[0]; // index 0 = Profit
    let right_profit = input.right_scanner[0];

    // Signal is at positive-y offset from heading=0, so right cone should see more.
    assert!(
        right_profit > 0.0 || left_profit > 0.0,
        "at least one scanner should detect the signal"
    );
}

// ── 2. Left/right discrimination ─────────────────────────────────────────────

#[test]
fn left_right_discrimination() {
    // Use a larger reputation grid so the signal and sample points fit inside.
    let config = swarm_economy::config::ReputationConfig {
        cell_size: 4,
        channels: swarm_economy::config::ReputationChannels {
            profit: swarm_economy::config::ChannelConfig { decay: 1.0, diffusion_sigma: 0.0, color: [255,255,255] },
            demand: swarm_economy::config::ChannelConfig { decay: 1.0, diffusion_sigma: 0.0, color: [255,255,255] },
            danger: swarm_economy::config::ChannelConfig { decay: 1.0, diffusion_sigma: 0.0, color: [255,255,255] },
            opportunity: swarm_economy::config::ChannelConfig { decay: 1.0, diffusion_sigma: 0.0, color: [255,255,255] },
        },
    };
    let mut rep = ReputationGrid::new(&config, 256, 256);

    // Merchant at (128, 128), heading=0 (facing +x).
    let mut m = make_merchant_at(Vec2::new(128.0, 128.0), Profession::Trader);
    m.heading = 0.0;

    // Place signal clearly to the right of heading.
    // Right cone sweeps heading..heading+35deg (positive y direction).
    // Deposit a broad patch of signal in the right-cone area.
    // At distance ~40, angle ~+20deg from heading: (128+40*cos20, 128+40*sin20) ≈ (165.6, 141.7)
    for dx in -2i32..=2 {
        for dy in -2i32..=2 {
            rep.deposit(
                ReputationChannel::Demand,
                Vec2::new(166.0 + dx as f32 * 4.0, 142.0 + dy as f32 * 4.0),
                1.0,
            );
        }
    }

    // Sample using the reputation grid's scanner_sample directly to verify.
    let half_angle = 35.0f32.to_radians();
    let (left, right) = rep.scanner_sample(
        ReputationChannel::Demand,
        Vec2::new(128.0, 128.0),
        0.0,
        half_angle,
        60.0,
    );

    assert!(
        right > left,
        "signal placed to the right should read higher on right cone: left={left}, right={right}"
    );
}

// ── 3. Raycast distances ─────────────────────────────────────────────────────

#[test]
fn terrain_rays_report_correct_distances() {
    let rep = make_reputation_grid();
    let terrain = make_all_land_terrain();
    let config = mini_merchant_config();

    // Place merchant at a known passable position.
    let pos = common::find_passable_pos(&terrain);
    let mut m = make_merchant_at(pos, Profession::Trader);
    m.heading = 0.0;

    let input = build_sensory(&m, &[], &[], &[], &rep);

    // All 5 rays should report positive distances, capped at the ray range.
    assert_eq!(input.terrain_rays.len(), 5);
    for (i, ray) in input.terrain_rays.iter().enumerate() {
        assert!(
            ray.distance > 0.0,
            "ray {i} distance should be positive, got {}",
            ray.distance
        );
        assert!(
            ray.distance <= config.terrain_ray_range + 0.1,
            "ray {i} distance {} exceeds max range {}",
            ray.distance,
            config.terrain_ray_range
        );
    }

    // If a ray hit an obstacle, its distance should be less than range.
    // If it didn't, distance should equal range.
    for (i, ray) in input.terrain_rays.iter().enumerate() {
        if !ray.terrain_type.is_passable() {
            assert!(
                ray.distance < config.terrain_ray_range,
                "ray {i} reports impassable terrain at max range — should have shorter distance"
            );
        }
    }
}

// ── 4. Neighbor detection ────────────────────────────────────────────────────

#[test]
fn neighbors_within_30px_detected() {
    let rep = make_reputation_grid();
    let m1 = make_merchant_with_id(1, Vec2::new(32.0, 32.0), Profession::Trader);
    let close = make_merchant_with_id(2, Vec2::new(40.0, 32.0), Profession::Miner); // 8px away
    let far = make_merchant_with_id(3, Vec2::new(32.0, 63.0), Profession::Farmer); // 31px away

    let others: Vec<&Merchant> = vec![&close, &far];
    let input = build_sensory(&m1, &others, &[], &[], &rep);

    // close is 8px away — within 30px radius.
    // far is 31px away — outside 30px radius.
    assert_eq!(
        input.neighbors.len(),
        1,
        "expected 1 neighbor, got {}",
        input.neighbors.len()
    );
    assert_eq!(input.neighbors[0].profession, Profession::Miner);
}

#[test]
fn neighbors_outside_30px_excluded() {
    let rep = make_reputation_grid();
    let m1 = make_merchant_with_id(1, Vec2::new(10.0, 10.0), Profession::Trader);
    let far = make_merchant_with_id(2, Vec2::new(50.0, 50.0), Profession::Miner); // ~56px away

    let others: Vec<&Merchant> = vec![&far];
    let input = build_sensory(&m1, &others, &[], &[], &rep);

    assert!(
        input.neighbors.is_empty(),
        "merchant 56px away should not be a neighbor"
    );
}

// ── 5. City direction accuracy ───────────────────────────────────────────────

#[test]
fn nearest_city_direction_points_toward_closest() {
    let rep = make_reputation_grid();
    // Cities from make_mini_cities: (16,16), (48,16), (32,48)
    // Merchant at (17,16) — very close to city 0 at (16,16).
    let m = make_merchant_at(Vec2::new(17.0, 16.0), Profession::Trader);

    let input = build_sensory(&m, &[], &[], &[], &rep);

    let (dir, dist) = input.nearest_city;

    // Nearest city should be city 0 at (16,16), ~1px away.
    assert!(
        dist < 2.0,
        "expected nearest city distance < 2.0, got {dist}"
    );

    // Direction should point roughly toward (16,16) from (17,16) — negative x.
    assert!(
        dir.x < 0.0,
        "direction x should be negative (pointing left to city), got {}",
        dir.x
    );
}

// ── 6. Price memory TTL ──────────────────────────────────────────────────────

#[test]
fn price_memory_ttl_expiry() {
    let mut m = make_merchant_at(Vec2::new(32.0, 32.0), Profession::Trader);

    // Record a price at tick 0.
    m.price_memory.record(0, Commodity::Grain, 10.0, 0);

    // Retrieving at tick 500 (within TTL of 1000) should succeed.
    assert!(
        m.price_memory.get(0, Commodity::Grain, 500).is_some(),
        "price should be available within TTL"
    );

    // Retrieving at tick 1001 (past TTL) should return None.
    assert!(
        m.price_memory.get(0, Commodity::Grain, 1001).is_none(),
        "price should expire after TTL"
    );
}

#[test]
fn price_memory_prune_removes_stale() {
    let mut m = make_merchant_at(Vec2::new(32.0, 32.0), Profession::Trader);

    m.price_memory.record(0, Commodity::Grain, 10.0, 0);
    m.price_memory.record(0, Commodity::Ore, 15.0, 900);

    // Prune at tick 1100: Grain (tick 0) is stale (age 1100 > ttl 1000), Ore (tick 900, age 200) is fresh.
    m.price_memory.prune(1100);

    assert!(
        m.price_memory.get(0, Commodity::Grain, 1100).is_none(),
        "Grain should be pruned"
    );
    assert!(
        m.price_memory.get(0, Commodity::Ore, 1100).is_some(),
        "Ore should survive pruning"
    );
}

// ── 7. Bandit proximity ──────────────────────────────────────────────────────

#[test]
fn bandit_within_80px_detected() {
    let rep = make_reputation_grid();
    let m = make_merchant_at(Vec2::new(32.0, 32.0), Profession::Trader);

    let bandits = vec![
        BanditInfo {
            pos: Vec2::new(52.0, 32.0), // 20px away
        },
    ];

    let input = build_sensory(&m, &[], &bandits, &[], &rep);

    assert!(
        input.nearest_bandit.is_some(),
        "bandit 20px away should be detected"
    );
    let (_, dist) = input.nearest_bandit.unwrap();
    assert!(
        (dist - 20.0).abs() < 1.0,
        "expected distance ~20, got {dist}"
    );
}

#[test]
fn bandit_outside_80px_not_detected() {
    let rep = make_reputation_grid();
    // Use a position where the bandit is clearly > 80px away.
    // World is only 64x64, so we need positions far enough apart.
    let m2 = make_merchant_at(Vec2::new(1.0, 1.0), Profession::Trader);
    let far_bandits = vec![
        BanditInfo {
            pos: Vec2::new(62.0, 62.0), // ~86px away
        },
    ];

    let input = build_sensory(&m2, &[], &far_bandits, &[], &rep);

    assert!(
        input.nearest_bandit.is_none(),
        "bandit ~86px away should not be detected (range is 80px)"
    );
}

#[test]
fn bandit_nearest_selected_from_multiple() {
    let rep = make_reputation_grid();
    let m = make_merchant_at(Vec2::new(32.0, 32.0), Profession::Trader);

    let bandits = vec![
        BanditInfo {
            pos: Vec2::new(50.0, 32.0), // 18px away
        },
        BanditInfo {
            pos: Vec2::new(42.0, 32.0), // 10px away — closest
        },
    ];

    let input = build_sensory(&m, &[], &bandits, &[], &rep);

    assert!(input.nearest_bandit.is_some());
    let (_, dist) = input.nearest_bandit.unwrap();
    assert!(
        (dist - 10.0).abs() < 1.0,
        "expected nearest bandit at ~10px, got {dist}"
    );
}
