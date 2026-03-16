mod common;

use swarm_economy::brain;
use swarm_economy::brain::interface::Brain;
use swarm_economy::types::*;

use common::*;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn shipwright_brain() -> Box<dyn Brain> {
    brain::brain_for_profession(Profession::Shipwright)
}

// ── Coast-only movement ─────────────────────────────────────────────────────

#[test]
fn sailing_speed_1_0_on_coast_terrain() {
    let brain = shipwright_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Shipwright);
    merchant.state = AgentState::Sailing;

    let mut sensory = default_sensory_input();
    sensory.fatigue = 10.0;
    sensory.inventory_fill_ratio = 0.5;
    sensory.current_terrain = TerrainType::Coast;
    sensory.current_season = Season::Spring;
    // City far enough away so we don't trigger unloading
    sensory.nearest_city = (Vec2::new(1.0, 0.0), 100.0);

    let action = brain.decide(&sensory, &mut merchant);

    assert!(
        (action.speed_mult - 1.0).abs() < 1e-6,
        "shipwright should sail at speed_mult=1.0 on Coast, got {}",
        action.speed_mult
    );
}

#[test]
fn sailing_speed_0_8_off_coast_terrain() {
    let brain = shipwright_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Shipwright);
    merchant.state = AgentState::Sailing;

    let mut sensory = default_sensory_input();
    sensory.fatigue = 10.0;
    sensory.inventory_fill_ratio = 0.5;
    sensory.current_terrain = TerrainType::Plains;
    sensory.current_season = Season::Spring;
    sensory.nearest_city = (Vec2::new(1.0, 0.0), 100.0);

    let action = brain.decide(&sensory, &mut merchant);

    assert!(
        (action.speed_mult - 0.8).abs() < 1e-6,
        "shipwright should sail at speed_mult=0.8 off Coast, got {}",
        action.speed_mult
    );
}

// ── Speed/carry multipliers ─────────────────────────────────────────────────

#[test]
fn shipwright_merchant_has_3x_carry_capacity() {
    let config = mini_merchant_config();
    let merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Shipwright);

    let expected_carry = config.max_carry * config.shipwright_carry_mult;
    assert!(
        (merchant.max_carry - expected_carry).abs() < 1e-6,
        "shipwright max_carry should be base ({}) * shipwright_carry_mult ({}), got {}",
        config.max_carry,
        config.shipwright_carry_mult,
        merchant.max_carry
    );
    assert!(
        (config.shipwright_carry_mult - 3.0).abs() < 1e-6,
        "shipwright_carry_mult should be 3.0"
    );
}

// ── Harbor requirement ──────────────────────────────────────────────────────

#[test]
fn loading_at_city_attempts_to_buy_cargo() {
    let brain = shipwright_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Shipwright);
    merchant.state = AgentState::Loading;

    // Simulate being at a city
    let mut sensory = sensory_at_city();
    sensory.fatigue = 10.0;
    sensory.gold = 100.0;
    sensory.inventory_fill_ratio = 0.0;
    sensory.current_season = Season::Spring;

    let action = brain.decide(&sensory, &mut merchant);

    // When at a city with gold and empty inventory, the brain should either
    // try to buy (MarketAction::Buy) or stay still waiting.
    // Given no price memory, it falls back to buying Timber at 5.0
    match action.market_action {
        MarketAction::Buy { quantity, .. } => {
            assert!(quantity > 0.0, "should attempt to buy a positive quantity");
        }
        MarketAction::None => {
            // Acceptable if the merchant is resting / waiting
            assert!(
                action.speed_mult < 0.01,
                "if not buying, should be stationary at city"
            );
        }
        _ => panic!("loading shipwright at city should Buy or idle, not Sell"),
    }
}

// ── Winter lockout ──────────────────────────────────────────────────────────

#[test]
fn winter_forces_resting_state() {
    let brain = shipwright_brain();

    // Test from various starting states
    for starting_state in [
        AgentState::Loading,
        AgentState::Sailing,
        AgentState::Unloading,
        AgentState::Resting,
    ] {
        let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Shipwright);
        merchant.state = starting_state;

        let mut sensory = default_sensory_input();
        sensory.current_season = Season::Winter;
        sensory.fatigue = 10.0;

        let _action = brain.decide(&sensory, &mut merchant);

        assert_eq!(
            merchant.state,
            AgentState::Resting,
            "shipwright in {:?} should go to Resting in Winter",
            starting_state
        );
    }
}

#[test]
fn winter_rest_does_not_sail() {
    let brain = shipwright_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Shipwright);
    merchant.state = AgentState::Sailing;

    let mut sensory = default_sensory_input();
    sensory.current_season = Season::Winter;
    sensory.fatigue = 10.0;
    sensory.inventory_fill_ratio = 0.8;
    // At city — should rest, not sail
    sensory.nearest_city = (Vec2::new(0.0, 0.0), 5.0);

    let action = brain.decide(&sensory, &mut merchant);

    assert_eq!(merchant.state, AgentState::Resting);
    // When at city during winter rest, should be stationary and resting
    assert!(
        action.speed_mult < 0.01 || action.rest,
        "should be resting or stationary during winter at city"
    );
}

// ── Loading -> Sailing ──────────────────────────────────────────────────────

#[test]
fn loading_to_sailing_when_inventory_above_60_percent() {
    let brain = shipwright_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Shipwright);
    merchant.state = AgentState::Loading;

    // Add actual inventory so that the merchant's inventory_fill_ratio matches
    // the sensory input, and so Unloading has goods to sell if it recurses.
    merchant.add_to_inventory(Commodity::Timber, merchant.max_carry * 0.7);

    let mut sensory = default_sensory_input();
    // City close enough to pass Loading's "not at city" check (< 25)
    sensory.nearest_city = (Vec2::new(0.0, 0.0), 10.0);
    sensory.fatigue = 10.0;
    sensory.gold = 100.0;
    sensory.inventory_fill_ratio = 0.65; // > 0.6 threshold
    sensory.current_season = Season::Spring;

    let _action = brain.decide(&sensory, &mut merchant);

    // Loading transitions to Sailing. Sailing may further transition to Unloading
    // (since the city is close and there are goods). Either way, Loading is left.
    assert!(
        merchant.state == AgentState::Sailing || merchant.state == AgentState::Unloading,
        "shipwright should transition from Loading when inventory > 60%, got {:?}",
        merchant.state
    );
}

#[test]
fn loading_stays_when_inventory_below_60_percent() {
    let brain = shipwright_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Shipwright);
    merchant.state = AgentState::Loading;

    let mut sensory = sensory_at_city();
    sensory.fatigue = 10.0;
    sensory.gold = 100.0;
    sensory.inventory_fill_ratio = 0.3; // < 0.6 threshold
    sensory.current_season = Season::Spring;

    let _action = brain.decide(&sensory, &mut merchant);

    // Should still be Loading (or might buy), not yet Sailing
    assert_ne!(
        merchant.state,
        AgentState::Sailing,
        "shipwright should NOT transition to Sailing when inventory < 60% (unless can't buy more)"
    );
}

// ── Unloading at city ───────────────────────────────────────────────────────

#[test]
fn unloading_at_city_with_goods_sells() {
    let brain = shipwright_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Shipwright);
    merchant.state = AgentState::Sailing;

    // Add goods to inventory
    merchant.add_to_inventory(Commodity::Timber, 5.0);

    let mut sensory = sensory_at_city();
    sensory.fatigue = 10.0;
    sensory.inventory_fill_ratio = 0.5;
    sensory.current_season = Season::Spring;

    let _action = brain.decide(&sensory, &mut merchant);

    // Should transition to Unloading when at city with goods during Sailing
    assert_eq!(
        merchant.state,
        AgentState::Unloading,
        "shipwright should transition to Unloading when arriving at city with goods"
    );
}

#[test]
fn unloading_state_issues_sell_action() {
    let brain = shipwright_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Shipwright);
    merchant.state = AgentState::Unloading;

    // Add goods to inventory
    merchant.add_to_inventory(Commodity::Timber, 5.0);

    let mut sensory = sensory_at_city();
    sensory.fatigue = 10.0;
    sensory.inventory_fill_ratio = 0.5;
    sensory.current_season = Season::Spring;
    sensory.inventory_breakdown.insert(Commodity::Timber, 5.0);

    let action = brain.decide(&sensory, &mut merchant);

    match action.market_action {
        MarketAction::Sell { commodity, min_price, quantity } => {
            assert_eq!(commodity, Commodity::Timber);
            assert!(quantity > 0.0, "should sell a positive quantity");
            assert!(min_price > 0.0, "should have a positive minimum price");
        }
        _ => panic!("unloading shipwright with goods should issue a Sell action"),
    }
}
