//! Criterion benchmarks for a full simulation tick with 200 merchants.
//!
//! Target: full 200-merchant tick < 8ms.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use rand::rngs::StdRng;
use rand::SeedableRng;

use swarm_economy::config::EconomyConfig;
use swarm_economy::world::world::World;

fn make_world() -> (World, StdRng) {
    let config = EconomyConfig::load("economy_config.toml").expect("load config");
    let mut rng = StdRng::seed_from_u64(42);
    let world = World::new(config, &mut rng);
    (world, rng)
}

fn bench_full_tick_200_merchants(c: &mut Criterion) {
    let (mut world, mut rng) = make_world();

    // Warm up for 50 ticks to establish some state.
    for _ in 0..50 {
        world.tick(&mut rng);
    }

    c.bench_function("full_tick_200_merchants", |b| {
        b.iter(|| {
            world.tick(&mut rng);
            black_box(world.current_tick);
        });
    });
}

fn bench_tick_10_iterations(c: &mut Criterion) {
    let (mut world, mut rng) = make_world();

    // Warm up.
    for _ in 0..50 {
        world.tick(&mut rng);
    }

    c.bench_function("tick_10_iterations", |b| {
        b.iter(|| {
            for _ in 0..10 {
                world.tick(&mut rng);
            }
            black_box(world.current_tick);
        });
    });
}

fn bench_world_initialization(c: &mut Criterion) {
    let config = EconomyConfig::load("economy_config.toml").expect("load config");

    c.bench_function("world_initialization", |b| {
        b.iter(|| {
            let mut rng = StdRng::seed_from_u64(42);
            let world = World::new(config.clone(), &mut rng);
            black_box(world.current_tick);
        });
    });
}

criterion_group!(
    benches,
    bench_full_tick_200_merchants,
    bench_tick_10_iterations,
    bench_world_initialization,
);
criterion_main!(benches);
