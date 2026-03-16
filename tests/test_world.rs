mod common;

use rand::rngs::StdRng;
use rand::SeedableRng;

use swarm_economy::types::*;
use swarm_economy::world::resource_node::ResourceNode;
use swarm_economy::world::terrain::Terrain;

use common::*;

// ── Resource depletion / regen ──────────────────────────────────────────────

#[test]
fn extraction_increases_depletion() {
    let mut node = ResourceNode::new(0, Vec2::new(50.0, 50.0), Commodity::Timber, 3.0);
    let depletion_before = node.depletion;
    node.extract(Season::Spring, 100.0);
    assert!(
        node.depletion > depletion_before,
        "extraction should increase depletion: before={}, after={}",
        depletion_before,
        node.depletion
    );
}

#[test]
fn idle_node_regenerates() {
    let mut node = ResourceNode::new(0, Vec2::new(50.0, 50.0), Commodity::Timber, 3.0);

    // Deplete partially
    node.extract(Season::Spring, 100.0);
    node.tick_regeneration(); // clears currently_harvested flag

    let depletion_after_extract = node.depletion;

    // Let it regenerate
    node.tick_regeneration();

    assert!(
        node.depletion < depletion_after_extract,
        "idle node should regenerate: after_extract={}, after_regen={}",
        depletion_after_extract,
        node.depletion
    );
}

#[test]
fn multiple_extractions_increase_depletion_monotonically() {
    let mut node = ResourceNode::new(0, Vec2::new(50.0, 50.0), Commodity::Timber, 3.0);
    let mut prev = node.depletion;
    for _ in 0..10 {
        let _y = node.extract(Season::Spring, 100.0);
        node.tick_regeneration(); // clear harvested flag for next round
        if node.depletion >= 1.0 {
            break;
        }
        assert!(
            node.depletion >= prev,
            "depletion should not decrease during extraction"
        );
        prev = node.depletion;
        // Re-enable extraction (tick_regeneration cleared harvested)
    }
}

// ── Exhausted nodes ─────────────────────────────────────────────────────────

#[test]
fn exhausted_node_yields_zero() {
    let mut node = ResourceNode::new(0, Vec2::new(50.0, 50.0), Commodity::Timber, 3.0);
    node.depletion = 1.0;

    let y = node.extract(Season::Spring, 100.0);
    assert!(
        y.abs() < 1e-6,
        "exhausted node (depletion=1.0) should yield 0, got {}",
        y
    );
}

#[test]
fn exhausted_node_recovers_to_0_8_after_2000_idle_ticks() {
    let mut node = ResourceNode::new(0, Vec2::new(50.0, 50.0), Commodity::Timber, 3.0);
    node.depletion = 1.0;

    for _ in 0..2000 {
        node.tick_regeneration();
    }

    assert!(
        (node.depletion - 0.8).abs() < 1e-5,
        "exhausted node should recover to 0.8 depletion after 2000 idle ticks, got {}",
        node.depletion
    );
}

#[test]
fn exhausted_node_does_not_recover_early() {
    let mut node = ResourceNode::new(0, Vec2::new(50.0, 50.0), Commodity::Timber, 3.0);
    node.depletion = 1.0;

    for _ in 0..1999 {
        node.tick_regeneration();
    }

    assert!(
        (node.depletion - 1.0).abs() < 1e-5,
        "exhausted node should NOT recover before 2000 idle ticks, depletion={}",
        node.depletion
    );
}

#[test]
fn exhausted_node_recovery_resets_on_harvest() {
    let mut node = ResourceNode::new(0, Vec2::new(50.0, 50.0), Commodity::Timber, 3.0);
    node.depletion = 1.0;

    // Idle for 1000 ticks
    for _ in 0..1000 {
        node.tick_regeneration();
    }

    // Harvest resets the idle counter
    node.extract(Season::Spring, 100.0);
    node.tick_regeneration(); // clears harvested flag

    // Need another full 2000 idle ticks now
    for _ in 0..1999 {
        node.tick_regeneration();
    }

    assert!(
        (node.depletion - 1.0).abs() < 1e-5,
        "harvest should reset the exhaustion counter"
    );
}

// ── City spacing ────────────────────────────────────────────────────────────

#[test]
fn city_spacing_poisson_disk_minimum() {
    // Use a larger world so Poisson-disk sampling has room to work.
    // The mini config has radius=15, so min_spacing = 15*4 = 60, which
    // needs a world at least 120x120 (margin on each side).
    let mut config = mini_economy_config();
    config.world.width = 256;
    config.world.height = 256;
    config.world.num_cities = 5;

    let terrain = Terrain::new(&config.world);
    let mut rng = StdRng::seed_from_u64(42);

    let cities = swarm_economy::world::city::City::generate(
        &config.world,
        &config.city,
        |pos| {
            let tx = (pos.x as u32).min(terrain.width().saturating_sub(1));
            let ty = (pos.y as u32).min(terrain.height().saturating_sub(1));
            terrain.terrain_at(tx, ty)
        },
        &mut rng,
    );

    let min_spacing = config.city.radius * 4.0;

    // Only check if we placed at least 2 cities (placement can fail on
    // very hostile terrain, but 256x256 should be fine).
    assert!(
        cities.len() >= 2,
        "should place at least 2 cities on a 256x256 map"
    );

    // Check all pairs
    for i in 0..cities.len() {
        for j in (i + 1)..cities.len() {
            let dist = cities[i].position.distance(cities[j].position);
            assert!(
                dist >= min_spacing - 0.1, // small epsilon for floating-point
                "cities {} and {} are too close: distance={:.2}, min_spacing={:.2}",
                cities[i].id,
                cities[j].id,
                dist,
                min_spacing
            );
        }
    }
}

// ── Terrain determinism ─────────────────────────────────────────────────────

#[test]
fn same_seed_produces_identical_heightmap() {
    let config = mini_world_config();
    let t1 = Terrain::new(&config);
    let t2 = Terrain::new(&config);

    // Compare heights at many positions
    for y in 0..config.height {
        for x in 0..config.width {
            let h1 = t1.height_at(x, y);
            let h2 = t2.height_at(x, y);
            assert!(
                (h1 - h2).abs() < 1e-10,
                "height mismatch at ({}, {}): {} vs {}",
                x, y, h1, h2
            );
        }
    }
}

#[test]
fn same_seed_produces_identical_terrain_types() {
    let config = mini_world_config();
    let t1 = Terrain::new(&config);
    let t2 = Terrain::new(&config);

    for y in 0..config.height {
        for x in 0..config.width {
            assert_eq!(
                t1.terrain_at(x, y),
                t2.terrain_at(x, y),
                "terrain type mismatch at ({}, {})",
                x, y
            );
        }
    }
}

#[test]
fn different_seed_produces_different_heightmap() {
    let config1 = mini_world_config();
    let mut config2 = mini_world_config();
    config2.terrain_seed = 999;

    let t1 = Terrain::new(&config1);
    let t2 = Terrain::new(&config2);

    let mut differences = 0;
    for y in 0..config1.height {
        for x in 0..config1.width {
            if (t1.height_at(x, y) - t2.height_at(x, y)).abs() > 1e-6 {
                differences += 1;
            }
        }
    }

    assert!(
        differences > 0,
        "different seeds should produce different heightmaps"
    );
}

// ── Bounds checking ─────────────────────────────────────────────────────────

#[test]
fn terrain_at_valid_coords_does_not_panic() {
    let config = mini_world_config();
    let terrain = Terrain::new(&config);

    // Check all four corners and center
    let positions = [
        (0, 0),
        (config.width - 1, 0),
        (0, config.height - 1),
        (config.width - 1, config.height - 1),
        (config.width / 2, config.height / 2),
    ];

    for &(x, y) in &positions {
        let _tt = terrain.terrain_at(x, y);
        let _h = terrain.height_at(x, y);
        // If we get here without panicking, the test passes
    }
}

#[test]
fn height_values_normalized_0_to_1() {
    let config = mini_world_config();
    let terrain = Terrain::new(&config);

    let mut min_h = f32::MAX;
    let mut max_h = f32::MIN;

    for y in 0..config.height {
        for x in 0..config.width {
            let h = terrain.height_at(x, y);
            if h < min_h { min_h = h; }
            if h > max_h { max_h = h; }
        }
    }

    assert!(
        min_h >= -1e-5,
        "minimum height should be >= 0, got {}",
        min_h
    );
    assert!(
        max_h <= 1.0 + 1e-5,
        "maximum height should be <= 1.0, got {}",
        max_h
    );
    assert!(
        (min_h).abs() < 1e-4,
        "minimum height should be ~0, got {}",
        min_h
    );
    assert!(
        (max_h - 1.0).abs() < 1e-4,
        "maximum height should be ~1.0, got {}",
        max_h
    );
}

#[test]
fn is_passable_consistent_with_terrain_at() {
    let config = mini_world_config();
    let terrain = Terrain::new(&config);

    for y in 0..config.height {
        for x in 0..config.width {
            let tt = terrain.terrain_at(x, y);
            let passable = terrain.is_passable(x, y);
            assert_eq!(
                passable,
                tt.is_passable(),
                "is_passable({},{}) should be consistent with terrain_at",
                x, y
            );
        }
    }
}

#[test]
fn speed_at_consistent_with_terrain_type() {
    let config = mini_world_config();
    let terrain = Terrain::new(&config);

    for y in 0..config.height {
        for x in 0..config.width {
            let tt = terrain.terrain_at(x, y);
            let spring_speed = terrain.speed_at(x, y, Season::Spring);
            assert!(
                (spring_speed - tt.speed_multiplier()).abs() < 1e-6,
                "spring speed at ({},{}) should match terrain speed multiplier",
                x, y
            );

            // Winter applies 0.7 modifier
            let winter_speed = terrain.speed_at(x, y, Season::Winter);
            assert!(
                (winter_speed - tt.speed_multiplier() * 0.7).abs() < 1e-5,
                "winter speed at ({},{}) should be 0.7x base",
                x, y
            );
        }
    }
}
