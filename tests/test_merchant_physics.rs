mod common;

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use swarm_economy::agents::actions::MerchantAction;
use swarm_economy::types::*;

use common::*;

// ── Movement formula ────────────────────────────────────────────────────────

#[test]
fn movement_straight_line_on_plains() {
    // A merchant heading east (0 rad) on plains with no road should move
    // exactly base_speed * speed_mult * terrain(1.0) * road(1.0) * fatigue(1.0) * season(1.0).
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();
    let config = mini_merchant_config();

    let start = find_passable_pos(&terrain);
    let mut m = make_merchant_at(start, Profession::Trader);
    m.heading = 0.0; // east
    m.fatigue = 0.0;

    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    let expected_speed = config.base_speed; // 1.5 * 1.0 * 1.0 * 1.0 * 1.0 * 1.0
    let expected_x = start.x + expected_speed;

    assert!(
        (m.pos.x - expected_x).abs() < 0.1,
        "expected x ~{expected_x}, got {}",
        m.pos.x
    );
    assert!(
        (m.pos.y - start.y).abs() < 0.1,
        "y should not change when heading east"
    );
}

#[test]
fn movement_half_speed() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();
    let config = mini_merchant_config();

    let start = find_passable_pos(&terrain);
    let mut m = make_merchant_at(start, Profession::Trader);
    m.heading = 0.0;
    m.fatigue = 0.0;

    let action = MerchantAction::movement(0.0, 0.5);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    let expected_speed = config.base_speed * 0.5;
    let expected_x = start.x + expected_speed;

    assert!(
        (m.pos.x - expected_x).abs() < 0.1,
        "expected x ~{expected_x} at half speed, got {}",
        m.pos.x
    );
}

#[test]
fn movement_with_fatigue_reduces_speed() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();
    let config = mini_merchant_config();

    let start = find_passable_pos(&terrain);
    let mut m = make_merchant_at(start, Profession::Trader);
    m.heading = 0.0;
    m.fatigue = 100.0; // fatigue_mult = max(0.3, 1.0 - 100/200) = 0.5

    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    // After apply_action, fatigue was 100 at time of speed calc, so fatigue_mult = 0.5
    // But fatigue >= 100 triggers collapse which sets fatigue=80.
    // Speed computation happens before collapse, so effective_speed = 1.5 * 0.5 = 0.75
    let expected_speed = config.base_speed * 0.5;
    let expected_x = start.x + expected_speed;

    assert!(
        (m.pos.x - expected_x).abs() < 0.2,
        "expected x ~{expected_x} with fatigue, got {}",
        m.pos.x
    );
}

// ── Heading wrap to [0, 2pi) ────────────────────────────────────────────────

#[test]
fn heading_wraps_to_0_tau() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let start = find_passable_pos(&terrain);
    let mut m = make_merchant_at(start, Profession::Trader);
    m.heading = TAU - 0.1; // just below 2pi

    // Turn by +0.2 should wrap past TAU
    let action = MerchantAction::movement(0.2, 0.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    assert!(
        m.heading >= 0.0 && m.heading < TAU,
        "heading should be in [0, TAU), got {}",
        m.heading
    );
}

#[test]
fn heading_negative_wraps() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let start = find_passable_pos(&terrain);
    let mut m = make_merchant_at(start, Profession::Trader);
    m.heading = 0.1;

    // Turn by -0.3 rad should go negative then wrap
    let action = MerchantAction::movement(-0.2, 0.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    assert!(
        m.heading >= 0.0 && m.heading < TAU,
        "heading should be in [0, TAU), got {}",
        m.heading
    );
    // Expected: (0.1 - 0.2).rem_euclid(TAU) = TAU - 0.1
    let expected = (0.1_f32 - 0.2).rem_euclid(TAU);
    assert!(
        (m.heading - expected).abs() < 1e-5,
        "expected heading ~{expected}, got {}",
        m.heading
    );
}

// ── Collision slide ────────────────────────────────────────────────────────

#[test]
fn collision_slide_along_axis() {
    let terrain = make_terrain(); // has water/mountains
    let roads = make_road_grid();

    let (passable_pos, _impassable_pos) = find_terrain_boundary(&terrain);

    // Place merchant on the passable cell, facing toward the impassable cell (east).
    let mut m = make_merchant_at(passable_pos, Profession::Trader);
    m.heading = 0.0; // east, toward impassable

    // Move fast enough to enter the impassable cell
    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    // Merchant should NOT end up on the impassable cell.
    let mx = m.pos.x as u32;
    let my = m.pos.y as u32;
    assert!(
        terrain.is_passable(mx.min(terrain.width() - 1), my.min(terrain.height() - 1)),
        "merchant should not end up on impassable terrain at ({}, {})",
        m.pos.x,
        m.pos.y
    );
}

#[test]
fn collision_stays_in_place_if_fully_blocked() {
    // Construct a scenario where the merchant is surrounded by impassable terrain
    // by using the terrain painter.
    let mut terrain = make_all_land_terrain();
    let roads = make_road_grid();

    // Place merchant at (10.5, 10.5) on passable, surround with mountains.
    terrain.set_terrain_at(11, 10, TerrainType::Mountains); // east
    terrain.set_terrain_at(10, 11, TerrainType::Mountains); // south
    terrain.set_terrain_at(11, 11, TerrainType::Mountains); // SE

    let start = Vec2::new(10.5, 10.5);
    let mut m = make_merchant_at(start, Profession::Trader);
    // Head southeast (toward blocked cells)
    m.heading = FRAC_PI_4;

    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    // If the candidate cell and both axis slides are blocked, merchant stays put.
    // The slide-x lands in (11, 10) = Mountains -> blocked.
    // The slide-y lands in (10, 11) = Mountains -> blocked.
    // So merchant should remain at start.
    assert!(
        (m.pos.x - start.x).abs() < 0.01 && (m.pos.y - start.y).abs() < 0.01,
        "merchant should stay in place when fully blocked, got ({}, {})",
        m.pos.x,
        m.pos.y
    );
}

// ── World-bound reflection ─────────────────────────────────────────────────

#[test]
fn world_bound_reflection_left_edge() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let mut m = make_merchant_at(Vec2::new(0.5, 32.0), Profession::Trader);
    m.heading = PI; // heading west

    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    // Should bounce back into bounds.
    assert!(
        m.pos.x >= 0.0,
        "x should be >= 0 after left-edge reflection, got {}",
        m.pos.x
    );
}

#[test]
fn world_bound_reflection_top_edge() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let mut m = make_merchant_at(Vec2::new(32.0, 0.5), Profession::Trader);
    m.heading = 3.0 * FRAC_PI_2; // heading north (270 deg = 3pi/2)

    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    assert!(
        m.pos.y >= 0.0,
        "y should be >= 0 after top-edge reflection, got {}",
        m.pos.y
    );
}

#[test]
fn world_bound_reflection_right_edge() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let mut m = make_merchant_at(Vec2::new(63.0, 32.0), Profession::Trader);
    m.heading = 0.0; // heading east

    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    assert!(
        m.pos.x < 64.0,
        "x should be < 64 after right-edge reflection, got {}",
        m.pos.x
    );
}

#[test]
fn world_bound_reflection_bottom_edge() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let mut m = make_merchant_at(Vec2::new(32.0, 63.0), Profession::Trader);
    m.heading = FRAC_PI_2; // heading south (pi/2 = down in screen space)

    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    assert!(
        m.pos.y < 64.0,
        "y should be < 64 after bottom-edge reflection, got {}",
        m.pos.y
    );
}

// ── Fatigue drain ──────────────────────────────────────────────────────────

#[test]
fn fatigue_increases_each_tick() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let pos = find_passable_pos(&terrain);
    let mut m = make_merchant_at(pos, Profession::Trader);
    m.fatigue = 0.0;

    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    // fatigue_cost = 0.03 + 0.02 * 1.0 + 0.04 * (0 / 10) = 0.05
    let expected_fatigue = 0.05;
    assert!(
        (m.fatigue - expected_fatigue).abs() < 1e-5,
        "expected fatigue {expected_fatigue}, got {}",
        m.fatigue
    );
}

#[test]
fn fatigue_cost_increases_with_weight() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let pos = find_passable_pos(&terrain);
    let mut m = make_merchant_at(pos, Profession::Trader);
    m.fatigue = 0.0;
    m.add_to_inventory(Commodity::Ore, 5.0); // 5 / 10 = 0.5 fill

    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    // fatigue_cost = 0.03 + 0.02 * 1.0 + 0.04 * (5 / 10) = 0.03 + 0.02 + 0.02 = 0.07
    let expected_fatigue = 0.07;
    assert!(
        (m.fatigue - expected_fatigue).abs() < 1e-5,
        "expected fatigue {expected_fatigue}, got {}",
        m.fatigue
    );
}

#[test]
fn fatigue_cost_increases_with_speed() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let pos = find_passable_pos(&terrain);
    let mut m1 = make_merchant_at(pos, Profession::Trader);
    m1.fatigue = 0.0;
    let action_slow = MerchantAction::movement(0.0, 0.0);
    m1.apply_action(&action_slow, &terrain, &roads, Season::Spring, 64.0, 64.0);
    let fatigue_slow = m1.fatigue;

    let mut m2 = make_merchant_at(pos, Profession::Trader);
    m2.fatigue = 0.0;
    let action_fast = MerchantAction::movement(0.0, 1.0);
    m2.apply_action(&action_fast, &terrain, &roads, Season::Spring, 64.0, 64.0);
    let fatigue_fast = m2.fatigue;

    assert!(
        fatigue_fast > fatigue_slow,
        "faster movement should cost more fatigue: slow={fatigue_slow}, fast={fatigue_fast}"
    );
    // speed_mult=0: 0.03 + 0.02*0 + 0.04*0 = 0.03
    // speed_mult=1: 0.03 + 0.02*1 + 0.04*0 = 0.05
    assert!((fatigue_slow - 0.03).abs() < 1e-5);
    assert!((fatigue_fast - 0.05).abs() < 1e-5);
}

// ── Fatigue collapse ───────────────────────────────────────────────────────

#[test]
fn fatigue_collapse_drops_inventory_and_resets() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let pos = find_passable_pos(&terrain);
    let mut m = make_merchant_at(pos, Profession::Trader);
    m.add_to_inventory(Commodity::Ore, 6.0);
    m.add_to_inventory(Commodity::Timber, 4.0); // total = 10 = max_carry
    // Set fatigue high enough that one tick pushes it to >= 100.
    m.fatigue = 99.96;

    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    // After collapse: fatigue should be 80.
    assert!(
        (m.fatigue - 80.0).abs() < 1e-4,
        "fatigue should be 80 after collapse, got {}",
        m.fatigue
    );
    // Ore: 6 - 15% = 5.1
    let ore = *m.inventory.get(&Commodity::Ore).unwrap_or(&0.0);
    assert!(
        (ore - 5.1).abs() < 0.1,
        "ore should be ~5.1 after collapse, got {ore}"
    );
    // Timber: 4 - 15% = 3.4
    let timber = *m.inventory.get(&Commodity::Timber).unwrap_or(&0.0);
    assert!(
        (timber - 3.4).abs() < 0.1,
        "timber should be ~3.4 after collapse, got {timber}"
    );
}

#[test]
fn fatigue_does_not_exceed_100_without_collapse() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let pos = find_passable_pos(&terrain);
    let mut m = make_merchant_at(pos, Profession::Trader);
    m.fatigue = 99.0;

    // fatigue_cost will be ~0.05, so 99.05 < 100 => no collapse
    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    assert!(
        m.fatigue < 100.0,
        "fatigue should stay below 100, got {}",
        m.fatigue
    );
}

// ── Fatigue recovery at city ───────────────────────────────────────────────

#[test]
fn fatigue_recovery_at_city() {
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);
    m.fatigue = 50.0;

    m.recover_fatigue_at_city();

    assert!(
        (m.fatigue - 48.5).abs() < 1e-5,
        "fatigue should decrease by 1.5 to 48.5, got {}",
        m.fatigue
    );
}

#[test]
fn fatigue_recovery_clamps_at_zero() {
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);
    m.fatigue = 0.5;

    m.recover_fatigue_at_city();

    assert!(
        (m.fatigue - 0.0).abs() < 1e-5,
        "fatigue should clamp at 0, got {}",
        m.fatigue
    );
}

#[test]
fn fatigue_recovery_multiple_ticks() {
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);
    m.fatigue = 80.0;

    for _ in 0..10 {
        m.recover_fatigue_at_city();
    }

    // 80 - 10 * 1.5 = 65
    assert!(
        (m.fatigue - 65.0).abs() < 1e-4,
        "expected fatigue 65 after 10 recovery ticks, got {}",
        m.fatigue
    );
}

// ── Inventory weight ───────────────────────────────────────────────────────

#[test]
fn inventory_weight_is_sum_of_commodities() {
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);
    m.add_to_inventory(Commodity::Ore, 3.0);
    m.add_to_inventory(Commodity::Timber, 2.5);
    m.add_to_inventory(Commodity::Grain, 1.0);

    assert!(
        (m.inventory_weight() - 6.5).abs() < 1e-6,
        "expected weight 6.5, got {}",
        m.inventory_weight()
    );
}

#[test]
fn inventory_weight_empty() {
    let m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);
    assert!(
        (m.inventory_weight() - 0.0).abs() < 1e-6,
        "empty inventory should weigh 0"
    );
}

#[test]
fn inventory_respects_max_carry() {
    let config = mini_merchant_config();
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);

    let added = m.add_to_inventory(Commodity::Ore, 100.0);
    assert!(
        (added - config.max_carry).abs() < 1e-6,
        "should only add max_carry={}, got {added}",
        config.max_carry
    );

    let added2 = m.add_to_inventory(Commodity::Timber, 5.0);
    assert!(
        added2 < 1e-6,
        "should not add more when at capacity, got {added2}"
    );
}

#[test]
fn remove_from_inventory_returns_actual_removed() {
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);
    m.add_to_inventory(Commodity::Ore, 3.0);

    let removed = m.remove_from_inventory(Commodity::Ore, 10.0);
    assert!(
        (removed - 3.0).abs() < 1e-6,
        "should only remove what's available: expected 3.0, got {removed}"
    );
    assert!(
        !m.inventory.contains_key(&Commodity::Ore),
        "commodity should be cleaned up from inventory"
    );
}

// ── Gold debit/credit ──────────────────────────────────────────────────────

#[test]
fn gold_debit_and_credit() {
    let config = mini_merchant_config();
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);

    assert!(
        (m.gold - config.initial_gold).abs() < 1e-6,
        "initial gold should be {}",
        config.initial_gold
    );

    m.gold -= 30.0;
    assert!(
        (m.gold - 70.0).abs() < 1e-6,
        "gold after debit should be 70, got {}",
        m.gold
    );

    m.gold += 50.0;
    assert!(
        (m.gold - 120.0).abs() < 1e-6,
        "gold after credit should be 120, got {}",
        m.gold
    );
}

#[test]
fn gold_can_go_negative() {
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);
    m.gold = -50.0;
    assert!(m.gold < 0.0, "gold should be able to go negative");
}

// ── Bankruptcy ─────────────────────────────────────────────────────────────

#[test]
fn bankruptcy_after_grace_period() {
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);
    m.gold = -10.0;

    for _ in 0..199 {
        assert!(!m.tick_bankruptcy(200), "should not be bankrupt before 200 ticks");
    }
    assert!(
        m.tick_bankruptcy(200),
        "should be bankrupt at exactly 200 ticks"
    );
    assert!(!m.alive, "bankrupt merchant should be dead");
}

#[test]
fn bankruptcy_resets_on_positive_gold() {
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);
    m.gold = -1.0;

    for _ in 0..100 {
        m.tick_bankruptcy(200);
    }

    // Reset: gold goes positive
    m.gold = 10.0;
    m.tick_bankruptcy(200);

    // Now go negative again — needs full 200 ticks again.
    m.gold = -1.0;
    for _ in 0..199 {
        assert!(!m.tick_bankruptcy(200), "counter should have reset");
    }
    assert!(m.tick_bankruptcy(200));
}

#[test]
fn bankruptcy_clears_inventory() {
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);
    m.add_to_inventory(Commodity::Ore, 5.0);
    m.gold = -10.0;

    for _ in 0..200 {
        m.tick_bankruptcy(200);
    }

    assert!(!m.alive);
    assert!(m.inventory.is_empty(), "bankrupt merchant should have empty inventory");
}

// ── Fatigue multiplier formula ─────────────────────────────────────────────

#[test]
fn fatigue_mult_formula() {
    let mut m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Trader);

    m.fatigue = 0.0;
    assert!(
        (m.fatigue_mult() - 1.0).abs() < 1e-6,
        "fatigue_mult at 0 should be 1.0"
    );

    m.fatigue = 100.0;
    assert!(
        (m.fatigue_mult() - 0.5).abs() < 1e-6,
        "fatigue_mult at 100 should be 0.5"
    );

    m.fatigue = 140.0;
    // 1.0 - 140/200 = 0.3
    assert!(
        (m.fatigue_mult() - 0.3).abs() < 1e-6,
        "fatigue_mult at 140 should be 0.3"
    );

    m.fatigue = 200.0;
    // 1.0 - 200/200 = 0.0, but clamped to 0.3
    assert!(
        (m.fatigue_mult() - 0.3).abs() < 1e-6,
        "fatigue_mult at 200 should clamp to 0.3"
    );

    m.fatigue = 50.0;
    // 1.0 - 50/200 = 0.75
    assert!(
        (m.fatigue_mult() - 0.75).abs() < 1e-6,
        "fatigue_mult at 50 should be 0.75"
    );
}

// ── Caravan speed (slowest member) ─────────────────────────────────────────
// Caravan speed is governed by the concept that a caravan moves at the speed
// of its slowest member. We test that fatigue_mult (which affects effective
// speed) varies per merchant, so a caravan coordinator would pick the minimum.

#[test]
fn caravan_slowest_member_determines_speed() {
    // Create merchants with different fatigue levels (simulating caravan members).
    let mut m1 = make_merchant_with_id(1, Vec2::new(10.0, 10.0), Profession::Trader);
    let mut m2 = make_merchant_with_id(2, Vec2::new(10.0, 10.0), Profession::Trader);
    let mut m3 = make_merchant_with_id(3, Vec2::new(10.0, 10.0), Profession::Trader);

    m1.fatigue = 0.0;   // mult = 1.0
    m2.fatigue = 50.0;  // mult = 0.75
    m3.fatigue = 100.0; // mult = 0.5

    let members = [&m1, &m2, &m3];
    let slowest_mult = members.iter().map(|m| m.fatigue_mult()).fold(f32::MAX, f32::min);

    assert!(
        (slowest_mult - 0.5).abs() < 1e-6,
        "caravan should move at slowest member's speed mult: expected 0.5, got {slowest_mult}"
    );
}

#[test]
fn caravan_with_heavy_load_is_slowest() {
    // A merchant with a full inventory has higher fatigue cost per tick,
    // and eventually the lowest fatigue_mult. Here we just verify
    // that a heavily loaded merchant has lower fatigue_mult after some ticks.
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let pos = find_passable_pos(&terrain);
    let mut light = make_merchant_with_id(10, pos, Profession::Trader);
    light.heading = 0.0;
    light.fatigue = 0.0;

    let mut heavy = make_merchant_with_id(11, pos, Profession::Trader);
    heavy.heading = 0.0;
    heavy.fatigue = 0.0;
    heavy.add_to_inventory(Commodity::Ore, 10.0); // full load

    let action = MerchantAction::movement(0.0, 1.0);

    for _ in 0..100 {
        light.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);
        heavy.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);
    }

    assert!(
        heavy.fatigue > light.fatigue || heavy.fatigue >= 80.0,
        "heavily loaded merchant should accumulate fatigue faster: light={}, heavy={}",
        light.fatigue,
        heavy.fatigue
    );
}

// ── Dead merchant cannot move ──────────────────────────────────────────────

#[test]
fn dead_merchant_does_not_move() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let pos = find_passable_pos(&terrain);
    let mut m = make_merchant_at(pos, Profession::Trader);
    m.alive = false;

    let original_pos = m.pos;
    let action = MerchantAction::movement(0.0, 1.0);
    m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);

    assert!(
        (m.pos.x - original_pos.x).abs() < 1e-6 && (m.pos.y - original_pos.y).abs() < 1e-6,
        "dead merchant should not move"
    );
}

// ── Age increments on apply_action ─────────────────────────────────────────

#[test]
fn age_increments_each_tick() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let pos = find_passable_pos(&terrain);
    let mut m = make_merchant_at(pos, Profession::Trader);
    assert_eq!(m.age, 0);

    let action = MerchantAction::movement(0.0, 0.0);
    for _ in 0..5 {
        m.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);
    }

    assert_eq!(m.age, 5, "age should increment each tick");
}

// ── Shipwright has extra carry capacity ─────────────────────────────────────

#[test]
fn shipwright_carry_capacity() {
    let config = mini_merchant_config();
    let m = make_merchant_at(Vec2::new(10.0, 10.0), Profession::Shipwright);

    let expected = config.max_carry * config.shipwright_carry_mult;
    assert!(
        (m.max_carry - expected).abs() < 1e-6,
        "shipwright should have max_carry={expected}, got {}",
        m.max_carry
    );
}

// ── Winter season reduces effective speed ──────────────────────────────────

#[test]
fn winter_reduces_movement_speed() {
    let terrain = make_all_land_terrain();
    let roads = make_road_grid();

    let pos = find_passable_pos(&terrain);

    let mut m_spring = make_merchant_at(pos, Profession::Trader);
    m_spring.heading = 0.0;
    m_spring.fatigue = 0.0;
    let action = MerchantAction::movement(0.0, 1.0);
    m_spring.apply_action(&action, &terrain, &roads, Season::Spring, 64.0, 64.0);
    let dx_spring = m_spring.pos.x - pos.x;

    let mut m_winter = make_merchant_at(pos, Profession::Trader);
    m_winter.heading = 0.0;
    m_winter.fatigue = 0.0;
    m_winter.apply_action(&action, &terrain, &roads, Season::Winter, 64.0, 64.0);
    let dx_winter = m_winter.pos.x - pos.x;

    // Winter speed = spring * 0.7 (both terrain and season modifiers apply)
    // The season.travel_speed_modifier() is applied in apply_action.
    assert!(
        dx_winter < dx_spring,
        "winter movement should be slower: spring dx={dx_spring}, winter dx={dx_winter}"
    );

    // The ratio should be approximately 0.7 (terrain seasonal * season modifier)
    // terrain.speed_at applies 0.7 for winter, AND season.travel_speed_modifier() = 0.7
    // So total ratio = 0.7 * 0.7 = 0.49
    let ratio = dx_winter / dx_spring;
    assert!(
        (ratio - 0.49).abs() < 0.05,
        "winter/spring speed ratio should be ~0.49, got {ratio}"
    );
}
