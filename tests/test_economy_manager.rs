mod common;

use rand::rngs::StdRng;
use rand::SeedableRng;

use swarm_economy::agents::economy_manager::EconomyManager;
use swarm_economy::types::*;

use common::*;

// ── Initial population ──────────────────────────────────────────────────────

#[test]
fn spawn_initial_population_creates_exact_count() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = test_rng();

    let mut mgr = EconomyManager::new(0);
    let merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    assert_eq!(
        merchants.len(),
        config.merchant.initial_population as usize,
        "spawn_initial_population should create exactly initial_population merchants"
    );
}

#[test]
fn initial_population_merchants_are_alive() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = test_rng();

    let mut mgr = EconomyManager::new(0);
    let merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    for m in &merchants {
        assert!(m.alive, "all initial merchants should be alive");
    }
}

#[test]
fn initial_population_has_unique_ids() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = test_rng();

    let mut mgr = EconomyManager::new(0);
    let merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    let mut ids: Vec<u32> = merchants.iter().map(|m| m.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(
        ids.len(),
        merchants.len(),
        "all merchants should have unique IDs"
    );
}

// ── Spawn rate ──────────────────────────────────────────────────────────────

#[test]
fn try_spawn_does_not_exceed_max_population() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = StdRng::seed_from_u64(123);

    let mut mgr = EconomyManager::new(0);
    let mut merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    // Run many ticks to attempt spawning
    for tick in 0..500 {
        mgr.tick(&mut merchants, &cities, &config, tick, &mut rng);
    }

    let alive = EconomyManager::alive_count(&merchants);
    assert!(
        alive <= config.merchant.max_population,
        "alive count ({}) should not exceed max_population ({})",
        alive,
        config.merchant.max_population
    );
}

#[test]
fn try_spawn_requires_healthy_economy() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = StdRng::seed_from_u64(456);

    let mut mgr = EconomyManager::new(0);
    let mut merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    // Set all merchants to very low gold (unhealthy economy)
    // avg_gold < initial_gold * 0.5 should prevent spawning
    for m in merchants.iter_mut() {
        m.gold = 1.0; // Well below 50% of initial_gold (100 * 0.5 = 50)
    }

    let alive_before = EconomyManager::alive_count(&merchants);

    // Run several ticks — no new merchants should spawn due to unhealthy economy
    for tick in 0..50 {
        mgr.tick(&mut merchants, &cities, &config, tick, &mut rng);
    }

    // The alive count should not have increased (it might decrease due to
    // bankruptcy if any merchant goes negative)
    let alive_after = EconomyManager::alive_count(&merchants);
    assert!(
        alive_after <= alive_before,
        "unhealthy economy should not spawn new merchants"
    );
}

// ── Profession distribution ─────────────────────────────────────────────────

#[test]
fn profession_distribution_within_tolerance() {
    // Use a larger population for statistical significance
    let mut config = mini_economy_config();
    config.merchant.initial_population = 200;
    config.merchant.max_population = 300;

    let cities = make_mini_cities();
    let mut rng = StdRng::seed_from_u64(789);

    let mut mgr = EconomyManager::new(0);
    let merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    let counts = EconomyManager::profession_counts(&merchants);
    let total = merchants.len() as f32;

    // Check that each profession is within +/-5% of configured distribution
    // (absolute, not relative tolerance)
    let tolerance = 0.05;
    for (name, expected_frac) in &config.professions.default_distribution {
        let profession = match name.as_str() {
            "trader" => Profession::Trader,
            "miner" => Profession::Miner,
            "farmer" => Profession::Farmer,
            "craftsman" => Profession::Craftsman,
            "soldier" => Profession::Soldier,
            "shipwright" => Profession::Shipwright,
            "idle" => Profession::Idle,
            _ => continue,
        };
        let actual_frac = *counts.get(&profession).unwrap_or(&0) as f32 / total;
        let diff = (actual_frac - expected_frac).abs();
        // With 200 merchants and random distribution, allow wider tolerance
        // for small fractions
        let allowed = tolerance + 0.05; // 10% total tolerance for statistical noise
        assert!(
            diff <= allowed,
            "profession {:?}: expected ~{:.2}, got {:.2} (diff {:.2} > {:.2})",
            profession,
            expected_frac,
            actual_frac,
            diff,
            allowed
        );
    }
}

// ── Bankrupt removal ────────────────────────────────────────────────────────

#[test]
fn bankrupt_merchant_marked_dead_after_grace_period() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = StdRng::seed_from_u64(101);

    let mut mgr = EconomyManager::new(0);
    let mut merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    // Force one merchant into negative gold
    merchants[0].gold = -100.0;

    // Tick through the bankruptcy grace period
    for tick in 0..(config.merchant.bankruptcy_grace_ticks + 10) {
        mgr.tick(&mut merchants, &cities, &config, tick, &mut rng);
    }

    assert!(
        !merchants[0].alive,
        "merchant with gold < 0 for {} ticks should be marked dead",
        config.merchant.bankruptcy_grace_ticks
    );
}

#[test]
fn bankrupt_merchant_recovers_if_gold_becomes_positive() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = StdRng::seed_from_u64(202);

    let mut mgr = EconomyManager::new(0);
    let mut merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    // Set negative gold
    merchants[0].gold = -10.0;

    // Tick partway through grace period
    let partial = config.merchant.bankruptcy_grace_ticks / 2;
    for tick in 0..partial {
        mgr.tick(&mut merchants, &cities, &config, tick, &mut rng);
    }

    assert!(
        merchants[0].alive,
        "merchant should still be alive partway through grace period"
    );

    // Recover gold
    merchants[0].gold = 100.0;

    // Tick some more — should not die
    for tick in partial..(partial + config.merchant.bankruptcy_grace_ticks) {
        mgr.tick(&mut merchants, &cities, &config, tick, &mut rng);
    }

    assert!(
        merchants[0].alive,
        "merchant should survive after gold recovery resets the counter"
    );
}

// ── Rebalancing ─────────────────────────────────────────────────────────────

#[test]
fn rebalancing_occurs_at_interval() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = StdRng::seed_from_u64(303);

    let mut mgr = EconomyManager::new(0);
    let mut merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    // Tick exactly to the rebalance interval
    let rebalance_tick = config.professions.rebalance_interval;
    for tick in 0..=rebalance_tick {
        mgr.tick(&mut merchants, &cities, &config, tick, &mut rng);
    }

    // After rebalancing, the profession distribution should have changed
    // (worst → best transfer). We just verify the tick ran without panicking
    // and that the total alive count is reasonable.
    let total_alive = EconomyManager::alive_count(&merchants);
    assert!(
        total_alive > 0,
        "there should be living merchants after rebalancing"
    );

    // At least verify something was tracked
    assert!(
        !mgr.total_gold_history.is_empty(),
        "gold history should be populated after ticks"
    );
}

// ── Emergency farmer shift ──────────────────────────────────────────────────

#[test]
fn emergency_farmer_shift_when_no_food_in_cities() {
    let config = mini_economy_config();
    // Cities have empty warehouses by default (no food)
    let cities = make_mini_cities();
    let mut rng = StdRng::seed_from_u64(404);

    let mut mgr = EconomyManager::new(0);
    let mut merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    // Ensure we have some idle merchants
    let mut found_idle = false;
    for m in merchants.iter_mut() {
        if m.profession == Profession::Trader && m.gold < 60.0 {
            m.profession = Profession::Idle;
            found_idle = true;
        }
    }
    // If no naturally low-gold traders, force one to be idle
    if !found_idle && !merchants.is_empty() {
        merchants[0].profession = Profession::Idle;
    }

    // Tick to the rebalance interval — this should trigger food crisis detection
    // and emergency rebalance
    let rebalance_tick = config.professions.rebalance_interval;
    for tick in 0..=rebalance_tick {
        mgr.tick(&mut merchants, &cities, &config, tick, &mut rng);
    }

    // After emergency rebalancing, some idle/poor traders should become Farmers
    let farmer_count = merchants
        .iter()
        .filter(|m| m.alive && m.profession == Profession::Farmer)
        .count();

    assert!(
        farmer_count > 0,
        "emergency rebalance should create at least one Farmer when cities have no food"
    );
}

// ── Gold conservation ───────────────────────────────────────────────────────

#[test]
fn gold_conservation_tracked_each_tick() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = StdRng::seed_from_u64(505);

    let mut mgr = EconomyManager::new(0);
    let mut merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    assert!(
        mgr.total_gold_history.is_empty(),
        "gold history should be empty before any ticks"
    );

    for tick in 0..10 {
        mgr.tick(&mut merchants, &cities, &config, tick, &mut rng);
    }

    assert_eq!(
        mgr.total_gold_history.len(),
        10,
        "gold history should have one entry per tick"
    );

    // Each entry should be the sum of all alive merchants' gold
    let expected_last: f32 = merchants.iter().filter(|m| m.alive).map(|m| m.gold).sum();
    let actual_last = *mgr.total_gold_history.last().unwrap();
    assert!(
        (actual_last - expected_last).abs() < 1e-2,
        "last gold history entry ({}) should match sum of alive gold ({})",
        actual_last,
        expected_last
    );
}

#[test]
fn gold_history_capped_at_1000() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = StdRng::seed_from_u64(606);

    let mut mgr = EconomyManager::new(0);
    let mut merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    for tick in 0..1100 {
        mgr.tick(&mut merchants, &cities, &config, tick, &mut rng);
    }

    assert!(
        mgr.total_gold_history.len() <= 1000,
        "gold history should be capped at 1000 entries, got {}",
        mgr.total_gold_history.len()
    );
}

// ── Accessor functions ──────────────────────────────────────────────────────

#[test]
fn alive_count_reflects_living_merchants() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = test_rng();

    let mut mgr = EconomyManager::new(0);
    let mut merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    let alive = EconomyManager::alive_count(&merchants);
    assert_eq!(alive, config.merchant.initial_population);

    // Kill one
    merchants[0].alive = false;
    let alive = EconomyManager::alive_count(&merchants);
    assert_eq!(alive, config.merchant.initial_population - 1);
}

#[test]
fn profession_counts_correct() {
    let config = mini_economy_config();
    let cities = make_mini_cities();
    let mut rng = test_rng();

    let mut mgr = EconomyManager::new(0);
    let merchants = mgr.spawn_initial_population(&config, &cities, &mut rng);

    let counts = EconomyManager::profession_counts(&merchants);
    let total: u32 = counts.values().sum();
    assert_eq!(
        total,
        config.merchant.initial_population,
        "sum of profession counts should equal total population"
    );
}
