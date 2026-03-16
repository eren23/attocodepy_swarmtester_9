mod common;

use swarm_economy::config::UpgradeCosts;
use swarm_economy::types::{CityUpgrade, Commodity, Vec2};
use swarm_economy::world::city::City;

use common::{mini_city_config, make_city};

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Build a city with known deterministic fields so tests are not seed-dependent.
fn deterministic_city() -> City {
    let mut city = make_city(0, Vec2::new(32.0, 32.0));
    // Reset to known baseline for predictable tests.
    city.population = 100.0;
    city.tax_rate = 0.05;
    city.treasury = 0.0;
    city.warehouse.clear();
    city.prosperity = 50.0;
    city.ticks_without_food = 0;
    city.trade_volume = 0.0;
    city
}

fn test_upgrade_costs() -> UpgradeCosts {
    UpgradeCosts {
        market_hall: 500.0,
        walls: 800.0,
        harbor: 1000.0,
        workshop: 600.0,
    }
}

// ── Population growth ───────────────────────────────────────────────────────

#[test]
fn population_grows_when_prosperous_and_food_stocked() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    city.prosperity = 70.0;
    city.warehouse.insert(Commodity::Grain, 10.0);

    let before = city.population;
    city.tick_population(&config);
    assert!(
        (city.population - (before + 0.01)).abs() < 1e-5,
        "expected +0.01 growth, got delta {}",
        city.population - before
    );
}

#[test]
fn population_grows_with_fish_as_food() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    city.prosperity = 70.0;
    city.warehouse.insert(Commodity::Fish, 5.0);

    let before = city.population;
    city.tick_population(&config);
    assert!((city.population - (before + 0.01)).abs() < 1e-5);
}

#[test]
fn population_grows_with_provisions_as_food() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    city.prosperity = 70.0;
    city.warehouse.insert(Commodity::Provisions, 5.0);

    let before = city.population;
    city.tick_population(&config);
    assert!((city.population - (before + 0.01)).abs() < 1e-5);
}

#[test]
fn population_does_not_grow_when_prosperity_too_low() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    city.prosperity = 50.0; // <= 60
    city.warehouse.insert(Commodity::Grain, 10.0);

    let before = city.population;
    city.tick_population(&config);
    assert!(
        (city.population - before).abs() < 1e-5,
        "no growth expected when prosperity <= 60"
    );
}

#[test]
fn population_does_not_grow_without_food() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    city.prosperity = 80.0;
    // No food commodities in warehouse.

    let before = city.population;
    city.tick_population(&config);
    // No growth because no food (even though prosperity > 60).
    assert!(
        (city.population - before).abs() < 1e-5,
        "no growth without food, even with high prosperity"
    );
}

// ── Population decline ──────────────────────────────────────────────────────

#[test]
fn population_declines_after_200_ticks_without_food() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    city.ticks_without_food = 199;
    // No food in warehouse.

    let before = city.population;
    city.tick_population(&config); // ticks_without_food becomes 200
    assert!(
        (city.population - (before - 0.02)).abs() < 1e-5,
        "expected -0.02 decline, got delta {}",
        city.population - before
    );
}

#[test]
fn population_does_not_decline_before_200_ticks_without_food() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    city.ticks_without_food = 198;

    let before = city.population;
    city.tick_population(&config); // ticks_without_food becomes 199, not yet 200
    assert!(
        (city.population - before).abs() < 1e-5,
        "no decline before 200 consecutive ticks without food"
    );
}

#[test]
fn food_resets_ticks_without_food_counter() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    city.ticks_without_food = 199;
    city.warehouse.insert(Commodity::Fish, 1.0);

    city.tick_population(&config);
    assert_eq!(
        city.ticks_without_food, 0,
        "having food should reset the counter"
    );
}

// ── Tax adjustment ──────────────────────────────────────────────────────────

#[test]
fn tax_increases_when_trade_above_average() {
    let mut city = deterministic_city();
    city.tax_rate = 0.05;
    city.trade_volume = 200.0;

    city.tick_tax_adjustment(500, 100.0); // above average
    assert!(
        (city.tax_rate - 0.06).abs() < 1e-5,
        "tax should increase by 0.01"
    );
}

#[test]
fn tax_decreases_when_trade_below_average() {
    let mut city = deterministic_city();
    city.tax_rate = 0.05;
    city.trade_volume = 50.0;

    city.tick_tax_adjustment(500, 100.0); // below average
    assert!(
        (city.tax_rate - 0.04).abs() < 1e-5,
        "tax should decrease by 0.01"
    );
}

#[test]
fn tax_unchanged_when_trade_equals_average() {
    let mut city = deterministic_city();
    city.tax_rate = 0.05;
    city.trade_volume = 100.0;

    city.tick_tax_adjustment(500, 100.0); // equal to average
    assert!(
        (city.tax_rate - 0.05).abs() < 1e-5,
        "tax should remain unchanged when volume equals average"
    );
}

#[test]
fn tax_adjustment_only_at_tick_500_multiples() {
    let mut city = deterministic_city();
    city.tax_rate = 0.05;
    city.trade_volume = 200.0;

    city.tick_tax_adjustment(499, 100.0);
    assert!(
        (city.tax_rate - 0.05).abs() < 1e-5,
        "no adjustment at tick 499"
    );

    city.tick_tax_adjustment(501, 100.0);
    assert!(
        (city.tax_rate - 0.05).abs() < 1e-5,
        "no adjustment at tick 501"
    );
}

#[test]
fn tax_adjustment_resets_trade_volume() {
    let mut city = deterministic_city();
    city.trade_volume = 200.0;

    city.tick_tax_adjustment(500, 100.0);
    assert!(
        city.trade_volume.abs() < 1e-5,
        "trade_volume should reset to 0 after adjustment"
    );
}

// ── Tax clamping ────────────────────────────────────────────────────────────

#[test]
fn tax_clamped_at_upper_bound() {
    let mut city = deterministic_city();
    city.tax_rate = 0.15;
    city.trade_volume = 999.0;

    city.tick_tax_adjustment(500, 0.0); // above average => try to increase
    assert!(
        city.tax_rate <= 0.15 + 1e-5,
        "tax must not exceed 0.15, got {}",
        city.tax_rate
    );
}

#[test]
fn tax_clamped_at_lower_bound() {
    let mut city = deterministic_city();
    city.tax_rate = 0.0;
    city.trade_volume = 0.0;

    city.tick_tax_adjustment(500, 100.0); // below average => try to decrease
    assert!(
        city.tax_rate >= -1e-5,
        "tax must not go below 0.0, got {}",
        city.tax_rate
    );
}

#[test]
fn tax_stays_in_range_after_many_adjustments() {
    let mut city = deterministic_city();
    city.tax_rate = 0.10;

    // Repeatedly push up.
    for i in 1..=20 {
        city.trade_volume = 999.0;
        city.tick_tax_adjustment(i * 500, 0.0);
    }
    assert!(city.tax_rate >= 0.0 && city.tax_rate <= 0.15);

    // Repeatedly push down.
    for i in 21..=40 {
        city.trade_volume = 0.0;
        city.tick_tax_adjustment(i * 500, 999.0);
    }
    assert!(city.tax_rate >= 0.0 && city.tax_rate <= 0.15);
}

// ── Upgrade purchase ────────────────────────────────────────────────────────

#[test]
fn upgrade_purchase_deducts_cost_and_adds_upgrade() {
    let mut city = deterministic_city();
    let costs = test_upgrade_costs();
    city.treasury = 1000.0;

    assert!(city.try_purchase_upgrade(CityUpgrade::MarketHall, &costs));
    assert!(
        (city.treasury - 500.0).abs() < 1e-5,
        "should deduct 500.0 for MarketHall"
    );
    assert!(city.upgrades.contains(&CityUpgrade::MarketHall));
}

#[test]
fn upgrade_purchase_fails_if_already_owned() {
    let mut city = deterministic_city();
    let costs = test_upgrade_costs();
    city.treasury = 2000.0;
    city.upgrades.insert(CityUpgrade::Walls);

    let result = city.try_purchase_upgrade(CityUpgrade::Walls, &costs);
    assert!(!result, "cannot purchase an already-owned upgrade");
    assert!(
        (city.treasury - 2000.0).abs() < 1e-5,
        "treasury unchanged on failed purchase"
    );
}

#[test]
fn upgrade_purchase_fails_if_insufficient_treasury() {
    let mut city = deterministic_city();
    let costs = test_upgrade_costs();
    city.treasury = 100.0;

    let result = city.try_purchase_upgrade(CityUpgrade::Harbor, &costs);
    assert!(!result, "cannot afford harbor at 1000.0 with 100.0 treasury");
    assert!(
        (city.treasury - 100.0).abs() < 1e-5,
        "treasury unchanged on failed purchase"
    );
}

#[test]
fn upgrade_purchase_succeeds_at_exact_cost() {
    let mut city = deterministic_city();
    let costs = test_upgrade_costs();
    city.treasury = 600.0;

    assert!(city.try_purchase_upgrade(CityUpgrade::Workshop, &costs));
    assert!(
        city.treasury.abs() < 1e-5,
        "treasury should be zero after spending exactly the cost"
    );
}

// ── Upgrade effects: crafting speed ─────────────────────────────────────────

#[test]
fn crafting_speed_base_is_one() {
    let city = deterministic_city();
    // Pick a commodity that is not the city's specialization.
    let non_spec = Commodity::ALL
        .iter()
        .copied()
        .find(|&c| c != city.specialization)
        .unwrap();

    assert!(
        (city.crafting_speed(non_spec) - 1.0).abs() < 1e-5,
        "non-specialized, no workshop => 1.0x"
    );
}

#[test]
fn crafting_speed_specialization_bonus() {
    let city = deterministic_city();
    let spec = city.specialization;

    assert!(
        (city.crafting_speed(spec) - 1.5).abs() < 1e-5,
        "specialization gives 1.5x, got {}",
        city.crafting_speed(spec)
    );
}

#[test]
fn crafting_speed_workshop_bonus() {
    let mut city = deterministic_city();
    city.upgrades.insert(CityUpgrade::Workshop);

    let non_spec = Commodity::ALL
        .iter()
        .copied()
        .find(|&c| c != city.specialization)
        .unwrap();

    assert!(
        (city.crafting_speed(non_spec) - 1.25).abs() < 1e-5,
        "workshop alone gives 1.25x, got {}",
        city.crafting_speed(non_spec)
    );
}

#[test]
fn crafting_speed_specialization_and_workshop_stacks() {
    let mut city = deterministic_city();
    city.upgrades.insert(CityUpgrade::Workshop);
    let spec = city.specialization;

    let expected = 1.5 * 1.25; // 1.875
    assert!(
        (city.crafting_speed(spec) - expected).abs() < 1e-5,
        "specialization + workshop = 1.875x, got {}",
        city.crafting_speed(spec)
    );
}

// ── Warehouse decay ─────────────────────────────────────────────────────────

#[test]
fn warehouse_decays_when_over_capacity() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    // Fill warehouse well above capacity (200.0).
    city.warehouse.insert(Commodity::Grain, 150.0);
    city.warehouse.insert(Commodity::Ore, 100.0);
    // Total = 250 > capacity 200.

    city.tick_warehouse(&config);

    let grain = *city.warehouse.get(&Commodity::Grain).unwrap_or(&0.0);
    let ore = *city.warehouse.get(&Commodity::Ore).unwrap_or(&0.0);

    let decay = config.warehouse_decay_rate; // 0.001
    assert!(
        (grain - 150.0 * (1.0 - decay)).abs() < 1e-3,
        "grain should decay by decay_rate"
    );
    assert!(
        (ore - 100.0 * (1.0 - decay)).abs() < 1e-3,
        "ore should decay by decay_rate"
    );
}

#[test]
fn warehouse_no_decay_under_capacity() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    city.warehouse.insert(Commodity::Grain, 50.0);
    city.warehouse.insert(Commodity::Ore, 50.0);
    // Total = 100 < capacity 200.

    city.tick_warehouse(&config);

    let grain = *city.warehouse.get(&Commodity::Grain).unwrap_or(&0.0);
    assert!(
        (grain - 50.0).abs() < 1e-5,
        "no decay when under capacity"
    );
}

#[test]
fn warehouse_decay_removes_negligible_entries() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    // Put a tiny amount plus enough to exceed capacity.
    city.warehouse.insert(Commodity::Grain, 250.0);
    city.warehouse.insert(Commodity::Herbs, 0.0005); // below 0.001 threshold

    // Trigger many decay ticks to erode the tiny entry further.
    // With total > capacity, the herbs entry (0.0005 * (1 - 0.001)) = ~0.0004995
    // which is < 0.001 and should be removed.
    city.tick_warehouse(&config);
    assert!(
        !city.warehouse.contains_key(&Commodity::Herbs)
            || *city.warehouse.get(&Commodity::Herbs).unwrap() < 0.001,
        "negligible entries should be removed"
    );
}

// ── Prosperity formula ──────────────────────────────────────────────────────

#[test]
fn prosperity_zero_when_empty_city() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    city.population = 0.0;
    city.trade_volume = 0.0;
    city.warehouse.clear();

    city.compute_prosperity(&config);
    assert!(
        city.prosperity.abs() < 1e-3,
        "empty city should have ~0 prosperity, got {}",
        city.prosperity
    );
}

#[test]
fn prosperity_max_at_full_stats() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    city.population = config.population_range[1] as f32; // max pop
    city.trade_volume = 100.0; // saturates trade score
    // Fill warehouse to capacity with all commodity types.
    for &c in &Commodity::ALL {
        city.warehouse
            .insert(c, config.warehouse_capacity / Commodity::ALL.len() as f32);
    }

    city.compute_prosperity(&config);
    assert!(
        (city.prosperity - 100.0).abs() < 1.0,
        "fully saturated city should have ~100 prosperity, got {}",
        city.prosperity
    );
}

#[test]
fn prosperity_clamped_to_zero_to_hundred() {
    let config = mini_city_config();
    let mut city = deterministic_city();

    // Even with extreme values the result stays in [0, 100].
    city.population = 9999.0; // way above max
    city.trade_volume = 99999.0;
    for &c in &Commodity::ALL {
        city.warehouse.insert(c, 99999.0);
    }
    city.compute_prosperity(&config);
    assert!(city.prosperity <= 100.0);
    assert!(city.prosperity >= 0.0);
}

#[test]
fn prosperity_sum_of_four_components() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    let pop_max = config.population_range[1] as f32;
    city.population = pop_max / 2.0; // pop_score = 12.5
    city.trade_volume = 50.0; // trade_score = (50/100)*25 = 12.5
    city.warehouse.insert(Commodity::Grain, config.warehouse_capacity / 2.0); // fullness = 0.5 => 12.5
    // 1 commodity out of 22 => diversity_score = (1/22)*25 ~= 1.136

    city.compute_prosperity(&config);

    let expected_pop = (city.population / pop_max) * 25.0;
    let expected_trade = (city.trade_volume / 100.0).min(1.0) * 25.0;
    let total_warehouse: f32 = city.warehouse.values().sum();
    let expected_fullness = (total_warehouse / config.warehouse_capacity).min(1.0) * 25.0;
    let expected_diversity =
        (city.warehouse.len() as f32 / Commodity::ALL.len() as f32).min(1.0) * 25.0;
    let expected =
        (expected_pop + expected_trade + expected_fullness + expected_diversity).clamp(0.0, 100.0);

    assert!(
        (city.prosperity - expected).abs() < 0.1,
        "prosperity {} should match computed {}",
        city.prosperity,
        expected
    );
}

// ── Record trade ────────────────────────────────────────────────────────────

#[test]
fn record_trade_accumulates_volume_and_tax() {
    let mut city = deterministic_city();
    city.tax_rate = 0.10;
    city.treasury = 0.0;
    city.trade_volume = 0.0;

    city.record_trade(100.0);
    assert!((city.trade_volume - 100.0).abs() < 1e-5);
    assert!(
        (city.treasury - 10.0).abs() < 1e-5,
        "tax revenue = 100 * 0.10 = 10.0"
    );

    city.record_trade(50.0);
    assert!((city.trade_volume - 150.0).abs() < 1e-5);
    assert!((city.treasury - 15.0).abs() < 1e-5);
}

// ── Population clamp ────────────────────────────────────────────────────────

#[test]
fn population_clamped_to_config_range() {
    let config = mini_city_config();
    let mut city = deterministic_city();
    let pop_min = config.population_range[0] as f32;
    let pop_max = config.population_range[1] as f32;

    // Force population above max, then tick.
    city.population = pop_max + 10.0;
    city.prosperity = 80.0;
    city.warehouse.insert(Commodity::Grain, 10.0);
    city.tick_population(&config);
    assert!(
        city.population <= pop_max + 1e-5,
        "population should be clamped to max"
    );

    // Force population below min, then tick with decline conditions.
    city.population = pop_min;
    city.warehouse.clear();
    city.ticks_without_food = 200;
    city.tick_population(&config);
    assert!(
        city.population >= pop_min - 1e-5,
        "population should be clamped to min"
    );
}
