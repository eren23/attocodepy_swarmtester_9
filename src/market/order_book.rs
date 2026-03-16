use std::collections::{HashMap, VecDeque};

use crate::types::{CityId, Commodity, Order, Side, Transaction};

// ── Constants ────────────────────────────────────────────────────────────

/// Special agent ID for NPC (city-generated) demand orders.
pub const NPC_AGENT_ID: u32 = u32::MAX;

/// Base order book capacity per side per commodity.
const BASE_CAPACITY: usize = 100;

/// Maximum price history entries per commodity.
const PRICE_HISTORY_LEN: usize = 2000;

// ── Price record ─────────────────────────────────────────────────────────

/// A single recorded execution price.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceRecord {
    pub tick: u32,
    pub price: f32,
    pub quantity: f32,
}

// ── Fill ──────────────────────────────────────────────────────────────────

/// Result of a single order match execution.
#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub transaction: Transaction,
    /// Tax deducted from the buyer (tax_rate × execution value).
    pub tax: f32,
}

// ── Order Book ───────────────────────────────────────────────────────────

/// Per-city order book managing orders across all commodities.
pub struct OrderBook {
    city_id: CityId,
    buys: HashMap<Commodity, Vec<Order>>,
    sells: HashMap<Commodity, Vec<Order>>,
    history: HashMap<Commodity, VecDeque<PriceRecord>>,
    has_market_hall: bool,
}

impl OrderBook {
    pub fn new(city_id: CityId, has_market_hall: bool) -> Self {
        Self {
            city_id,
            buys: HashMap::new(),
            sells: HashMap::new(),
            history: HashMap::new(),
            has_market_hall,
        }
    }

    /// Maximum orders per side per commodity.
    pub fn capacity(&self) -> usize {
        if self.has_market_hall {
            BASE_CAPACITY * 2
        } else {
            BASE_CAPACITY
        }
    }

    pub fn city_id(&self) -> CityId {
        self.city_id
    }

    /// Update the MarketHall upgrade status.
    pub fn set_market_hall(&mut self, has_it: bool) {
        self.has_market_hall = has_it;
    }

    /// Number of orders on a given side for a commodity.
    pub fn order_count(&self, commodity: Commodity, side: Side) -> usize {
        match side {
            Side::Buy => self.buys.get(&commodity).map_or(0, Vec::len),
            Side::Sell => self.sells.get(&commodity).map_or(0, Vec::len),
        }
    }

    // ── (1) place_order ──────────────────────────────────────────────

    /// Add an order to the book. Returns `false` if capacity is reached.
    pub fn place_order(&mut self, order: Order) -> bool {
        let cap = self.capacity();
        let book = match order.side {
            Side::Buy => self.buys.entry(order.commodity).or_default(),
            Side::Sell => self.sells.entry(order.commodity).or_default(),
        };
        if book.len() >= cap {
            return false;
        }
        book.push(order);
        true
    }

    // ── (2) match_orders ─────────────────────────────────────────────

    /// Match buy and sell orders using price-time priority.
    ///
    /// Execution price = midpoint of matched buy and sell prices.
    /// Partial fills leave the remainder on the book.
    /// No self-matching (same agent_id).
    /// Tax = `tax_rate × execution_value`, deducted from buyer.
    ///
    /// Returns all fills and total tax collected.
    pub fn match_orders(&mut self, current_tick: u32, tax_rate: f32) -> (Vec<Fill>, f32) {
        let mut all_fills = Vec::new();
        let mut total_tax = 0.0;
        let city_id = self.city_id;

        for commodity in Commodity::ALL {
            let mut buys = self.buys.remove(&commodity).unwrap_or_default();
            let mut sells = self.sells.remove(&commodity).unwrap_or_default();

            let fills =
                match_orders_inner(city_id, commodity, &mut buys, &mut sells, current_tick, tax_rate);

            for fill in fills {
                total_tax += fill.tax;
                all_fills.push(fill);
            }

            if !buys.is_empty() {
                self.buys.insert(commodity, buys);
            }
            if !sells.is_empty() {
                self.sells.insert(commodity, sells);
            }
        }

        // Record price history.
        for fill in &all_fills {
            let history = self.history.entry(fill.transaction.commodity).or_default();
            history.push_back(PriceRecord {
                tick: current_tick,
                price: fill.transaction.price,
                quantity: fill.transaction.quantity,
            });
            if history.len() > PRICE_HISTORY_LEN {
                history.pop_front();
            }
        }

        (all_fills, total_tax)
    }

    // ── (3) expire_orders ────────────────────────────────────────────

    /// Remove orders whose TTL has elapsed. Default TTL is 200 ticks.
    pub fn expire_orders(&mut self, current_tick: u32) {
        for orders in self.buys.values_mut() {
            orders.retain(|o| current_tick.saturating_sub(o.tick_placed) < o.ttl);
        }
        for orders in self.sells.values_mut() {
            orders.retain(|o| current_tick.saturating_sub(o.tick_placed) < o.ttl);
        }
    }

    // ── (4) generate_npc_demand ──────────────────────────────────────

    /// Generate passive NPC buy orders from the city.
    ///
    /// Quantity per commodity = `population × npc_demand_base × necessity_weight`.
    /// Price = last known price × (1.0 + 0.05 × scarcity).
    /// Orders are funded from `treasury`; austerity mode activates when
    /// treasury < 50g (only essentials with weight >= 1.5, halved quantities).
    ///
    /// Returns the orders placed and total gold committed.
    pub fn generate_npc_demand(
        &mut self,
        population: f32,
        treasury: f32,
        current_tick: u32,
        order_ttl: u32,
        npc_demand_base: f32,
    ) -> (Vec<Order>, f32) {
        let austerity = treasury < 50.0;
        let mut orders = Vec::new();
        let mut budget = treasury;

        for commodity in Commodity::ALL {
            let weight = commodity.necessity_weight();

            // Austerity: only essential goods (necessity_weight >= 1.5).
            if austerity && weight < 1.5 {
                continue;
            }

            let mut qty = population * npc_demand_base * weight;
            if austerity {
                qty *= 0.5;
            }
            if qty < 0.001 {
                continue;
            }

            let scarcity = self.compute_scarcity(commodity);
            let base_price = self
                .last_price(commodity)
                .unwrap_or_else(|| default_price(commodity));
            let price = base_price * (1.0 + 0.05 * scarcity);

            let cost = price * qty;
            if cost > budget {
                continue;
            }
            budget -= cost;

            let order = Order {
                agent_id: NPC_AGENT_ID,
                commodity,
                side: Side::Buy,
                price,
                quantity: qty,
                tick_placed: current_tick,
                ttl: order_ttl,
            };
            self.place_order(order.clone());
            orders.push(order);
        }

        let committed = treasury - budget;
        (orders, committed)
    }

    // ── (5) price_history ────────────────────────────────────────────

    /// Price history for a commodity (up to 2000 entries).
    pub fn price_history(&self, commodity: Commodity) -> Option<&VecDeque<PriceRecord>> {
        self.history.get(&commodity)
    }

    /// Most recent execution price for a commodity.
    pub fn last_price(&self, commodity: Commodity) -> Option<f32> {
        self.history
            .get(&commodity)
            .and_then(|h| h.back())
            .map(|r| r.price)
    }

    /// Volume-weighted average price over the last `window` entries.
    pub fn avg_price(&self, commodity: Commodity, window: usize) -> Option<f32> {
        let history = self.history.get(&commodity)?;
        if history.is_empty() {
            return None;
        }
        let start = history.len().saturating_sub(window);
        let mut total_value = 0.0;
        let mut total_qty = 0.0;
        for i in start..history.len() {
            let r = &history[i];
            total_value += r.price * r.quantity;
            total_qty += r.quantity;
        }
        if total_qty > 0.0 {
            Some(total_value / total_qty)
        } else {
            None
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────

    /// Compute scarcity for a commodity based on pending order volumes.
    /// Returns a value in [0, 1]: 1.0 = very scarce, 0.0 = abundant.
    fn compute_scarcity(&self, commodity: Commodity) -> f32 {
        let buy_vol: f32 = self
            .buys
            .get(&commodity)
            .map(|orders| orders.iter().map(|o| o.quantity).sum())
            .unwrap_or(0.0);
        let sell_vol: f32 = self
            .sells
            .get(&commodity)
            .map(|orders| orders.iter().map(|o| o.quantity).sum())
            .unwrap_or(0.0);
        let total = buy_vol + sell_vol;
        if total < 0.001 {
            1.0
        } else {
            buy_vol / total
        }
    }
}

// ── Matching algorithm ───────────────────────────────────────────────────

/// Price-time priority matching. Best buy (highest) vs best sell (lowest).
/// Execute at midpoint. Partial fills update quantities in place.
fn match_orders_inner(
    city_id: CityId,
    commodity: Commodity,
    buys: &mut Vec<Order>,
    sells: &mut Vec<Order>,
    current_tick: u32,
    tax_rate: f32,
) -> Vec<Fill> {
    // Sort buys: highest price first, then earliest tick.
    buys.sort_by(|a, b| {
        b.price
            .partial_cmp(&a.price)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.tick_placed.cmp(&b.tick_placed))
    });

    // Sort sells: lowest price first, then earliest tick.
    sells.sort_by(|a, b| {
        a.price
            .partial_cmp(&b.price)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.tick_placed.cmp(&b.tick_placed))
    });

    let mut fills = Vec::new();

    for buy in buys.iter_mut() {
        if buy.quantity <= 0.0 {
            continue;
        }
        for sell in sells.iter_mut() {
            if sell.quantity <= 0.0 {
                continue;
            }
            if buy.price < sell.price {
                break;
            }
            if buy.agent_id == sell.agent_id {
                continue;
            }

            let exec_price = (buy.price + sell.price) / 2.0;
            let exec_qty = buy.quantity.min(sell.quantity);
            let exec_value = exec_price * exec_qty;
            let tax = tax_rate * exec_value;

            fills.push(Fill {
                transaction: Transaction {
                    tick: current_tick,
                    commodity,
                    price: exec_price,
                    quantity: exec_qty,
                    buyer_id: buy.agent_id,
                    seller_id: sell.agent_id,
                    city_id,
                },
                tax,
            });

            buy.quantity -= exec_qty;
            sell.quantity -= exec_qty;

            if buy.quantity <= 0.0 {
                break;
            }
        }
    }

    buys.retain(|o| o.quantity > 0.0);
    sells.retain(|o| o.quantity > 0.0);

    fills
}

// ── Dynamic tax adjustment ───────────────────────────────────────────────

/// Compute a new tax rate considering multiple economic factors.
///
/// Factors:
/// - Trade volume vs city average (encourage/discourage trade)
/// - Treasury health (raise if critical, lower if flush)
/// - Population pressure (higher pop → more services → higher tax)
///
/// Returns the adjusted rate, clamped to [0.0, 0.15].
pub fn compute_dynamic_tax(
    current_tax: f32,
    trade_volume: f32,
    avg_trade_volume: f32,
    treasury: f32,
    population: f32,
    max_population: f32,
) -> f32 {
    let mut adjustment = 0.0;

    // Trade volume: raise if city is booming, lower to attract traders.
    if trade_volume > avg_trade_volume * 1.1 {
        adjustment += 0.005;
    } else if trade_volume < avg_trade_volume * 0.9 {
        adjustment -= 0.005;
    }

    // Treasury health.
    if treasury < 50.0 {
        adjustment += 0.01;
    } else if treasury > 500.0 {
        adjustment -= 0.005;
    }

    // Population pressure.
    if max_population > 0.0 && population / max_population > 0.8 {
        adjustment += 0.005;
    }

    (current_tax + adjustment).clamp(0.0, 0.15)
}

// ── Default prices ───────────────────────────────────────────────────────

/// Default commodity price when no history exists, based on tier.
fn default_price(commodity: Commodity) -> f32 {
    match commodity.tier() {
        0 => 5.0,
        1 => 15.0,
        2 => 35.0,
        _ => 80.0,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn buy_order(agent: u32, commodity: Commodity, price: f32, qty: f32, tick: u32) -> Order {
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

    fn sell_order(agent: u32, commodity: Commodity, price: f32, qty: f32, tick: u32) -> Order {
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

    // ── place_order ──────────────────────────────────────────────────

    #[test]
    fn place_order_adds_to_book() {
        let mut ob = book();
        let order = buy_order(1, Commodity::Grain, 10.0, 5.0, 0);
        assert!(ob.place_order(order));
        assert_eq!(ob.order_count(Commodity::Grain, Side::Buy), 1);
    }

    #[test]
    fn place_order_respects_capacity() {
        let mut ob = book();
        for i in 0..BASE_CAPACITY {
            assert!(ob.place_order(buy_order(i as u32, Commodity::Grain, 10.0, 1.0, 0)));
        }
        assert!(!ob.place_order(buy_order(999, Commodity::Grain, 10.0, 1.0, 0)));
    }

    #[test]
    fn market_hall_doubles_capacity() {
        let mut ob = OrderBook::new(0, true);
        for i in 0..(BASE_CAPACITY * 2) {
            assert!(ob.place_order(buy_order(i as u32, Commodity::Grain, 10.0, 1.0, 0)));
        }
        assert!(!ob.place_order(buy_order(9999, Commodity::Grain, 10.0, 1.0, 0)));
    }

    #[test]
    fn set_market_hall_updates_capacity() {
        let mut ob = book();
        assert_eq!(ob.capacity(), BASE_CAPACITY);
        ob.set_market_hall(true);
        assert_eq!(ob.capacity(), BASE_CAPACITY * 2);
    }

    // ── match_orders ─────────────────────────────────────────────────

    #[test]
    fn basic_match() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 12.0, 5.0, 0));
        ob.place_order(sell_order(2, Commodity::Grain, 8.0, 5.0, 0));

        let (fills, tax) = ob.match_orders(10, 0.05);

        assert_eq!(fills.len(), 1);
        let f = &fills[0];
        assert_eq!(f.transaction.commodity, Commodity::Grain);
        assert!((f.transaction.price - 10.0).abs() < f32::EPSILON);
        assert!((f.transaction.quantity - 5.0).abs() < f32::EPSILON);
        assert_eq!(f.transaction.buyer_id, 1);
        assert_eq!(f.transaction.seller_id, 2);
        // tax = 0.05 × 10.0 × 5.0 = 2.5
        assert!((f.tax - 2.5).abs() < f32::EPSILON);
        assert!((tax - 2.5).abs() < f32::EPSILON);

        assert_eq!(ob.order_count(Commodity::Grain, Side::Buy), 0);
        assert_eq!(ob.order_count(Commodity::Grain, Side::Sell), 0);
    }

    #[test]
    fn no_match_when_buy_below_sell() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 5.0, 5.0, 0));
        ob.place_order(sell_order(2, Commodity::Grain, 10.0, 5.0, 0));

        let (fills, _) = ob.match_orders(10, 0.05);
        assert!(fills.is_empty());
        assert_eq!(ob.order_count(Commodity::Grain, Side::Buy), 1);
        assert_eq!(ob.order_count(Commodity::Grain, Side::Sell), 1);
    }

    #[test]
    fn partial_fill() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 12.0, 10.0, 0));
        ob.place_order(sell_order(2, Commodity::Grain, 8.0, 3.0, 0));

        let (fills, _) = ob.match_orders(10, 0.0);
        assert_eq!(fills.len(), 1);
        assert!((fills[0].transaction.quantity - 3.0).abs() < f32::EPSILON);

        // Buyer has 7 remaining.
        assert_eq!(ob.order_count(Commodity::Grain, Side::Buy), 1);
        assert_eq!(ob.order_count(Commodity::Grain, Side::Sell), 0);
    }

    #[test]
    fn price_priority() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 10.0, 5.0, 0));
        ob.place_order(buy_order(2, Commodity::Grain, 15.0, 5.0, 1));
        ob.place_order(sell_order(3, Commodity::Grain, 8.0, 5.0, 0));

        let (fills, _) = ob.match_orders(10, 0.0);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].transaction.buyer_id, 2);
    }

    #[test]
    fn time_priority_at_same_price() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 10.0, 5.0, 5));
        ob.place_order(buy_order(2, Commodity::Grain, 10.0, 5.0, 1)); // earlier
        ob.place_order(sell_order(3, Commodity::Grain, 8.0, 5.0, 0));

        let (fills, _) = ob.match_orders(10, 0.0);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].transaction.buyer_id, 2);
    }

    #[test]
    fn no_self_matching() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 12.0, 5.0, 0));
        ob.place_order(sell_order(1, Commodity::Grain, 8.0, 5.0, 0));

        let (fills, _) = ob.match_orders(10, 0.0);
        assert!(fills.is_empty());
    }

    #[test]
    fn self_match_skipped_but_other_matches() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 12.0, 5.0, 0));
        ob.place_order(sell_order(1, Commodity::Grain, 8.0, 5.0, 0));
        ob.place_order(sell_order(2, Commodity::Grain, 9.0, 3.0, 0));

        let (fills, _) = ob.match_orders(10, 0.0);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].transaction.seller_id, 2);
        assert!((fills[0].transaction.quantity - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn multiple_sells_match_one_buy() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 15.0, 10.0, 0));
        ob.place_order(sell_order(2, Commodity::Grain, 5.0, 3.0, 0));
        ob.place_order(sell_order(3, Commodity::Grain, 7.0, 4.0, 1));

        let (fills, _) = ob.match_orders(10, 0.0);
        assert_eq!(fills.len(), 2);
        // First: buy@15 vs sell@5 → midpoint 10, qty 3
        assert_eq!(fills[0].transaction.seller_id, 2);
        assert!((fills[0].transaction.price - 10.0).abs() < f32::EPSILON);
        assert!((fills[0].transaction.quantity - 3.0).abs() < f32::EPSILON);
        // Second: buy@15 vs sell@7 → midpoint 11, qty 4
        assert_eq!(fills[1].transaction.seller_id, 3);
        assert!((fills[1].transaction.price - 11.0).abs() < f32::EPSILON);
        assert!((fills[1].transaction.quantity - 4.0).abs() < f32::EPSILON);

        // Buy has 3 remaining.
        assert_eq!(ob.order_count(Commodity::Grain, Side::Buy), 1);
    }

    #[test]
    fn tax_calculation() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 20.0, 4.0, 0));
        ob.place_order(sell_order(2, Commodity::Grain, 10.0, 4.0, 0));

        let (fills, total_tax) = ob.match_orders(10, 0.10);
        // Midpoint = 15.0, qty = 4, value = 60, tax = 6.0
        assert!((fills[0].tax - 6.0).abs() < f32::EPSILON);
        assert!((total_tax - 6.0).abs() < f32::EPSILON);
    }

    // ── expire_orders ────────────────────────────────────────────────

    #[test]
    fn expire_removes_old_orders() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 10.0, 5.0, 0)); // ttl=200
        ob.place_order(buy_order(2, Commodity::Grain, 10.0, 5.0, 100));

        ob.expire_orders(200);
        // tick 0: age 200 >= ttl 200 → expired.
        // tick 100: age 100 < ttl 200 → kept.
        assert_eq!(ob.order_count(Commodity::Grain, Side::Buy), 1);
    }

    #[test]
    fn expire_keeps_fresh_orders() {
        let mut ob = book();
        ob.place_order(sell_order(1, Commodity::Grain, 10.0, 5.0, 50));

        ob.expire_orders(100);
        assert_eq!(ob.order_count(Commodity::Grain, Side::Sell), 1);
    }

    // ── price_history ────────────────────────────────────────────────

    #[test]
    fn price_recorded_on_match() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 12.0, 5.0, 0));
        ob.place_order(sell_order(2, Commodity::Grain, 8.0, 5.0, 0));

        ob.match_orders(10, 0.0);
        assert_eq!(ob.last_price(Commodity::Grain), Some(10.0));
    }

    #[test]
    fn price_history_capped() {
        let mut ob = book();
        for tick in 0..2500u32 {
            ob.place_order(buy_order(1, Commodity::Grain, 12.0, 1.0, tick));
            ob.place_order(sell_order(2, Commodity::Grain, 8.0, 1.0, tick));
            ob.match_orders(tick, 0.0);
        }
        let history = ob.price_history(Commodity::Grain).unwrap();
        assert_eq!(history.len(), PRICE_HISTORY_LEN);
    }

    #[test]
    fn avg_price_calculation() {
        let mut ob = book();
        // 3 trades at different midpoints, qty 1 each.
        for (tick, sell_price) in [(0u32, 6.0f32), (1, 8.0), (2, 10.0)] {
            ob.place_order(buy_order(1, Commodity::Grain, 14.0, 1.0, tick));
            ob.place_order(sell_order(2, Commodity::Grain, sell_price, 1.0, tick));
            ob.match_orders(tick, 0.0);
        }
        // Prices: midpoint(14,6)=10, midpoint(14,8)=11, midpoint(14,10)=12
        // VWAP = (10+11+12)/3 = 11.0
        let avg = ob.avg_price(Commodity::Grain, 3).unwrap();
        assert!((avg - 11.0).abs() < f32::EPSILON);
    }

    // ── generate_npc_demand ──────────────────────────────────────────

    #[test]
    fn npc_demand_generates_orders() {
        let mut ob = book();
        let (orders, committed) = ob.generate_npc_demand(100.0, 1000.0, 0, 200, 0.01);
        assert!(!orders.is_empty());
        assert!(committed > 0.0);
        for order in &orders {
            assert_eq!(order.agent_id, NPC_AGENT_ID);
            assert_eq!(order.side, Side::Buy);
        }
    }

    #[test]
    fn npc_demand_austerity_mode() {
        let mut ob = book();
        let (orders, _) = ob.generate_npc_demand(100.0, 30.0, 0, 200, 0.01);
        for order in &orders {
            assert!(
                order.commodity.necessity_weight() >= 1.5,
                "austerity should skip {:?} (weight {})",
                order.commodity,
                order.commodity.necessity_weight()
            );
        }
    }

    #[test]
    fn npc_demand_respects_budget() {
        let mut ob = book();
        let (_, committed) = ob.generate_npc_demand(100.0, 10.0, 0, 200, 0.01);
        assert!(committed <= 10.0);
    }

    // ── dynamic tax ──────────────────────────────────────────────────

    #[test]
    fn dynamic_tax_raises_on_high_volume() {
        let rate = compute_dynamic_tax(0.05, 200.0, 100.0, 200.0, 250.0, 500.0);
        assert!(rate > 0.05);
    }

    #[test]
    fn dynamic_tax_lowers_on_low_volume() {
        let rate = compute_dynamic_tax(0.05, 50.0, 100.0, 200.0, 250.0, 500.0);
        assert!(rate < 0.05);
    }

    #[test]
    fn dynamic_tax_raises_when_treasury_low() {
        let rate = compute_dynamic_tax(0.05, 100.0, 100.0, 30.0, 250.0, 500.0);
        assert!(rate > 0.05);
    }

    #[test]
    fn dynamic_tax_clamped() {
        let rate = compute_dynamic_tax(0.15, 200.0, 100.0, 10.0, 450.0, 500.0);
        assert!(rate <= 0.15);
        let rate = compute_dynamic_tax(0.0, 50.0, 100.0, 600.0, 100.0, 500.0);
        assert!(rate >= 0.0);
    }

    // ── scarcity ─────────────────────────────────────────────────────

    #[test]
    fn scarcity_all_buys() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 10.0, 5.0, 0));
        assert!((ob.compute_scarcity(Commodity::Grain) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scarcity_all_sells() {
        let mut ob = book();
        ob.place_order(sell_order(1, Commodity::Grain, 10.0, 5.0, 0));
        assert!((ob.compute_scarcity(Commodity::Grain) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scarcity_balanced() {
        let mut ob = book();
        ob.place_order(buy_order(1, Commodity::Grain, 10.0, 5.0, 0));
        ob.place_order(sell_order(2, Commodity::Grain, 10.0, 5.0, 0));
        assert!((ob.compute_scarcity(Commodity::Grain) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn scarcity_no_orders() {
        let ob = book();
        assert!((ob.compute_scarcity(Commodity::Grain) - 1.0).abs() < f32::EPSILON);
    }
}
