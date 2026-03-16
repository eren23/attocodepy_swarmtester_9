mod common;

use swarm_economy::agents::merchant::Merchant;
use swarm_economy::brain::brain_for_profession;
use swarm_economy::brain::interface::Brain;
use swarm_economy::types::*;

// ── Helpers ────────────────────────────────────────────────────────────────

fn craftsman_brain() -> Box<dyn Brain> {
    brain_for_profession(Profession::Craftsman)
}

fn craftsman_merchant() -> Merchant {
    common::make_merchant_at(Vec2::new(32.0, 32.0), Profession::Craftsman)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[test]
fn picks_highest_margin_recipe() {
    let brain = craftsman_brain();
    let mut merchant = craftsman_merchant();
    merchant.state = AgentState::Crafting;
    // Give materials for multiple recipes:
    // Timber + Ore -> Tools (margin 2.0)
    // Grain + Fish -> Provisions (margin 2.5) -- highest margin
    merchant.inventory.insert(Commodity::Timber, 2.0);
    merchant.inventory.insert(Commodity::Ore, 2.0);
    merchant.inventory.insert(Commodity::Grain, 2.0);
    merchant.inventory.insert(Commodity::Fish, 2.0);

    let sensory = common::sensory_at_city();

    let action = brain.decide(&sensory, &mut merchant);
    assert_eq!(merchant.state, AgentState::Crafting);
    match action.craft {
        Some(recipe) => {
            // Provisions has highest margin/tick (2.5) among available recipes
            assert_eq!(
                recipe.output,
                Commodity::Provisions,
                "Should pick Provisions (highest margin recipe)"
            );
        }
        None => panic!("Expected a craft recipe"),
    }
}

#[test]
fn buys_materials_at_city_when_none() {
    let brain = craftsman_brain();
    let mut merchant = craftsman_merchant();
    merchant.state = AgentState::BuyingMaterials;
    // Empty inventory, has gold

    let mut sensory = common::sensory_at_city();
    sensory.gold = 100.0;
    sensory.inventory_fill_ratio = 0.0;

    let action = brain.decide(&sensory, &mut merchant);
    match action.market_action {
        MarketAction::Buy { commodity, .. } => {
            // Should buy one of the RAW commodities
            assert!(
                Commodity::RAW.contains(&commodity),
                "Should buy a raw material, got {:?}",
                commodity
            );
        }
        _ => {
            // Could also transition to SellingGoods or Resting if no gold
            // but we gave 100 gold, so Buy is expected
            panic!(
                "Expected Buy action with gold available, got {:?}",
                action.market_action
            );
        }
    }
}

#[test]
fn transitions_to_crafting_with_materials() {
    let brain = craftsman_brain();
    let mut merchant = craftsman_merchant();
    merchant.state = AgentState::BuyingMaterials;
    // Has enough for Timber + Ore -> Tools
    merchant.inventory.insert(Commodity::Timber, 1.0);
    merchant.inventory.insert(Commodity::Ore, 1.0);

    let sensory = common::sensory_at_city();

    let action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::Crafting,
        "Should transition to Crafting when materials available"
    );
    assert!(
        action.craft.is_some(),
        "Should produce a craft action with materials"
    );
}

#[test]
fn crafting_produces_recipe_action() {
    let brain = craftsman_brain();
    let mut merchant = craftsman_merchant();
    merchant.state = AgentState::Crafting;
    merchant.inventory.insert(Commodity::Timber, 1.0);
    merchant.inventory.insert(Commodity::Ore, 1.0);

    let sensory = common::sensory_at_city();

    let action = brain.decide(&sensory, &mut merchant);
    assert_eq!(merchant.state, AgentState::Crafting);
    match action.craft {
        Some(recipe) => {
            assert_eq!(recipe.output, Commodity::Tools);
            assert_eq!(recipe.inputs.len(), 2);
            assert!(
                (action.speed_mult).abs() < 0.01,
                "Should be stationary while crafting"
            );
        }
        None => panic!("Expected craft recipe for Timber+Ore->Tools"),
    }
}

#[test]
fn travel_toward_city_when_not_at_one() {
    let brain = craftsman_brain();
    let mut merchant = craftsman_merchant();
    merchant.state = AgentState::BuyingMaterials;
    merchant.heading = 0.0;

    // Far from city
    let mut sensory = common::default_sensory_input();
    sensory.nearest_city = (Vec2::new(1.0, 0.0), 60.0);
    sensory.gold = 100.0;

    let action = brain.decide(&sensory, &mut merchant);
    // Should be moving toward city
    assert!(
        action.speed_mult > 0.0,
        "Should be moving when not at a city"
    );
}

#[test]
fn sells_highest_tier_first() {
    let brain = craftsman_brain();
    let mut merchant = craftsman_merchant();
    merchant.state = AgentState::SellingGoods;
    // Mix of raw (tier 0) and refined (tier 1) goods
    merchant.inventory.insert(Commodity::Ore, 2.0);     // tier 0
    merchant.inventory.insert(Commodity::Tools, 1.0);    // tier 1
    merchant.inventory.insert(Commodity::Weapons, 0.5);  // tier 2

    let sensory = common::sensory_at_city();

    let action = brain.decide(&sensory, &mut merchant);
    match action.market_action {
        MarketAction::Sell { commodity, .. } => {
            assert_eq!(
                commodity,
                Commodity::Weapons,
                "Should sell highest tier (Weapons, tier 2) first"
            );
        }
        _ => panic!("Expected Sell action"),
    }
}

#[test]
fn sells_tier1_before_tier0() {
    let brain = craftsman_brain();
    let mut merchant = craftsman_merchant();
    merchant.state = AgentState::SellingGoods;
    merchant.inventory.insert(Commodity::Ore, 2.0);    // tier 0
    merchant.inventory.insert(Commodity::Tools, 1.0);   // tier 1

    let sensory = common::sensory_at_city();

    let action = brain.decide(&sensory, &mut merchant);
    match action.market_action {
        MarketAction::Sell { commodity, .. } => {
            assert_eq!(
                commodity,
                Commodity::Tools,
                "Should sell tier 1 (Tools) before tier 0 (Ore)"
            );
        }
        _ => panic!("Expected Sell action"),
    }
}

#[test]
fn inventory_full_without_craftable_transitions_to_selling() {
    let brain = craftsman_brain();
    let mut merchant = craftsman_merchant();
    merchant.state = AgentState::BuyingMaterials;
    // Inventory is full but no pair of materials can be crafted
    // Only have one type of raw material
    merchant.inventory.insert(Commodity::Ore, 9.0);

    let mut sensory = common::sensory_at_city();
    sensory.inventory_fill_ratio = 0.9; // > 0.85

    let _action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::SellingGoods,
        "Should transition to SellingGoods when full without craftable pair"
    );
}

#[test]
fn crafting_not_at_city_transitions_to_buying_materials() {
    let brain = craftsman_brain();
    let mut merchant = craftsman_merchant();
    merchant.state = AgentState::Crafting;
    merchant.inventory.insert(Commodity::Timber, 1.0);
    merchant.inventory.insert(Commodity::Ore, 1.0);

    // Far from any city
    let mut sensory = common::default_sensory_input();
    sensory.nearest_city = (Vec2::new(1.0, 0.0), 60.0);

    let _action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::BuyingMaterials,
        "Should go back to BuyingMaterials when not at a city"
    );
}

#[test]
fn selling_done_transitions_to_buying_materials() {
    let brain = craftsman_brain();
    let mut merchant = craftsman_merchant();
    merchant.state = AgentState::SellingGoods;
    // Empty inventory, low fatigue

    let mut sensory = common::sensory_at_city();
    sensory.fatigue = 10.0;
    sensory.inventory_fill_ratio = 0.0;

    let _action = brain.decide(&sensory, &mut merchant);
    assert_eq!(
        merchant.state,
        AgentState::BuyingMaterials,
        "Should go back to buying materials after selling everything"
    );
}
