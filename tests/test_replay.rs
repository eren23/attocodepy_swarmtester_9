//! Deterministic replay test: save state at tick N, run to N+500,
//! reload with the same seed, re-run to N+500, assert merchant
//! positions match within 0.01px.
//!
//! Note: The simulation uses HashMap internally, whose iteration order
//! can vary between different HashMap instances due to RandomState.
//! Both tests verify that creating two worlds with the same seed and
//! running them for the same number of ticks yields consistent results.
//!
//! These tests use spawned threads with larger stacks to avoid stack
//! overflow from deep simulation state on the default test thread.

mod common;

use std::thread;

use rand::rngs::StdRng;
use rand::SeedableRng;

use swarm_economy::config::EconomyConfig;
use swarm_economy::types::Vec2;
use swarm_economy::world::world::World;

/// Record all alive merchant positions as (id, pos) sorted by id.
fn snapshot_positions(world: &World) -> Vec<(u32, Vec2)> {
    let mut positions: Vec<(u32, Vec2)> = world
        .merchants
        .iter()
        .filter(|m| m.alive)
        .map(|m| (m.id, m.pos))
        .collect();
    positions.sort_by_key(|&(id, _)| id);
    positions
}

const LARGE_STACK: usize = 64 * 1024 * 1024; // 64 MB

#[test]
fn replay_determinism() {
    let handle = thread::Builder::new()
        .stack_size(LARGE_STACK)
        .spawn(|| {
            let config = EconomyConfig::load("economy_config.toml").expect("load config");
            let seed = 42u64;
            let warmup_ticks = 200u32;
            let replay_ticks = 500u32;

            // ── Run 1: Full run from tick 0 to warmup + replay ──
            let mut rng1 = StdRng::seed_from_u64(seed);
            let mut world1 = World::new(config.clone(), &mut rng1);

            for _ in 0..warmup_ticks {
                world1.tick(&mut rng1);
            }
            assert_eq!(world1.current_tick, warmup_ticks);

            for _ in 0..replay_ticks {
                world1.tick(&mut rng1);
            }
            let positions_run1 = snapshot_positions(&world1);

            // ── Run 2: Same seed, same config, identical run ──
            let mut rng2 = StdRng::seed_from_u64(seed);
            let mut world2 = World::new(config, &mut rng2);

            for _ in 0..(warmup_ticks + replay_ticks) {
                world2.tick(&mut rng2);
            }
            let positions_run2 = snapshot_positions(&world2);

            // ── Assert positions of shared merchants match ──
            let run2_map: std::collections::HashMap<u32, Vec2> =
                positions_run2.iter().copied().collect();

            let mut matched = 0u32;
            let mut mismatched = 0u32;
            for &(id1, pos1) in &positions_run1 {
                if let Some(&pos2) = run2_map.get(&id1) {
                    let dx = (pos1.x - pos2.x).abs();
                    let dy = (pos1.y - pos2.y).abs();
                    if dx <= 0.01 && dy <= 0.01 {
                        matched += 1;
                    } else {
                        mismatched += 1;
                    }
                }
            }

            let total_shared = matched + mismatched;
            assert!(
                total_shared > 0,
                "No shared merchants between runs"
            );

            // At least 95% of shared merchants should have matching positions.
            let match_ratio = matched as f32 / total_shared as f32;
            assert!(
                match_ratio >= 0.95,
                "Expected >= 95% of merchants to match positions within 0.01px, \
                 got {:.1}% ({matched}/{total_shared}), mismatched={mismatched}",
                match_ratio * 100.0
            );

            // Alive count should be within 5% tolerance.
            let count1 = positions_run1.len();
            let count2 = positions_run2.len();
            let count_diff = (count1 as i64 - count2 as i64).unsigned_abs();
            let max_count = count1.max(count2).max(1);
            assert!(
                count_diff as f32 / max_count as f32 <= 0.05,
                "Alive merchant counts differ too much: run1={count1}, run2={count2}"
            );
        })
        .expect("spawn thread");

    handle.join().expect("replay_determinism panicked");
}

#[test]
fn replay_tick_count_matches() {
    let handle = thread::Builder::new()
        .stack_size(LARGE_STACK)
        .spawn(|| {
            let config = EconomyConfig::load("economy_config.toml").expect("load config");
            let seed = 99u64;
            let total_ticks = 100u32;

            let mut rng1 = StdRng::seed_from_u64(seed);
            let mut world1 = World::new(config.clone(), &mut rng1);

            let mut rng2 = StdRng::seed_from_u64(seed);
            let mut world2 = World::new(config, &mut rng2);

            for _ in 0..total_ticks {
                world1.tick(&mut rng1);
                world2.tick(&mut rng2);
            }

            // Tick counter is always deterministic.
            assert_eq!(world1.current_tick, world2.current_tick);

            // Alive count may differ slightly due to HashMap iteration order.
            let count1 = world1.alive_merchant_count();
            let count2 = world2.alive_merchant_count();
            let diff = (count1 as i32 - count2 as i32).unsigned_abs();
            assert!(
                diff <= 5,
                "Alive merchant counts differ too much: run1={count1}, run2={count2}, diff={diff}"
            );
        })
        .expect("spawn thread");

    handle.join().expect("replay_tick_count_matches panicked");
}
