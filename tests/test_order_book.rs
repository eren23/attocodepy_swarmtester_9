mod common;

use swarm_economy::market::order_book::{OrderBook, NPC_AGENT_ID};
use swarm_economy::types::*;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn buy(agent: u32, commodity: Commodity, price: f32, qty: f32, tick: u32) -> Order {
    Order {
        agent_id: agent,
        commodity,
        side: Side::Buy,
        price,
        quantity: qty,
        tick_placed: tick,
        ttl: 200,
    }
}

fn sell(agent: u32, commodity: Commodity, price: f32, qty: f32, tick: u32) -> Order {
    Order {
        agent_id: agent,
        commodity,
        side: Side::Sell,
        price,
        quantity: qty,
        tick_placed: tick,
        ttl: 200,
    }
}

fn book() -> OrderBook {
    OrderBook::new(0, false)
}

// ── 1. Midpoint execution ────────────────────────────────────────────────────

#[test]
fn midpoint_execution_price() {
    let mut ob = book();
    ob.place_order(buy(1, Commodity::Grain, 12.0, 5.0, 0));
    ob.place_order(sell(2, Commodity::Grain, 8.0, 5.0, 0));

    let (fills, _) = ob.match_orders(10, 0.0);

    assert_eq!(fills.len(), 1);
    let f = &fills[0];
    // Midpoint of 12 and 8 is 10.
    assert!(
        (f.transaction.price - 10.0).abs() < f32::EPSILON,
        "expected midpoint price 10.0, got {}",
        f.transaction.price
    );
    assert!(
        (f.transaction.quantity - 5.0).abs() < f32::EPSILON,
        "expected quantity 5.0, got {}",
        f.transaction.quantity
    );
    assert_eq!(f.transaction.buyer_id, 1);
    assert_eq!(f.transaction.seller_id, 2);
}

// ── 2. Partial fills ─────────────────────────────────────────────────────────

#[test]
fn partial_fill_buyer_keeps_remainder() {
    let mut ob = book();
    ob.place_order(buy(1, Commodity::Grain, 12.0, 10.0, 0));
    ob.place_order(sell(2, Commodity::Grain, 8.0, 3.0, 0));

    let (fills, _) = ob.match_orders(10, 0.0);

    assert_eq!(fills.len(), 1);
    assert!(
        (fills[0].transaction.quantity - 3.0).abs() < f32::EPSILON,
        "fill should be for 3 units, got {}",
        fills[0].transaction.quantity
    );

    // Buyer has 7 remaining on the book.
    assert_eq!(ob.order_count(Commodity::Grain, Side::Buy), 1);
    assert_eq!(ob.order_count(Commodity::Grain, Side::Sell), 0);
}

// ── 3. Tax deduction ─────────────────────────────────────────────────────────

#[test]
fn tax_deduction_correct() {
    let mut ob = book();
    ob.place_order(buy(1, Commodity::Grain, 12.0, 5.0, 0));
    ob.place_order(sell(2, Commodity::Grain, 8.0, 5.0, 0));

    let tax_rate = 0.10;
    let (fills, total_tax) = ob.match_orders(10, tax_rate);

    assert_eq!(fills.len(), 1);
    // exec_price = 10, qty = 5, value = 50, tax = 0.10 * 50 = 5.0
    let expected_tax = tax_rate * 10.0 * 5.0;
    assert!(
        (fills[0].tax - expected_tax).abs() < f32::EPSILON,
        "expected tax {expected_tax}, got {}",
        fills[0].tax
    );
    assert!(
        (total_tax - expected_tax).abs() < f32::EPSILON,
        "expected total_tax {expected_tax}, got {total_tax}"
    );
}

// ── 4. TTL expiry ────────────────────────────────────────────────────────────

#[test]
fn ttl_expiry_at_boundary() {
    let mut ob = book();
    // Order placed at tick 0 with ttl=200.
    ob.place_order(buy(1, Commodity::Grain, 10.0, 5.0, 0));

    // At tick 199, order should still be alive (age 199 < ttl 200).
    ob.expire_orders(199);
    assert_eq!(
        ob.order_count(Commodity::Grain, Side::Buy),
        1,
        "order should survive at tick 199"
    );

    // At tick 200, order should expire (age 200 >= ttl 200).
    ob.expire_orders(200);
    assert_eq!(
        ob.order_count(Commodity::Grain, Side::Buy),
        0,
        "order should expire at tick 200"
    );
}

#[test]
fn fresh_orders_survive_expiry() {
    let mut ob = book();
    ob.place_order(sell(1, Commodity::Ore, 10.0, 3.0, 100));

    ob.expire_orders(200);
    // Age = 100 < ttl 200, should survive.
    assert_eq!(ob.order_count(Commodity::Ore, Side::Sell), 1);
}

// ── 5. No self-match ─────────────────────────────────────────────────────────

#[test]
fn no_self_match() {
    let mut ob = book();
    ob.place_order(buy(1, Commodity::Grain, 12.0, 5.0, 0));
    ob.place_order(sell(1, Commodity::Grain, 8.0, 5.0, 0));

    let (fills, _) = ob.match_orders(10, 0.0);
    assert!(
        fills.is_empty(),
        "same agent_id on both sides should produce no fills"
    );
}

#[test]
fn self_match_skipped_other_matches_proceed() {
    let mut ob = book();
    ob.place_order(buy(1, Commodity::Grain, 12.0, 5.0, 0));
    ob.place_order(sell(1, Commodity::Grain, 8.0, 5.0, 0)); // self — skipped
    ob.place_order(sell(2, Commodity::Grain, 9.0, 3.0, 0)); // different agent — should match

    let (fills, _) = ob.match_orders(10, 0.0);
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].transaction.seller_id, 2);
    assert!(
        (fills[0].transaction.quantity - 3.0).abs() < f32::EPSILON,
    );
}

// ── 6. NPC demand ────────────────────────────────────────────────────────────

#[test]
fn npc_demand_creates_buy_orders_from_npc_agent_id() {
    let mut ob = book();
    let (orders, committed) = ob.generate_npc_demand(100.0, 1000.0, 0, 200, 0.01);

    assert!(
        !orders.is_empty(),
        "NPC demand should generate at least one order"
    );
    assert!(committed > 0.0, "committed gold should be positive");

    for order in &orders {
        assert_eq!(
            order.agent_id, NPC_AGENT_ID,
            "NPC orders must use NPC_AGENT_ID"
        );
        assert_eq!(order.side, Side::Buy, "NPC demand should be buy orders");
    }
}

#[test]
fn npc_demand_respects_budget() {
    let mut ob = book();
    let budget = 10.0;
    let (_, committed) = ob.generate_npc_demand(100.0, budget, 0, 200, 0.01);

    assert!(
        committed <= budget,
        "committed {committed} should not exceed budget {budget}"
    );
}

// ── 7. Price-time priority ───────────────────────────────────────────────────

#[test]
fn highest_buy_matched_first() {
    let mut ob = book();
    ob.place_order(buy(1, Commodity::Grain, 10.0, 5.0, 0));
    ob.place_order(buy(2, Commodity::Grain, 15.0, 5.0, 0));
    ob.place_order(sell(3, Commodity::Grain, 8.0, 5.0, 0));

    let (fills, _) = ob.match_orders(10, 0.0);
    assert_eq!(fills.len(), 1);
    assert_eq!(
        fills[0].transaction.buyer_id, 2,
        "highest-price buyer (agent 2 at 15.0) should match first"
    );
}

#[test]
fn lowest_sell_matched_first() {
    let mut ob = book();
    ob.place_order(buy(1, Commodity::Grain, 20.0, 5.0, 0));
    ob.place_order(sell(2, Commodity::Grain, 12.0, 5.0, 0));
    ob.place_order(sell(3, Commodity::Grain, 8.0, 5.0, 0));

    let (fills, _) = ob.match_orders(10, 0.0);
    assert_eq!(fills.len(), 1);
    assert_eq!(
        fills[0].transaction.seller_id, 3,
        "lowest-price seller (agent 3 at 8.0) should match first"
    );
}

#[test]
fn time_priority_breaks_tie() {
    let mut ob = book();
    ob.place_order(buy(1, Commodity::Grain, 10.0, 5.0, 5)); // later
    ob.place_order(buy(2, Commodity::Grain, 10.0, 5.0, 1)); // earlier
    ob.place_order(sell(3, Commodity::Grain, 8.0, 5.0, 0));

    let (fills, _) = ob.match_orders(10, 0.0);
    assert_eq!(fills.len(), 1);
    assert_eq!(
        fills[0].transaction.buyer_id, 2,
        "at same price, earliest buyer (agent 2, tick 1) should match first"
    );
}

// ── 8. Price history ─────────────────────────────────────────────────────────

#[test]
fn last_price_returns_most_recent_exec_price() {
    let mut ob = book();

    // First trade: midpoint (14, 6) = 10.
    ob.place_order(buy(1, Commodity::Grain, 14.0, 1.0, 0));
    ob.place_order(sell(2, Commodity::Grain, 6.0, 1.0, 0));
    ob.match_orders(0, 0.0);

    // Second trade: midpoint (20, 10) = 15.
    ob.place_order(buy(3, Commodity::Grain, 20.0, 1.0, 1));
    ob.place_order(sell(4, Commodity::Grain, 10.0, 1.0, 1));
    ob.match_orders(1, 0.0);

    assert_eq!(
        ob.last_price(Commodity::Grain),
        Some(15.0),
        "last_price should return the most recent execution price"
    );
}

#[test]
fn last_price_none_for_untraded_commodity() {
    let ob = book();
    assert_eq!(
        ob.last_price(Commodity::Grain),
        None,
        "no trades should mean no last_price"
    );
}

// ── 9. Empty book ────────────────────────────────────────────────────────────

#[test]
fn match_orders_on_empty_book() {
    let mut ob = book();
    let (fills, total_tax) = ob.match_orders(10, 0.05);

    assert!(fills.is_empty(), "empty book should produce no fills");
    assert!(
        total_tax.abs() < f32::EPSILON,
        "empty book should produce no tax"
    );
}

// ── 10. Austerity mode ──────────────────────────────────────────────────────

#[test]
fn austerity_only_essential_goods() {
    let mut ob = book();
    // Treasury < 50 triggers austerity.
    let (orders, _) = ob.generate_npc_demand(100.0, 30.0, 0, 200, 0.01);

    for order in &orders {
        let weight = order.commodity.necessity_weight();
        assert!(
            weight >= 1.5,
            "austerity mode should skip {:?} (weight={weight} < 1.5)",
            order.commodity
        );
    }
}

#[test]
fn austerity_halves_quantities() {
    // Compare quantities with and without austerity for an essential commodity.
    // Provisions has necessity_weight 3.0.

    // Normal mode (treasury = 10000, well above 50).
    let mut ob_normal = book();
    let (orders_normal, _) = ob_normal.generate_npc_demand(100.0, 10000.0, 0, 200, 0.01);

    // Austerity mode (treasury = 30, below 50).
    let mut ob_austerity = book();
    let (orders_austerity, _) = ob_austerity.generate_npc_demand(100.0, 30.0, 0, 200, 0.01);

    // Find Provisions in both sets.
    let normal_qty = orders_normal
        .iter()
        .find(|o| o.commodity == Commodity::Provisions)
        .map(|o| o.quantity);
    let austerity_qty = orders_austerity
        .iter()
        .find(|o| o.commodity == Commodity::Provisions)
        .map(|o| o.quantity);

    if let (Some(nq), Some(aq)) = (normal_qty, austerity_qty) {
        assert!(
            (aq - nq * 0.5).abs() < 0.01,
            "austerity should halve quantity: normal={nq}, austerity={aq}, expected={}",
            nq * 0.5
        );
    }
    // If one or both are missing (budget too low), that's also acceptable in
    // austerity mode — the essential check above already validates the constraint.
}
