mod common;

use swarm_economy::agents::merchant::Merchant;
use swarm_economy::brain::brain_for_profession;
use swarm_economy::brain::interface::Brain;
use swarm_economy::types::*;

// ── Helpers ────────────────────────────────────────────────────────────────

fn miner_brain() -> Box<dyn Brain> {
    brain_for_profession(Profession::Miner)
}

fn miner_merchant() -> Merchant {
    common::make_merchant_at(Vec2::new(32.0, 32.0), Profession::Miner)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[test]
fn traveling_to_node_moves_toward_ore() {
    let brain = miner_brain();
    let mut merchant = miner_merchant();
    merchant.state = AgentState::TravelingToNode;
    merchant.heading = 0.0;

    let mut sensory = common::default_sensory_input();
    // Ore node 30px away to the right
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 30.0, Commodity::Ore));

    let action = brain.decide(&sensory, &mut merchant);
    // Should be moving (speed > 0) and state stays TravelingToNode
    assert_eq!(merchant.state, AgentState::TravelingToNode);
    assert!(action.speed_mult > 0.0, "Should be moving toward node");
}

#[test]
fn extraction_starts_when_at_node() {
    let brain = miner_brain();
    let mut merchant = miner_merchant();
    merchant.state = AgentState::TravelingToNode;

    let mut sensory = common::default_sensory_input();
    // At the node (dist=5 < 10)
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Ore));

    let action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::Extracting,
        "Should transition to Extracting when at node"
    );
    assert!(action.extract, "Should set extract=true");
}

#[test]
fn extracting_continues_until_full() {
    let brain = miner_brain();
    let mut merchant = miner_merchant();
    merchant.state = AgentState::Extracting;

    let mut sensory = common::default_sensory_input();
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Ore));
    sensory.inventory_fill_ratio = 0.5; // not full yet

    let action = brain.decide(&sensory, &mut merchant);
    assert_eq!(merchant.state, AgentState::Extracting);
    assert!(action.extract, "Should continue extracting");
    assert!(
        (action.speed_mult).abs() < 0.01,
        "Should be stationary while extracting"
    );
}

#[test]
fn extraction_to_traveling_to_city_when_full() {
    let brain = miner_brain();
    let mut merchant = miner_merchant();
    merchant.state = AgentState::Extracting;

    let mut sensory = common::default_sensory_input();
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Ore));
    sensory.inventory_fill_ratio = 0.95; // > 0.9 threshold

    let _action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::TravelingToCity,
        "Should go to city when inventory >90% full"
    );
}

#[test]
fn frozen_clay_skipped_in_winter() {
    let brain = miner_brain();
    let mut merchant = miner_merchant();
    merchant.state = AgentState::TravelingToNode;

    let mut sensory = common::default_sensory_input();
    sensory.current_season = Season::Winter;
    // Only resource nearby is Clay
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Clay));

    let action = brain.decide(&sensory, &mut merchant);
    // Should NOT extract frozen clay -> stays wandering/traveling
    assert_ne!(
        merchant.state,
        AgentState::Extracting,
        "Should not extract frozen Clay in Winter"
    );
    assert!(
        !action.extract,
        "extract should be false for frozen Clay in Winter"
    );
}

#[test]
fn clay_accepted_in_summer() {
    let brain = miner_brain();
    let mut merchant = miner_merchant();
    merchant.state = AgentState::TravelingToNode;

    let mut sensory = common::default_sensory_input();
    sensory.current_season = Season::Summer;
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Clay));

    let action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::Extracting,
        "Should extract Clay in Summer"
    );
    assert!(action.extract);
}

#[test]
fn non_target_commodity_causes_wander() {
    let brain = miner_brain();
    let mut merchant = miner_merchant();
    merchant.state = AgentState::TravelingToNode;

    let mut sensory = common::default_sensory_input();
    // Grain is not a miner target (only Ore/Clay)
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Grain));

    let action = brain.decide(&sensory, &mut merchant);
    assert_ne!(
        merchant.state,
        AgentState::Extracting,
        "Should not extract Grain as a miner"
    );
    assert!(
        !action.extract,
        "extract should be false for non-target resource"
    );
    // Should be wandering (speed > 0)
    assert!(
        action.speed_mult > 0.0,
        "Should keep moving to find a valid node"
    );
}

#[test]
fn travel_to_city_when_inventory_full() {
    let brain = miner_brain();
    let mut merchant = miner_merchant();
    merchant.state = AgentState::TravelingToNode;
    // Fill inventory so extracting would immediately trigger travel-to-city
    merchant.inventory.insert(Commodity::Ore, 9.5); // nearly full (max_carry=10)

    let mut sensory = common::default_sensory_input();
    sensory.nearest_resource = Some((Vec2::new(1.0, 0.0), 5.0, Commodity::Ore));
    sensory.inventory_fill_ratio = 0.95;

    let _action = brain.decide(&sensory, &mut merchant);
    // At node with full inventory -> Extracting then immediately TravelingToCity
    assert_eq!(
        merchant.state,
        AgentState::TravelingToCity,
        "Should transition to TravelingToCity when inventory full"
    );
}

#[test]
fn selling_ore_at_city() {
    let brain = miner_brain();
    let mut merchant = miner_merchant();
    merchant.state = AgentState::Selling;
    merchant.inventory.insert(Commodity::Ore, 5.0);

    let sensory = common::sensory_at_city();

    let action = brain.decide(&sensory, &mut merchant);
    assert_eq!(merchant.state, AgentState::Selling);
    match action.market_action {
        MarketAction::Sell {
            commodity,
            quantity,
            ..
        } => {
            assert_eq!(commodity, Commodity::Ore, "Should sell Ore first");
            assert!((quantity - 5.0).abs() < 0.1, "Should sell all Ore");
        }
        _ => panic!("Expected a Sell market action, got {:?}", action.market_action),
    }
}

#[test]
fn selling_done_transitions_to_traveling_to_node() {
    let brain = miner_brain();
    let mut merchant = miner_merchant();
    merchant.state = AgentState::Selling;
    // Empty inventory, low fatigue

    let mut sensory = common::sensory_at_city();
    sensory.fatigue = 10.0;

    let _action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::TravelingToNode,
        "Should go back to mining after selling everything"
    );
}
