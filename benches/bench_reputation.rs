//! Criterion benchmarks for the reputation grid: deposit, tick (diffusion + decay),
//! sample, gradient, and combined reputation+road operations.
//!
//! Target: reputation + road tick combined < 2ms for a 1600x1000 world.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use swarm_economy::config::{ChannelConfig, ReputationChannels, ReputationConfig, RoadConfig};
use swarm_economy::types::{ReputationChannel, Vec2};
use swarm_economy::world::reputation::ReputationGrid;
use swarm_economy::world::road::RoadGrid;

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

fn bench_reputation_tick(c: &mut Criterion) {
    let config = full_reputation_config();
    let mut grid = ReputationGrid::new(&config, 1600, 1000);

    // Pre-seed with some deposits.
    for i in 0..200 {
        let x = (i * 7 % 1600) as f32;
        let y = (i * 13 % 1000) as f32;
        grid.deposit(ReputationChannel::Profit, Vec2::new(x, y), 0.5);
        grid.deposit(ReputationChannel::Danger, Vec2::new(x + 10.0, y + 10.0), 0.3);
    }

    c.bench_function("reputation_tick_1600x1000", |b| {
        b.iter(|| {
            grid.tick();
            black_box(&grid);
        });
    });
}

fn bench_road_tick(c: &mut Criterion) {
    let config = full_road_config();
    let mut grid = RoadGrid::new(&config, 1600, 1000);

    // Pre-seed road traversals.
    for i in 0..500 {
        let x = (i * 3 % 1600) as f32;
        let y = (i * 7 % 1000) as f32;
        grid.traverse(Vec2::new(x, y));
    }

    c.bench_function("road_tick_1600x1000", |b| {
        b.iter(|| {
            grid.tick();
            black_box(&grid);
        });
    });
}

fn bench_reputation_plus_road(c: &mut Criterion) {
    let rep_config = full_reputation_config();
    let road_config = full_road_config();
    let mut rep = ReputationGrid::new(&rep_config, 1600, 1000);
    let mut road = RoadGrid::new(&road_config, 1600, 1000);

    // Pre-seed.
    for i in 0..200 {
        let x = (i * 7 % 1600) as f32;
        let y = (i * 13 % 1000) as f32;
        rep.deposit(ReputationChannel::Profit, Vec2::new(x, y), 0.5);
        road.traverse(Vec2::new(x, y));
    }

    c.bench_function("reputation_plus_road_tick", |b| {
        b.iter(|| {
            rep.tick();
            road.tick();
            black_box((&rep, &road));
        });
    });
}

fn bench_reputation_deposit(c: &mut Criterion) {
    let config = full_reputation_config();
    let mut grid = ReputationGrid::new(&config, 1600, 1000);

    c.bench_function("reputation_deposit_200", |b| {
        b.iter(|| {
            for i in 0..200 {
                let x = (i * 7 % 1600) as f32;
                let y = (i * 13 % 1000) as f32;
                grid.deposit(ReputationChannel::Profit, Vec2::new(x, y), 0.1);
            }
            black_box(&grid);
        });
    });
}

fn bench_reputation_sample(c: &mut Criterion) {
    let config = full_reputation_config();
    let mut grid = ReputationGrid::new(&config, 1600, 1000);

    // Seed data.
    for i in 0..100 {
        let x = (i * 7 % 1600) as f32;
        let y = (i * 13 % 1000) as f32;
        grid.deposit(ReputationChannel::Profit, Vec2::new(x, y), 0.5);
    }
    grid.tick();

    c.bench_function("reputation_sample_1000", |b| {
        b.iter(|| {
            let mut total = 0.0f32;
            for i in 0..1000 {
                let x = (i * 11 % 1600) as f32;
                let y = (i * 17 % 1000) as f32;
                total += grid.sample(ReputationChannel::Profit, Vec2::new(x, y));
            }
            black_box(total);
        });
    });
}

fn bench_reputation_gradient(c: &mut Criterion) {
    let config = full_reputation_config();
    let mut grid = ReputationGrid::new(&config, 1600, 1000);

    for i in 0..100 {
        let x = (i * 7 % 1600) as f32;
        let y = (i * 13 % 1000) as f32;
        grid.deposit(ReputationChannel::Profit, Vec2::new(x, y), 0.5);
    }
    grid.tick();

    c.bench_function("reputation_gradient_200", |b| {
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

criterion_group!(
    benches,
    bench_reputation_tick,
    bench_road_tick,
    bench_reputation_plus_road,
    bench_reputation_deposit,
    bench_reputation_sample,
    bench_reputation_gradient,
);
criterion_main!(benches);
