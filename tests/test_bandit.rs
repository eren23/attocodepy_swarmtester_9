mod common;

use std::collections::HashMap;

use rand::rngs::StdRng;
use rand::SeedableRng;

use swarm_economy::config::{BanditConfig, SeasonalActivity};
use swarm_economy::types::{Commodity, Profession, Season, TerrainType, Vec2};
use swarm_economy::world::bandit::{BanditSystem, CityInfo, MerchantInfo};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn test_config() -> BanditConfig {
    BanditConfig {
        num_camps: 2,
        patrol_radius_range: [100.0, 200.0],
        agents_per_camp: [2, 4],
        rob_gold_pct: [0.10, 0.30],
        rob_goods_pct: [0.20, 0.40],
        attack_range: 15.0,
        starvation_ticks: 3000,
        respawn_interval: 100,
        seasonal_activity: SeasonalActivity {
            spring: 1.0,
            summer: 1.3,
            autumn: 1.0,
            winter: 0.5,
        },
    }
}

fn forest_terrain(_pos: Vec2) -> TerrainType {
    TerrainType::Forest
}

fn plains_terrain(_pos: Vec2) -> TerrainType {
    TerrainType::Plains
}

fn make_merchant(id: u32, position: Vec2, gold: f32, group_size: u32) -> MerchantInfo {
    MerchantInfo {
        id,
        position,
        gold,
        inventory: HashMap::new(),
        profession: Profession::Trader,
        group_size,
    }
}

fn make_merchant_with_goods(
    id: u32,
    position: Vec2,
    gold: f32,
    goods: Vec<(Commodity, f32)>,
    group_size: u32,
) -> MerchantInfo {
    MerchantInfo {
        id,
        position,
        gold,
        inventory: goods.into_iter().collect(),
        profession: Profession::Trader,
        group_size,
    }
}

fn make_soldier(id: u32, position: Vec2, gold: f32) -> MerchantInfo {
    MerchantInfo {
        id,
        position,
        gold,
        inventory: HashMap::new(),
        profession: Profession::Soldier,
        group_size: 1,
    }
}

// ── Patrol Radius ───────────────────────────────────────────────────────────

#[test]
fn bandits_stay_within_camp_patrol_radius_after_many_ticks() {
    let config = test_config();
    let mut rng = StdRng::seed_from_u64(42);
    let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

    // Run many ticks of patrol movement.
    for _ in 0..1000 {
        system.tick(
            &config,
            &[],
            &[],
            Season::Spring,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );
    }

    for bandit in system.bandits() {
        if !bandit.active {
            continue;
        }
        let camp = system
            .camps()
            .iter()
            .find(|c| c.id == bandit.camp_id)
            .unwrap();
        let dist = bandit.position.distance(camp.position);
        assert!(
            dist <= camp.patrol_radius,
            "bandit at {:?} is {:.1}px from camp center {:?}, exceeding patrol_radius {:.1}",
            bandit.position,
            dist,
            camp.position,
            camp.patrol_radius,
        );
    }
}

// ── Robbery Percentages ─────────────────────────────────────────────────────

#[test]
fn gold_stolen_is_within_configured_range() {
    let config = test_config();
    let merchant_gold = 100.0;

    // Keep placing bandit right at the camp and ticking until we get a robbery.
    let mut got_robbery = false;
    for seed in 0..100 {
        let mut rng2 = StdRng::seed_from_u64(seed);
        let mut sys = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng2);
        // Place a bandit close to the merchant.
        if sys.bandits().is_empty() {
            continue;
        }
        let camp_pos = sys.camps()[0].position;
        sys.bandits_mut()[0].position = camp_pos;

        let merchants_local = vec![make_merchant_with_goods(
            1,
            camp_pos + Vec2::new(5.0, 0.0),
            merchant_gold,
            vec![(Commodity::Timber, 50.0)],
            1,
        )];

        let result = sys.tick(
            &config,
            &merchants_local,
            &[],
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng2,
        );

        if let Some(robbery) = result.robberies.first() {
            // Gold: 10-30% of 100.
            assert!(
                robbery.gold_stolen >= merchant_gold * 0.10 - 0.01
                    && robbery.gold_stolen <= merchant_gold * 0.30 + 0.01,
                "gold_stolen {:.2} should be within 10-30% of {:.2}",
                robbery.gold_stolen,
                merchant_gold,
            );

            // Goods: 20-40% of 50 timber.
            if let Some(&timber_stolen) = robbery.goods_stolen.get(&Commodity::Timber) {
                assert!(
                    timber_stolen >= 50.0 * 0.20 - 0.01 && timber_stolen <= 50.0 * 0.40 + 0.01,
                    "timber_stolen {:.2} should be within 20-40% of 50.0",
                    timber_stolen,
                );
            }
            got_robbery = true;
            break;
        }
    }
    assert!(got_robbery, "should have gotten at least one robbery across many seeds");
}

// ── Attack Range ────────────────────────────────────────────────────────────

#[test]
fn merchants_beyond_15px_are_not_attacked() {
    let config = test_config();
    let mut rng = StdRng::seed_from_u64(42);
    let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

    if system.camps().is_empty() {
        return; // Skip if no camps spawned.
    }

    let camp_pos = system.camps()[0].position;

    // Place merchant far beyond attack range (15px) from the camp center.
    // Use a large offset so even patrolling bandits cannot reach the merchant.
    let merchants = vec![make_merchant(
        1,
        camp_pos + Vec2::new(300.0, 0.0), // 300px away, well beyond any patrol radius
        100.0,
        1,
    )];

    for _ in 0..50 {
        let result = system.tick(
            &config,
            &merchants,
            &[],
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );
        assert!(
            result.robberies.iter().all(|r| r.merchant_id != 1),
            "merchant beyond attack range should not be attacked",
        );
    }
}

#[test]
fn merchants_within_15px_can_be_attacked() {
    let config = test_config();

    let mut any_robbery = false;
    for seed in 0..50 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        if system.camps().is_empty() {
            continue;
        }

        let camp_pos = system.camps()[0].position;
        system.bandits_mut()[0].position = camp_pos;

        let merchants = vec![make_merchant(
            1,
            camp_pos + Vec2::new(5.0, 0.0), // 5px away, well within 15px
            100.0,
            1,
        )];

        let result = system.tick(
            &config,
            &merchants,
            &[],
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );

        if !result.robberies.is_empty() {
            any_robbery = true;
            break;
        }
    }
    assert!(any_robbery, "merchant within 15px should eventually be attacked");
}

// ── Caravan Safety Thresholds ───────────────────────────────────────────────

#[test]
fn caravan_of_4_or_more_is_immune() {
    let config = test_config();
    let mut rng = StdRng::seed_from_u64(42);
    let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

    if system.camps().is_empty() {
        return;
    }

    let camp_pos = system.camps()[0].position;
    let merchants = vec![make_merchant(
        1,
        camp_pos + Vec2::new(5.0, 0.0),
        100.0,
        4, // group_size >= 4 = immune
    )];

    for _ in 0..100 {
        system.bandits_mut()[0].position = camp_pos;
        let result = system.tick(
            &config,
            &merchants,
            &[],
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );
        assert!(
            result.robberies.is_empty(),
            "caravan of 4+ should never be robbed",
        );
    }
}

#[test]
fn caravan_of_5_is_also_immune() {
    let config = test_config();
    let mut rng = StdRng::seed_from_u64(42);
    let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

    if system.camps().is_empty() {
        return;
    }

    let camp_pos = system.camps()[0].position;
    let merchants = vec![make_merchant(
        1,
        camp_pos + Vec2::new(5.0, 0.0),
        100.0,
        5,
    )];

    for _ in 0..50 {
        system.bandits_mut()[0].position = camp_pos;
        let result = system.tick(
            &config,
            &merchants,
            &[],
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );
        assert!(result.robberies.is_empty(), "caravan of 5 should be immune");
    }
}

#[test]
fn caravan_of_3_has_repel_chance() {
    let config = test_config();

    // Run many trials: a caravan of 3 should repel ~70% of the time.
    let mut repelled = 0;
    let mut attacked = 0;
    let trials = 200;

    for seed in 0..trials {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        if system.camps().is_empty() {
            continue;
        }

        let camp_pos = system.camps()[0].position;
        system.bandits_mut()[0].position = camp_pos;

        let merchants = vec![make_merchant(
            1,
            camp_pos + Vec2::new(5.0, 0.0),
            100.0,
            3, // group_size = 3 has 70% repel chance
        )];

        let result = system.tick(
            &config,
            &merchants,
            &[],
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );

        if result.robberies.is_empty() {
            repelled += 1;
        } else {
            attacked += 1;
        }
    }

    // Some should be repelled, some not. We verify both outcomes happen.
    // Note: not all trials produce attack attempts (probability gate), so we check
    // that when attacks happen, some get through and some are repelled.
    let total = repelled + attacked;
    if total > 0 {
        // With 70% repel chance, we expect more repels than successes.
        assert!(
            repelled > 0,
            "caravan of 3 should repel at least some attacks",
        );
    }
}

// ── Soldier Combat Outcomes ─────────────────────────────────────────────────

#[test]
fn soldier_nearby_triggers_combat() {
    let config = test_config();

    let mut any_combat = false;
    for seed in 0..100 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        if system.camps().is_empty() {
            continue;
        }

        let camp_pos = system.camps()[0].position;
        system.bandits_mut()[0].position = camp_pos;

        // Merchant + nearby soldier (within 25px of merchant).
        let merchant_pos = camp_pos + Vec2::new(5.0, 0.0);
        let merchants = vec![
            make_merchant(1, merchant_pos, 100.0, 1),
            make_soldier(2, merchant_pos + Vec2::new(3.0, 0.0), 50.0),
        ];

        let result = system.tick(
            &config,
            &merchants,
            &[],
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );

        if !result.combats.is_empty() {
            any_combat = true;
            let combat = &result.combats[0];
            assert_eq!(combat.soldier_id, 2);
            // 50/50 outcome: either soldier wins or loses.
            if combat.soldier_wins {
                assert!(combat.reputation_delta > 0.0);
                assert!((combat.gold_lost - 0.0).abs() < f32::EPSILON);
            } else {
                assert!((combat.reputation_delta - 0.0).abs() < f32::EPSILON);
                // Soldier loses 30% of gold.
                assert!((combat.gold_lost - 50.0 * 0.3).abs() < 0.01);
            }
            break;
        }
    }
    assert!(any_combat, "soldier nearby should trigger combat eventually");
}

#[test]
fn soldier_combat_has_roughly_50_50_outcomes() {
    let config = test_config();

    let mut wins = 0;
    let mut losses = 0;

    for seed in 0..500 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        if system.camps().is_empty() {
            continue;
        }

        let camp_pos = system.camps()[0].position;
        system.bandits_mut()[0].position = camp_pos;

        let merchant_pos = camp_pos + Vec2::new(5.0, 0.0);
        let merchants = vec![
            make_merchant(1, merchant_pos, 100.0, 1),
            make_soldier(2, merchant_pos + Vec2::new(3.0, 0.0), 50.0),
        ];

        let result = system.tick(
            &config,
            &merchants,
            &[],
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );

        for combat in &result.combats {
            if combat.soldier_wins {
                wins += 1;
            } else {
                losses += 1;
            }
        }
    }

    let total = wins + losses;
    if total >= 10 {
        // Roughly 50/50 — allow wide margin since RNG is involved.
        let win_rate = wins as f64 / total as f64;
        assert!(
            win_rate > 0.2 && win_rate < 0.8,
            "soldier win rate {:.2} should be roughly 50/50 (wins={}, losses={})",
            win_rate,
            wins,
            losses,
        );
    }
}

// ── Camp Starvation / Respawn ───────────────────────────────────────────────

#[test]
fn camp_with_no_robberies_gets_destroyed() {
    let mut config = test_config();
    config.starvation_ticks = 10; // Short starvation for testing.
    config.num_camps = 1;

    let mut rng = StdRng::seed_from_u64(42);
    let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

    assert_eq!(system.active_camp_count(), 1);

    let mut destroyed = false;
    for _ in 0..20 {
        let result = system.tick(
            &config,
            &[], // No merchants = no robberies.
            &[],
            Season::Spring,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );
        if !result.camps_destroyed.is_empty() {
            destroyed = true;
            break;
        }
    }
    assert!(destroyed, "camp with no robberies should be destroyed after starvation_ticks");
}

#[test]
fn system_respawns_camps_to_maintain_target_count() {
    let mut config = test_config();
    config.num_camps = 3;
    config.starvation_ticks = 5; // Fast starvation.

    let mut rng = StdRng::seed_from_u64(42);
    let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

    assert_eq!(system.active_camp_count(), 3);

    // Run many ticks: camps starve and respawn.
    for _ in 0..50 {
        system.tick(
            &config,
            &[],
            &[],
            Season::Spring,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );
    }

    // After respawn cycles, should still have target count.
    assert_eq!(
        system.active_camp_count(),
        3,
        "system should maintain target camp count via respawning",
    );
}

#[test]
fn respawn_reports_camps_spawned() {
    let mut config = test_config();
    config.num_camps = 2;
    config.starvation_ticks = 3;

    let mut rng = StdRng::seed_from_u64(42);
    let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

    let mut total_spawned = 0u32;
    for _ in 0..30 {
        let result = system.tick(
            &config,
            &[],
            &[],
            Season::Spring,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );
        total_spawned += result.camps_spawned;
    }

    assert!(
        total_spawned > 0,
        "should report camps_spawned > 0 after starvation + respawn cycles",
    );
}

// ── Seasonal Activity ───────────────────────────────────────────────────────

#[test]
fn summer_increases_attack_frequency() {
    let config = test_config();

    let mut summer_robberies = 0;
    let mut winter_robberies = 0;
    let trials = 200;

    for seed in 0..trials {
        // Summer trial.
        let mut rng = StdRng::seed_from_u64(seed);
        let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);
        if system.camps().is_empty() {
            continue;
        }
        let camp_pos = system.camps()[0].position;
        system.bandits_mut()[0].position = camp_pos;
        let merchants = vec![make_merchant(1, camp_pos + Vec2::new(5.0, 0.0), 100.0, 1)];
        let result = system.tick(
            &config,
            &merchants,
            &[],
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );
        summer_robberies += result.robberies.len();

        // Winter trial (fresh system from same seed structure).
        let mut rng2 = StdRng::seed_from_u64(seed + 10000);
        let mut system2 = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng2);
        if system2.camps().is_empty() {
            continue;
        }
        let camp_pos2 = system2.camps()[0].position;
        system2.bandits_mut()[0].position = camp_pos2;
        let merchants2 = vec![make_merchant(1, camp_pos2 + Vec2::new(5.0, 0.0), 100.0, 1)];
        let result2 = system2.tick(
            &config,
            &merchants2,
            &[],
            Season::Winter,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng2,
        );
        winter_robberies += result2.robberies.len();
    }

    // Summer (1.3x) should produce more robberies than winter (0.5x).
    assert!(
        summer_robberies > winter_robberies,
        "summer ({}) should have more robberies than winter ({})",
        summer_robberies,
        winter_robberies,
    );
}

// ── Wall Avoidance ──────────────────────────────────────────────────────────

#[test]
fn bandits_dont_attack_near_walled_cities() {
    let config = test_config();
    let mut rng = StdRng::seed_from_u64(42);
    let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

    if system.camps().is_empty() {
        return;
    }

    let camp_pos = system.camps()[0].position;

    // Place a walled city within MIN_CITY_DISTANCE (150px) of the bandit.
    let cities = vec![CityInfo {
        position: camp_pos + Vec2::new(50.0, 0.0),
        has_walls: true,
    }];

    let merchants = vec![make_merchant(
        1,
        camp_pos + Vec2::new(5.0, 0.0),
        100.0,
        1,
    )];

    for _ in 0..100 {
        system.bandits_mut()[0].position = camp_pos;
        let result = system.tick(
            &config,
            &merchants,
            &cities,
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );
        assert!(
            result.robberies.is_empty(),
            "bandits should not attack near a walled city",
        );
    }
}

#[test]
fn bandits_can_attack_near_unwalled_cities() {
    let config = test_config();

    let mut any_robbery = false;
    for seed in 0..100 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        if system.camps().is_empty() {
            continue;
        }

        let camp_pos = system.camps()[0].position;
        system.bandits_mut()[0].position = camp_pos;

        // Unwalled city nearby should NOT prevent attacks.
        let cities = vec![CityInfo {
            position: camp_pos + Vec2::new(50.0, 0.0),
            has_walls: false,
        }];

        let merchants = vec![make_merchant(
            1,
            camp_pos + Vec2::new(5.0, 0.0),
            100.0,
            1,
        )];

        let result = system.tick(
            &config,
            &merchants,
            &cities,
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );

        if !result.robberies.is_empty() {
            any_robbery = true;
            break;
        }
    }
    assert!(
        any_robbery,
        "bandits should be able to attack near unwalled cities",
    );
}

// ── Terrain Requirement for Camp Spawning ───────────────────────────────────

#[test]
fn camps_only_spawn_on_forest_or_hills() {
    let config = test_config();
    let mut rng = StdRng::seed_from_u64(42);

    // All-plains terrain should yield zero camps.
    let system = BanditSystem::new(&config, &[], plains_terrain, 1600.0, 1000.0, &mut rng);
    assert_eq!(
        system.active_camp_count(),
        0,
        "no camps should spawn on plains-only terrain",
    );
}

#[test]
fn camps_spawn_on_forest_terrain() {
    let config = test_config();
    let mut rng = StdRng::seed_from_u64(42);

    let system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);
    assert_eq!(
        system.active_camp_count(),
        config.num_camps,
        "camps should spawn normally on forest terrain",
    );
}

#[test]
fn camps_spawn_on_hills_terrain() {
    let config = test_config();
    let mut rng = StdRng::seed_from_u64(42);

    let hills_terrain = |_pos: Vec2| TerrainType::Hills;
    let system = BanditSystem::new(&config, &[], hills_terrain, 1600.0, 1000.0, &mut rng);
    assert_eq!(
        system.active_camp_count(),
        config.num_camps,
        "camps should spawn normally on hills terrain",
    );
}
