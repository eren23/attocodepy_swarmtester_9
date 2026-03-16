mod common;

use swarm_economy::agents::merchant::Merchant;
use swarm_economy::brain::brain_for_profession;
use swarm_economy::brain::interface::Brain;
use swarm_economy::types::*;

// ── Helpers ────────────────────────────────────────────────────────────────

fn farmer_brain() -> Box<dyn Brain> {
    brain_for_profession(Profession::Farmer)
}

fn farmer_merchant() -> Merchant {
    common::make_merchant_at(Vec2::new(32.0, 32.0), Profession::Farmer)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[test]
fn winter_targets_only_fish() {
    let brain = farmer_brain();

    // Fish should be accepted in Winter
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::TravelingToNode;
    let mut sensory = common::default_sensory_input();
    sensory.current_season = Season::Winter;
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Fish));

    let action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::Extracting,
        "Winter: should extract Fish"
    );
    assert!(action.extract);
}

#[test]
fn winter_rejects_grain() {
    let brain = farmer_brain();
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::TravelingToNode;

    let mut sensory = common::default_sensory_input();
    sensory.current_season = Season::Winter;
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Grain));

    let action = brain.decide(&sensory, &mut merchant);
    assert_ne!(
        merchant.state,
        AgentState::Extracting,
        "Winter: should NOT extract Grain"
    );
    assert!(!action.extract);
}

#[test]
fn winter_rejects_herbs() {
    let brain = farmer_brain();
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::TravelingToNode;

    let mut sensory = common::default_sensory_input();
    sensory.current_season = Season::Winter;
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Herbs));

    let action = brain.decide(&sensory, &mut merchant);
    assert_ne!(
        merchant.state,
        AgentState::Extracting,
        "Winter: should NOT extract Herbs"
    );
    assert!(!action.extract);
}

#[test]
fn summer_targets_grain() {
    let brain = farmer_brain();
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::TravelingToNode;

    let mut sensory = common::default_sensory_input();
    sensory.current_season = Season::Summer;
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Grain));

    let action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::Extracting,
        "Summer: should extract Grain"
    );
    assert!(action.extract);
}

#[test]
fn summer_targets_herbs() {
    let brain = farmer_brain();
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::TravelingToNode;

    let mut sensory = common::default_sensory_input();
    sensory.current_season = Season::Summer;
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Herbs));

    let action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::Extracting,
        "Summer: should extract Herbs"
    );
    assert!(action.extract);
}

#[test]
fn summer_rejects_fish() {
    let brain = farmer_brain();
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::TravelingToNode;

    let mut sensory = common::default_sensory_input();
    sensory.current_season = Season::Summer;
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Fish));

    let action = brain.decide(&sensory, &mut merchant);
    assert_ne!(
        merchant.state,
        AgentState::Extracting,
        "Summer: should NOT extract Fish"
    );
    assert!(!action.extract);
}

#[test]
fn spring_targets_all_three() {
    let brain = farmer_brain();

    for &commodity in &[Commodity::Grain, Commodity::Herbs, Commodity::Fish] {
        let mut merchant = farmer_merchant();
        merchant.state = AgentState::TravelingToNode;

        let mut sensory = common::default_sensory_input();
        sensory.current_season = Season::Spring;
        sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, commodity));

        let action = brain.decide(&sensory, &mut merchant);
        assert_eq!(
            merchant.state,
            AgentState::Extracting,
            "Spring: should extract {:?}",
            commodity
        );
        assert!(action.extract, "Spring: extract flag for {:?}", commodity);
    }
}

#[test]
fn autumn_targets_all_three() {
    let brain = farmer_brain();

    for &commodity in &[Commodity::Grain, Commodity::Herbs, Commodity::Fish] {
        let mut merchant = farmer_merchant();
        merchant.state = AgentState::TravelingToNode;

        let mut sensory = common::default_sensory_input();
        sensory.current_season = Season::Autumn;
        sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, commodity));

        let action = brain.decide(&sensory, &mut merchant);
        assert_eq!(
            merchant.state,
            AgentState::Extracting,
            "Autumn: should extract {:?}",
            commodity
        );
        assert!(action.extract, "Autumn: extract flag for {:?}", commodity);
    }
}

#[test]
fn extract_at_seasonal_target_resource() {
    let brain = farmer_brain();
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::TravelingToNode;

    let mut sensory = common::default_sensory_input();
    sensory.current_season = Season::Spring;
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Herbs));

    let action = brain.decide(&sensory, &mut merchant);
    assert_eq!(merchant.state, AgentState::Extracting);
    assert!(action.extract);
    assert!(
        (action.speed_mult).abs() < 0.01,
        "Should be stationary while extracting"
    );
}

#[test]
fn travel_to_city_when_full() {
    let brain = farmer_brain();
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::Extracting;
    merchant.inventory.insert(Commodity::Grain, 9.5);

    let mut sensory = common::default_sensory_input();
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Grain));
    sensory.inventory_fill_ratio = 0.95; // > 0.9 threshold

    let _action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::TravelingToCity,
        "Should travel to city when inventory > 90% full"
    );
}

#[test]
fn sell_priority_grain_first() {
    let brain = farmer_brain();
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::Selling;
    merchant.inventory.insert(Commodity::Grain, 3.0);
    merchant.inventory.insert(Commodity::Herbs, 2.0);
    merchant.inventory.insert(Commodity::Fish, 1.0);

    let sensory = common::sensory_at_city();

    let action = brain.decide(&sensory, &mut merchant);
    match action.market_action {
        MarketAction::Sell { commodity, .. } => {
            assert_eq!(
                commodity,
                Commodity::Grain,
                "Should sell Grain first (highest priority)"
            );
        }
        _ => panic!("Expected Sell action"),
    }
}

#[test]
fn sell_priority_herbs_after_grain() {
    let brain = farmer_brain();
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::Selling;
    // No Grain, but have Herbs and Fish
    merchant.inventory.insert(Commodity::Herbs, 2.0);
    merchant.inventory.insert(Commodity::Fish, 1.0);

    let sensory = common::sensory_at_city();

    let action = brain.decide(&sensory, &mut merchant);
    match action.market_action {
        MarketAction::Sell { commodity, .. } => {
            assert_eq!(
                commodity,
                Commodity::Herbs,
                "Should sell Herbs second (after Grain)"
            );
        }
        _ => panic!("Expected Sell action"),
    }
}

#[test]
fn sell_priority_fish_last() {
    let brain = farmer_brain();
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::Selling;
    // Only Fish
    merchant.inventory.insert(Commodity::Fish, 1.0);

    let sensory = common::sensory_at_city();

    let action = brain.decide(&sensory, &mut merchant);
    match action.market_action {
        MarketAction::Sell { commodity, .. } => {
            assert_eq!(commodity, Commodity::Fish, "Should sell Fish last");
        }
        _ => panic!("Expected Sell action"),
    }
}

#[test]
fn rest_when_fatigued() {
    let brain = farmer_brain();
    let mut merchant = farmer_merchant();
    merchant.state = AgentState::TravelingToNode;

    let mut sensory = common::default_sensory_input();
    sensory.fatigue = 85.0; // > 80 threshold
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 30.0, Commodity::Grain));
    sensory.current_season = Season::Spring;

    let _action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::Resting,
        "Should rest when fatigue > 80"
    );
}
