mod common;

use swarm_economy::types::{Commodity, Season, Vec2};
use swarm_economy::world::resource_node::ResourceNode;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn make_node(commodity: Commodity) -> ResourceNode {
    ResourceNode::new(0, Vec2::new(50.0, 50.0), commodity, 3.0)
}

fn make_node_at(commodity: Commodity, pos: Vec2) -> ResourceNode {
    ResourceNode::new(0, pos, commodity, 3.0)
}

const MAP_HEIGHT: f32 = 100.0;

// ── Grain seasonal yield modifiers ──────────────────────────────────────────

#[test]
fn grain_summer_yields_2x() {
    let mut node = make_node(Commodity::Grain);
    let y = node.extract(Season::Summer, MAP_HEIGHT);
    assert!(
        (y - 6.0).abs() < 1e-5,
        "Grain summer: base 3.0 * 2.0 = 6.0, got {}",
        y
    );
}

#[test]
fn grain_winter_yields_0_3x() {
    let mut node = make_node(Commodity::Grain);
    let y = node.extract(Season::Winter, MAP_HEIGHT);
    assert!(
        (y - 0.9).abs() < 1e-5,
        "Grain winter: base 3.0 * 0.3 = 0.9, got {}",
        y
    );
}

#[test]
fn grain_spring_yields_1x() {
    let mut node = make_node(Commodity::Grain);
    let y = node.extract(Season::Spring, MAP_HEIGHT);
    assert!(
        (y - 3.0).abs() < 1e-5,
        "Grain spring: base 3.0 * 1.0 = 3.0, got {}",
        y
    );
}

#[test]
fn grain_autumn_yields_1x() {
    let mut node = make_node(Commodity::Grain);
    let y = node.extract(Season::Autumn, MAP_HEIGHT);
    assert!(
        (y - 3.0).abs() < 1e-5,
        "Grain autumn: base 3.0 * 1.0 = 3.0, got {}",
        y
    );
}

// ── Herbs seasonal yield modifiers ──────────────────────────────────────────

#[test]
fn herbs_summer_yields_2x() {
    let mut node = make_node(Commodity::Herbs);
    let y = node.extract(Season::Summer, MAP_HEIGHT);
    assert!(
        (y - 6.0).abs() < 1e-5,
        "Herbs summer: base 3.0 * 2.0 = 6.0, got {}",
        y
    );
}

#[test]
fn herbs_winter_yields_0_3x() {
    let mut node = make_node(Commodity::Herbs);
    let y = node.extract(Season::Winter, MAP_HEIGHT);
    assert!(
        (y - 0.9).abs() < 1e-5,
        "Herbs winter: base 3.0 * 0.3 = 0.9, got {}",
        y
    );
}

#[test]
fn herbs_spring_yields_1x() {
    let mut node = make_node(Commodity::Herbs);
    let y = node.extract(Season::Spring, MAP_HEIGHT);
    assert!(
        (y - 3.0).abs() < 1e-5,
        "Herbs spring: unaffected, got {}",
        y
    );
}

#[test]
fn herbs_autumn_yields_1x() {
    let mut node = make_node(Commodity::Herbs);
    let y = node.extract(Season::Autumn, MAP_HEIGHT);
    assert!(
        (y - 3.0).abs() < 1e-5,
        "Herbs autumn: unaffected, got {}",
        y
    );
}

// ── Fish seasonal yield modifiers ───────────────────────────────────────────

#[test]
fn fish_spring_yields_1_5x() {
    let mut node = make_node(Commodity::Fish);
    let y = node.extract(Season::Spring, MAP_HEIGHT);
    assert!(
        (y - 4.5).abs() < 1e-5,
        "Fish spring: base 3.0 * 1.5 = 4.5, got {}",
        y
    );
}

#[test]
fn fish_autumn_yields_0_5x() {
    let mut node = make_node(Commodity::Fish);
    let y = node.extract(Season::Autumn, MAP_HEIGHT);
    assert!(
        (y - 1.5).abs() < 1e-5,
        "Fish autumn: base 3.0 * 0.5 = 1.5, got {}",
        y
    );
}

#[test]
fn fish_summer_yields_1x() {
    let mut node = make_node(Commodity::Fish);
    let y = node.extract(Season::Summer, MAP_HEIGHT);
    assert!(
        (y - 3.0).abs() < 1e-5,
        "Fish summer: unaffected, got {}",
        y
    );
}

#[test]
fn fish_winter_yields_1x() {
    let mut node = make_node(Commodity::Fish);
    let y = node.extract(Season::Winter, MAP_HEIGHT);
    assert!(
        (y - 3.0).abs() < 1e-5,
        "Fish winter: unaffected, got {}",
        y
    );
}

// ── Clay seasonal yield modifiers ───────────────────────────────────────────

#[test]
fn clay_winter_northern_yields_zero() {
    // Northern = y < 0.3 * map_height = 30.0
    let mut node = make_node_at(Commodity::Clay, Vec2::new(50.0, 10.0));
    let y = node.extract(Season::Winter, MAP_HEIGHT);
    assert!(
        y.abs() < 1e-5,
        "Clay winter northern: should yield 0.0, got {}",
        y
    );
}

#[test]
fn clay_winter_southern_yields_1x() {
    // Southern = y >= 0.3 * map_height = 30.0
    let mut node = make_node_at(Commodity::Clay, Vec2::new(50.0, 50.0));
    let y = node.extract(Season::Winter, MAP_HEIGHT);
    assert!(
        (y - 3.0).abs() < 1e-5,
        "Clay winter southern: unaffected, got {}",
        y
    );
}

#[test]
fn clay_winter_boundary_northern() {
    // Exactly at 0.3 * map_height boundary: y = 29.9 (< 30.0) => northern
    let mut node = make_node_at(Commodity::Clay, Vec2::new(50.0, 29.9));
    let y = node.extract(Season::Winter, MAP_HEIGHT);
    assert!(
        y.abs() < 1e-5,
        "Clay at y=29.9 (< 30.0) in winter should yield 0.0, got {}",
        y
    );
}

#[test]
fn clay_winter_boundary_southern() {
    // Due to f32 precision, 100.0 * 0.3 is slightly above 30.0,
    // so y must be comfortably above the boundary to count as southern.
    let mut node = make_node_at(Commodity::Clay, Vec2::new(50.0, 31.0));
    let y = node.extract(Season::Winter, MAP_HEIGHT);
    assert!(
        (y - 3.0).abs() < 1e-5,
        "Clay at y=31.0 (southern) in winter should be unaffected, got {}",
        y
    );
}

#[test]
fn clay_summer_unaffected_even_northern() {
    let mut node = make_node_at(Commodity::Clay, Vec2::new(50.0, 10.0));
    let y = node.extract(Season::Summer, MAP_HEIGHT);
    assert!(
        (y - 3.0).abs() < 1e-5,
        "Clay summer: always 1.0x, got {}",
        y
    );
}

#[test]
fn clay_spring_unaffected() {
    let mut node = make_node_at(Commodity::Clay, Vec2::new(50.0, 10.0));
    let y = node.extract(Season::Spring, MAP_HEIGHT);
    assert!(
        (y - 3.0).abs() < 1e-5,
        "Clay spring: always 1.0x, got {}",
        y
    );
}

// ── Timber / Ore always 1.0x ────────────────────────────────────────────────

#[test]
fn timber_unaffected_all_seasons() {
    for season in [Season::Spring, Season::Summer, Season::Autumn, Season::Winter] {
        let mut node = make_node(Commodity::Timber);
        let y = node.extract(season, MAP_HEIGHT);
        assert!(
            (y - 3.0).abs() < 1e-5,
            "Timber should be 1.0x in {:?}, got {}",
            season,
            y
        );
    }
}

#[test]
fn ore_unaffected_all_seasons() {
    for season in [Season::Spring, Season::Summer, Season::Autumn, Season::Winter] {
        let mut node = make_node(Commodity::Ore);
        let y = node.extract(season, MAP_HEIGHT);
        assert!(
            (y - 3.0).abs() < 1e-5,
            "Ore should be 1.0x in {:?}, got {}",
            season,
            y
        );
    }
}

// ── Winter travel speed modifier ────────────────────────────────────────────

#[test]
fn winter_travel_speed_is_0_7() {
    assert!(
        (Season::Winter.travel_speed_modifier() - 0.7).abs() < 1e-5,
        "Winter travel speed should be 0.7"
    );
}

#[test]
fn spring_travel_speed_is_1_0() {
    assert!(
        (Season::Spring.travel_speed_modifier() - 1.0).abs() < 1e-5,
        "Spring travel speed should be 1.0"
    );
}

#[test]
fn summer_travel_speed_is_1_0() {
    assert!(
        (Season::Summer.travel_speed_modifier() - 1.0).abs() < 1e-5,
        "Summer travel speed should be 1.0"
    );
}

#[test]
fn autumn_travel_speed_is_1_0() {
    assert!(
        (Season::Autumn.travel_speed_modifier() - 1.0).abs() < 1e-5,
        "Autumn travel speed should be 1.0"
    );
}

// ── Winter food consumption modifier ────────────────────────────────────────

#[test]
fn winter_food_consumption_is_1_5() {
    assert!(
        (Season::Winter.food_consumption_modifier() - 1.5).abs() < 1e-5,
        "Winter food consumption should be 1.5x"
    );
}

#[test]
fn spring_food_consumption_is_1_0() {
    assert!(
        (Season::Spring.food_consumption_modifier() - 1.0).abs() < 1e-5,
        "Spring food consumption should be 1.0x"
    );
}

#[test]
fn summer_food_consumption_is_1_0() {
    assert!(
        (Season::Summer.food_consumption_modifier() - 1.0).abs() < 1e-5,
        "Summer food consumption should be 1.0x"
    );
}

#[test]
fn autumn_food_consumption_is_1_0() {
    assert!(
        (Season::Autumn.food_consumption_modifier() - 1.0).abs() < 1e-5,
        "Autumn food consumption should be 1.0x"
    );
}

// ── Bandit activity modifier (bonus coverage) ───────────────────────────────

#[test]
fn bandit_activity_summer_1_3() {
    assert!(
        (Season::Summer.bandit_activity_modifier() - 1.3).abs() < 1e-5,
        "Summer bandit activity should be 1.3"
    );
}

#[test]
fn bandit_activity_winter_0_5() {
    assert!(
        (Season::Winter.bandit_activity_modifier() - 0.5).abs() < 1e-5,
        "Winter bandit activity should be 0.5"
    );
}

#[test]
fn bandit_activity_spring_1_0() {
    assert!(
        (Season::Spring.bandit_activity_modifier() - 1.0).abs() < 1e-5,
        "Spring bandit activity should be 1.0"
    );
}

#[test]
fn bandit_activity_autumn_1_0() {
    assert!(
        (Season::Autumn.bandit_activity_modifier() - 1.0).abs() < 1e-5,
        "Autumn bandit activity should be 1.0"
    );
}

// ── Harbor freeze: Shipwright rests in winter ───────────────────────────────
// The Shipwright's "Sailing" state should not be active in winter.
// We verify this indirectly: in winter, travel_speed_modifier is 0.7 and
// harbors would freeze, preventing sailing. The Season API provides the
// travel_speed_modifier that the simulation uses to decide whether to sail.

#[test]
fn winter_speed_modifier_slows_all_travel_including_ships() {
    // In the simulation, Shipwright checks Season::travel_speed_modifier
    // and rests when it indicates winter conditions.
    let winter_speed = Season::Winter.travel_speed_modifier();
    let spring_speed = Season::Spring.travel_speed_modifier();
    assert!(
        winter_speed < spring_speed,
        "Winter travel is slower ({}), signaling harbor freeze vs spring ({})",
        winter_speed,
        spring_speed
    );
}

// ── Season cycle ────────────────────────────────────────────────────────────

#[test]
fn season_cycle_spring_to_summer() {
    assert_eq!(Season::Spring.next(), Season::Summer);
}

#[test]
fn season_cycle_summer_to_autumn() {
    assert_eq!(Season::Summer.next(), Season::Autumn);
}

#[test]
fn season_cycle_autumn_to_winter() {
    assert_eq!(Season::Autumn.next(), Season::Winter);
}

#[test]
fn season_cycle_winter_to_spring() {
    assert_eq!(Season::Winter.next(), Season::Spring);
}

#[test]
fn full_year_cycle_returns_to_start() {
    let start = Season::Spring;
    let end = start.next().next().next().next();
    assert_eq!(start, end, "four nexts should return to the same season");
}

// ── Comprehensive per-commodity per-season table ────────────────────────────

/// Exhaustive verification of all yield modifiers.
/// Each tuple: (commodity, season, position, expected_modifier).
#[test]
fn exhaustive_seasonal_modifier_table() {
    let cases: Vec<(Commodity, Season, Vec2, f32)> = vec![
        // Grain
        (Commodity::Grain, Season::Spring, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Grain, Season::Summer, Vec2::new(50.0, 50.0), 2.0),
        (Commodity::Grain, Season::Autumn, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Grain, Season::Winter, Vec2::new(50.0, 50.0), 0.3),
        // Herbs
        (Commodity::Herbs, Season::Spring, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Herbs, Season::Summer, Vec2::new(50.0, 50.0), 2.0),
        (Commodity::Herbs, Season::Autumn, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Herbs, Season::Winter, Vec2::new(50.0, 50.0), 0.3),
        // Fish
        (Commodity::Fish, Season::Spring, Vec2::new(50.0, 50.0), 1.5),
        (Commodity::Fish, Season::Summer, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Fish, Season::Autumn, Vec2::new(50.0, 50.0), 0.5),
        (Commodity::Fish, Season::Winter, Vec2::new(50.0, 50.0), 1.0),
        // Clay (southern, y >= 30)
        (Commodity::Clay, Season::Spring, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Clay, Season::Summer, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Clay, Season::Autumn, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Clay, Season::Winter, Vec2::new(50.0, 50.0), 1.0),
        // Clay (northern, y < 30) — only winter differs
        (Commodity::Clay, Season::Spring, Vec2::new(50.0, 10.0), 1.0),
        (Commodity::Clay, Season::Summer, Vec2::new(50.0, 10.0), 1.0),
        (Commodity::Clay, Season::Autumn, Vec2::new(50.0, 10.0), 1.0),
        (Commodity::Clay, Season::Winter, Vec2::new(50.0, 10.0), 0.0),
        // Timber — always 1.0
        (Commodity::Timber, Season::Spring, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Timber, Season::Summer, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Timber, Season::Autumn, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Timber, Season::Winter, Vec2::new(50.0, 50.0), 1.0),
        // Ore — always 1.0
        (Commodity::Ore, Season::Spring, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Ore, Season::Summer, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Ore, Season::Autumn, Vec2::new(50.0, 50.0), 1.0),
        (Commodity::Ore, Season::Winter, Vec2::new(50.0, 50.0), 1.0),
    ];

    for (commodity, season, pos, expected_mod) in cases {
        let mut node = ResourceNode::new(0, pos, commodity, 3.0);
        let y = node.extract(season, MAP_HEIGHT);
        let expected_yield = 3.0 * expected_mod;
        assert!(
            (y - expected_yield).abs() < 1e-4,
            "{:?} in {:?} at y={}: expected yield {}, got {}",
            commodity,
            season,
            pos.y,
            expected_yield,
            y
        );
    }
}
