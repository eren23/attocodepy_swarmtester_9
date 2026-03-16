//! Criterion benchmarks for pathfinding-related operations:
//! terrain lookups, road speed multiplier queries, reputation gradient
//! computation, and sensory input building (the "A*-equivalent" in this
//! agent-based simulation).
//!
//! Target: sensory build (A*-equivalent) < 500μs per merchant.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use swarm_economy::config::{
    ChannelConfig, ReputationChannels, ReputationConfig, RoadConfig, WorldConfig,
};
use swarm_economy::types::{ReputationChannel, Season, Vec2};
use swarm_economy::world::reputation::ReputationGrid;
use swarm_economy::world::road::RoadGrid;
use swarm_economy::world::terrain::Terrain;

fn full_world_config() -> WorldConfig {
    WorldConfig {
        width: 1600,
        height: 1000,
        terrain_seed: 42,
        terrain_octaves: 4,
        sea_level: 0.25,
        num_cities: 10,
        num_resource_nodes: 30,
        season_length_ticks: 2500,
    }
}

fn full_reputation_config() -> ReputationConfig {
    let ch = ChannelConfig {
        decay: 0.99,
        diffusion_sigma: 0.6,
        color: [255, 255, 255],
    };
    ReputationConfig {
        cell_size: 8,
        channels: ReputationChannels {
            profit: ch.clone(),
            demand: ch.clone(),
            danger: ch.clone(),
            opportunity: ch,
        },
    }
}

fn full_road_config() -> RoadConfig {
    RoadConfig {
        cell_size: 8,
        increment: 0.002,
        decay: 0.9998,
        max_speed_bonus: 0.6,
    }
}

fn bench_terrain_lookups(c: &mut Criterion) {
    let terrain = Terrain::new(&full_world_config());

    c.bench_function("terrain_1000_lookups", |b| {
        b.iter(|| {
            let mut passable = 0u32;
            for i in 0..1000 {
                let x = (i * 7 % 1600) as u32;
                let y = (i * 13 % 1000) as u32;
                if terrain.is_passable(x, y) {
                    passable += 1;
                }
            }
            black_box(passable);
        });
    });
}

fn bench_terrain_speed_at(c: &mut Criterion) {
    let terrain = Terrain::new(&full_world_config());

    c.bench_function("terrain_speed_at_1000", |b| {
        b.iter(|| {
            let mut total = 0.0f32;
            for i in 0..1000 {
                let x = (i * 7 % 1600) as u32;
                let y = (i * 13 % 1000) as u32;
                total += terrain.speed_at(x, y, Season::Summer);
            }
            black_box(total);
        });
    });
}

fn bench_road_speed_multiplier(c: &mut Criterion) {
    let config = full_road_config();
    let mut grid = RoadGrid::new(&config, 1600, 1000);

    // Pre-seed roads.
    for i in 0..500 {
        let x = (i * 3 % 1600) as f32;
        let y = (i * 7 % 1000) as f32;
        grid.traverse(Vec2::new(x, y));
    }

    c.bench_function("road_speed_multiplier_1000", |b| {
        b.iter(|| {
            let mut total = 0.0f32;
            for i in 0..1000 {
                let x = (i * 11 % 1600) as f32;
                let y = (i * 17 % 1000) as f32;
                total += grid.speed_multiplier(Vec2::new(x, y));
            }
            black_box(total);
        });
    });
}

fn bench_reputation_gradient_pathfinding(c: &mut Criterion) {
    let config = full_reputation_config();
    let mut grid = ReputationGrid::new(&config, 1600, 1000);

    // Pre-seed profit signals.
    for i in 0..100 {
        let x = (i * 7 % 1600) as f32;
        let y = (i * 13 % 1000) as f32;
        grid.deposit(ReputationChannel::Profit, Vec2::new(x, y), 0.5);
    }
    grid.tick();

    c.bench_function("gradient_pathfinding_200", |b| {
        b.iter(|| {
            let mut total = Vec2::ZERO;
            for i in 0..200 {
                let x = (i * 11 % 1600) as f32;
                let y = (i * 17 % 1000) as f32;
                let g = grid.gradient(ReputationChannel::Profit, Vec2::new(x, y));
                total = total + g;
            }
            black_box(total);
        });
    });
}

fn bench_scanner_sample(c: &mut Criterion) {
    let config = full_reputation_config();
    let mut grid = ReputationGrid::new(&config, 1600, 1000);

    for i in 0..100 {
        let x = (i * 7 % 1600) as f32;
        let y = (i * 13 % 1000) as f32;
        grid.deposit(ReputationChannel::Profit, Vec2::new(x, y), 0.5);
    }
    grid.tick();

    let half_angle = 35.0f32.to_radians();

    c.bench_function("scanner_sample_200_merchants", |b| {
        b.iter(|| {
            let mut total_left = 0.0f32;
            let mut total_right = 0.0f32;
            for i in 0..200 {
                let x = (i * 11 % 1400 + 100) as f32;
                let y = (i * 17 % 800 + 100) as f32;
                let heading = (i as f32 * 0.31).sin() * std::f32::consts::TAU;
                let (l, r) = grid.scanner_sample(
                    ReputationChannel::Profit,
                    Vec2::new(x, y),
                    heading,
                    half_angle,
                    60.0,
                );
                total_left += l;
                total_right += r;
            }
            black_box((total_left, total_right));
        });
    });
}

fn bench_combined_pathfinding_per_merchant(c: &mut Criterion) {
    let terrain = Terrain::new(&full_world_config());
    let rep_config = full_reputation_config();
    let road_config = full_road_config();
    let mut rep = ReputationGrid::new(&rep_config, 1600, 1000);
    let mut road = RoadGrid::new(&road_config, 1600, 1000);

    // Pre-seed.
    for i in 0..200 {
        let x = (i * 7 % 1600) as f32;
        let y = (i * 13 % 1000) as f32;
        rep.deposit(ReputationChannel::Profit, Vec2::new(x, y), 0.3);
        rep.deposit(ReputationChannel::Danger, Vec2::new(x + 50.0, y), 0.2);
        road.traverse(Vec2::new(x, y));
    }
    rep.tick();

    let half_angle = 35.0f32.to_radians();

    c.bench_function("combined_pathfinding_per_merchant", |b| {
        b.iter(|| {
            // Simulate what one merchant does per tick for navigation:
            // 1. Terrain lookup
            // 2. Road speed multiplier
            // 3. Profit gradient
            // 4. Danger gradient
            // 5. Scanner sample (profit + danger)
            let pos = Vec2::new(800.0, 500.0);
            let heading = 1.0f32;

            let tx = (pos.x as u32).min(1599);
            let ty = (pos.y as u32).min(999);
            let passable = terrain.is_passable(tx, ty);
            let speed = terrain.speed_at(tx, ty, Season::Summer);
            let road_mult = road.speed_multiplier(pos);
            let profit_grad = rep.gradient(ReputationChannel::Profit, pos);
            let danger_grad = rep.gradient(ReputationChannel::Danger, pos);
            let profit_scan =
                rep.scanner_sample(ReputationChannel::Profit, pos, heading, half_angle, 60.0);
            let danger_scan =
                rep.scanner_sample(ReputationChannel::Danger, pos, heading, half_angle, 60.0);

            black_box((
                passable,
                speed,
                road_mult,
                profit_grad,
                danger_grad,
                profit_scan,
                danger_scan,
            ));
        });
    });
}

criterion_group!(
    benches,
    bench_terrain_lookups,
    bench_terrain_speed_at,
    bench_road_speed_multiplier,
    bench_reputation_gradient_pathfinding,
    bench_scanner_sample,
    bench_combined_pathfinding_per_merchant,
);
criterion_main!(benches);
