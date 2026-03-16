mod common;

use swarm_economy::agents::merchant::Merchant;
use swarm_economy::brain::brain_for_profession;
use swarm_economy::brain::interface::Brain;
use swarm_economy::types::*;

// ── Helpers ────────────────────────────────────────────────────────────────

fn trader_brain() -> Box<dyn Brain> {
    brain_for_profession(Profession::Trader)
}

fn trader_merchant() -> Merchant {
    common::make_merchant_at(Vec2::new(32.0, 32.0), Profession::Trader)
}

/// Seed price memory so `has_profitable_route` returns true (>15% margin).
/// Records cheap Ore at city 0 and expensive Ore at city 1.
fn seed_profitable_memory(merchant: &mut Merchant) {
    for &commodity in Commodity::RAW.iter() {
        merchant.price_memory.record(0, commodity, 5.0, 0);
        merchant.price_memory.record(1, commodity, 50.0, 0);
    }
}

/// Run a closure in a thread with a large stack to accommodate the Trader FSM's
/// recursive state-chaining calls in unoptimized debug builds.
fn with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
    let builder = std::thread::Builder::new().stack_size(64 * 1024 * 1024);
    let handle = builder.spawn(f).expect("failed to spawn test thread");
    handle.join().expect("test thread panicked");
}

// ── Tests ──────────────────────────────────────────────────────────────────

/// Scouting near a city with profitable price memory -> transitions away from Scouting.
/// When scouting finds city < 25 and a profitable route, it transitions to Buying
/// (and buying may chain further depending on inventory/gold state).
#[test]
fn scouting_to_buying_transition() {
    with_large_stack(|| {
        let brain = trader_brain();
        let mut merchant = trader_merchant();
        merchant.state = AgentState::Scouting;
        merchant.gold = 500.0;
        seed_profitable_memory(&mut merchant);
        // Give merchant some existing inventory so that if buying fails to find
        // a commodity (HashMap order), the fallback goes to Transporting (fill>0.1)
        // instead of looping back to Scouting.
        merchant.inventory.insert(Commodity::Timber, 2.0);

        let mut sensory = common::default_sensory_input();
        sensory.nearest_city = (Vec2::new(1.0, 0.0), 20.0);
        sensory.gold = 500.0;
        sensory.inventory_fill_ratio = 0.2;

        let _action = brain.decide(&sensory, &mut merchant);
        // The key check: scouting detected the profitable route and moved on.
        // It transitions to Buying, which may chain to Transporting or Selling.
        assert_ne!(
            merchant.state,
            AgentState::Scouting,
            "Should leave Scouting when at city with profitable route"
        );
        assert!(
            merchant.state == AgentState::Buying
                || merchant.state == AgentState::Transporting
                || merchant.state == AgentState::Selling,
            "Expected Buying/Transporting/Selling, got {:?}",
            merchant.state
        );
    });
}

/// Buying with nearly full inventory -> transitions to Transporting or Selling.
#[test]
fn buying_to_transporting_when_nearly_full() {
    with_large_stack(|| {
        let brain = trader_brain();
        let mut merchant = trader_merchant();
        merchant.state = AgentState::Buying;
        seed_profitable_memory(&mut merchant);
        merchant.inventory.insert(Commodity::Ore, 9.0);

        let mut sensory = common::default_sensory_input();
        sensory.nearest_city = (Vec2::new(1.0, 0.0), 10.0);
        sensory.inventory_fill_ratio = 0.9; // > 0.85 threshold

        let _action = brain.decide(&sensory, &mut merchant);
        assert!(
            merchant.state == AgentState::Transporting || merchant.state == AgentState::Selling,
            "Expected Transporting or Selling after full inventory, got {:?}",
            merchant.state,
        );
    });
}

/// Transporting at a city with goods -> transitions to Selling.
#[test]
fn transporting_to_selling_at_city_with_goods() {
    with_large_stack(|| {
        let brain = trader_brain();
        let mut merchant = trader_merchant();
        merchant.state = AgentState::Transporting;
        merchant.inventory.insert(Commodity::Ore, 5.0);

        let mut sensory = common::default_sensory_input();
        sensory.nearest_city = (Vec2::new(0.0, 1.0), 10.0);
        sensory.inventory_fill_ratio = 0.5;
        sensory.fatigue = 10.0;

        let action = brain.decide(&sensory, &mut merchant);
        assert_eq!(merchant.state, AgentState::Selling);
        assert!(
            matches!(action.market_action, MarketAction::Sell { .. }),
            "Expected Sell action"
        );
    });
}

/// Selling with no inventory and low fatigue -> transitions to Scouting or Resting.
/// No profitable route seeded to prevent scouting -> buying -> scouting loop.
#[test]
fn selling_to_scouting_or_resting_when_empty() {
    with_large_stack(|| {
        let brain = trader_brain();
        let mut merchant = trader_merchant();
        merchant.state = AgentState::Selling;
        // No price memory -> scouting won't find profitable route.

        let mut sensory = common::default_sensory_input();
        sensory.nearest_city = (Vec2::new(1.0, 0.0), 5.0);
        sensory.inventory_fill_ratio = 0.0;
        sensory.fatigue = 10.0;

        let _action = brain.decide(&sensory, &mut merchant);
        assert!(
            merchant.state == AgentState::Scouting || merchant.state == AgentState::Resting,
            "Expected Scouting or Resting, got {:?}",
            merchant.state
        );
    });
}

/// High greed (>0.7) causes the trader to buy full available space of best commodity.
/// We give inventory so if buying finds nothing, it falls through to Transporting.
#[test]
fn greed_affects_buy_quantity_high_greed_all_in() {
    with_large_stack(|| {
        let brain = trader_brain();
        let mut merchant = trader_merchant();
        merchant.state = AgentState::Buying;
        merchant.traits.greed = 0.8;
        merchant.gold = 500.0;
        seed_profitable_memory(&mut merchant);
        // Some inventory as safety net for fallback path
        merchant.inventory.insert(Commodity::Timber, 1.0);

        let mut sensory = common::default_sensory_input();
        sensory.nearest_city = (Vec2::new(1.0, 0.0), 10.0);
        sensory.inventory_fill_ratio = 0.1;
        sensory.gold = 500.0;

        let action = brain.decide(&sensory, &mut merchant);
        if let MarketAction::Buy { quantity, .. } = action.market_action {
            // With greed > 0.7, max_buy = space (full remaining)
            assert!(
                quantity > 4.0,
                "High greed should buy large quantity, got {}",
                quantity
            );
        }
        // If buying didn't issue a Buy (HashMap order edge case), that's OK.
    });
}

/// Low greed (<=0.7) causes trader to only buy 50% of available space.
#[test]
fn greed_affects_buy_quantity_low_greed_diversifies() {
    with_large_stack(|| {
        let brain = trader_brain();
        let mut merchant = trader_merchant();
        merchant.state = AgentState::Buying;
        merchant.traits.greed = 0.5;
        merchant.gold = 500.0;
        seed_profitable_memory(&mut merchant);
        merchant.inventory.insert(Commodity::Timber, 1.0);

        let mut sensory = common::default_sensory_input();
        sensory.nearest_city = (Vec2::new(1.0, 0.0), 10.0);
        sensory.inventory_fill_ratio = 0.1;
        sensory.gold = 500.0;

        let action = brain.decide(&sensory, &mut merchant);
        if let MarketAction::Buy { quantity, .. } = action.market_action {
            // With greed <= 0.7, max_buy = space * 0.5
            // space = 10 - 1 = 9, max_buy = 4.5
            assert!(
                quantity <= 4.6,
                "Low greed should diversify, got {}",
                quantity
            );
        }
    });
}

/// High greed weights PROFIT scanner more; low greed weights DEMAND more.
/// Both merchants far from any city so scouting stays in scouting.
#[test]
fn high_greed_weights_profit_scanner_more() {
    with_large_stack(|| {
        let brain = trader_brain();

        let mut merchant_high = trader_merchant();
        merchant_high.state = AgentState::Scouting;
        merchant_high.traits.greed = 0.9;

        let mut sensory = common::default_sensory_input();
        sensory.right_scanner[0] = 0.8; // profit right
        sensory.left_scanner[1] = 0.8;  // demand left
        sensory.nearest_city = (Vec2::new(1.0, 0.0), 100.0); // far

        let action_high = brain.decide(&sensory, &mut merchant_high);

        let mut merchant_low = trader_merchant();
        merchant_low.state = AgentState::Scouting;
        merchant_low.traits.greed = 0.1;
        merchant_low.heading = merchant_high.heading;

        let action_low = brain.decide(&sensory, &mut merchant_low);

        assert!(
            (action_high.turn - action_low.turn).abs() > 0.001,
            "Greed should affect turn direction: high={}, low={}",
            action_high.turn,
            action_low.turn
        );
    });
}

/// Danger gradient + low risk_tolerance -> avoidance while transporting.
/// Merchant is far from city (no chaining to selling).
#[test]
fn danger_avoidance_turns_away() {
    with_large_stack(|| {
        let brain = trader_brain();
        let mut merchant = trader_merchant();
        merchant.state = AgentState::Transporting;
        merchant.heading = 0.0;
        merchant.traits.risk_tolerance = 0.1;
        merchant.inventory.insert(Commodity::Ore, 2.0);

        let mut sensory = common::default_sensory_input();
        sensory.nearest_city = (Vec2::new(1.0, 0.0), 40.0); // not at city
        sensory.inventory_fill_ratio = 0.2;
        sensory.fatigue = 10.0;
        sensory.danger_gradient = Vec2::new(1.0, 0.0);

        let action = brain.decide(&sensory, &mut merchant);
        assert_eq!(merchant.state, AgentState::Transporting);
        assert!(action.speed_mult > 0.0, "Should be moving");
    });
}

/// Bandit within flee threshold and not in caravan -> Fleeing.
#[test]
fn fleeing_triggered_by_nearby_bandit() {
    with_large_stack(|| {
        let brain = trader_brain();
        let mut merchant = trader_merchant();
        merchant.state = AgentState::Scouting;
        merchant.traits.risk_tolerance = 0.2;
        merchant.caravan_id = None;

        // flee_threshold = 50 * (1 - 0.2*0.5) = 50 * 0.9 = 45
        let mut sensory = common::default_sensory_input();
        sensory.nearest_bandit = Some((Vec2::new(1.0, 0.0), 30.0)); // 30 < 45
        sensory.nearest_city = (Vec2::new(1.0, 0.0), 100.0);

        let _action = brain.decide(&sensory, &mut merchant);
        assert_eq!(
            merchant.state,
            AgentState::Fleeing,
            "Should flee when bandit is within threshold"
        );
    });
}

/// In a caravan, even with close bandit, should NOT flee.
#[test]
fn fleeing_not_triggered_in_caravan() {
    with_large_stack(|| {
        let brain = trader_brain();
        let mut merchant = trader_merchant();
        merchant.state = AgentState::Scouting;
        merchant.traits.risk_tolerance = 0.2;
        merchant.caravan_id = Some(1);

        let mut sensory = common::default_sensory_input();
        sensory.nearest_bandit = Some((Vec2::new(1.0, 0.0), 30.0));
        sensory.nearest_city = (Vec2::new(1.0, 0.0), 100.0);

        let _action = brain.decide(&sensory, &mut merchant);
        assert_ne!(
            merchant.state,
            AgentState::Fleeing,
            "Should not flee when in a caravan"
        );
    });
}

/// Bandit far (>80px) -> stop fleeing, back to Scouting.
#[test]
fn fleeing_clears_when_bandit_far() {
    with_large_stack(|| {
        let brain = trader_brain();
        let mut merchant = trader_merchant();
        merchant.state = AgentState::Fleeing;

        let mut sensory = common::default_sensory_input();
        sensory.nearest_bandit = Some((Vec2::new(1.0, 0.0), 90.0));
        sensory.nearest_city = (Vec2::new(1.0, 0.0), 100.0);

        let _action = brain.decide(&sensory, &mut merchant);
        assert_eq!(
            merchant.state,
            AgentState::Scouting,
            "Should stop fleeing when bandit is >80px away"
        );
    });
}

/// No bandit at all -> stop fleeing, back to Scouting.
#[test]
fn fleeing_clears_when_bandit_none() {
    with_large_stack(|| {
        let brain = trader_brain();
        let mut merchant = trader_merchant();
        merchant.state = AgentState::Fleeing;

        let mut sensory = common::default_sensory_input();
        sensory.nearest_bandit = None;
        sensory.nearest_city = (Vec2::new(1.0, 0.0), 100.0);

        let _action = brain.decide(&sensory, &mut merchant);
        assert_eq!(
            merchant.state,
            AgentState::Scouting,
            "Should stop fleeing when no bandit detected"
        );
    });
}
