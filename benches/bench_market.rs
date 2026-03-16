//! Criterion benchmarks for the market order book: order placement,
//! order matching, NPC demand generation, and expiry.
//!
//! Target: order matching < 1ms for a fully loaded book.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use swarm_economy::market::order_book::OrderBook;
use swarm_economy::types::{Commodity, Order, Side};

fn make_buy(agent: u32, commodity: Commodity, price: f32, qty: f32, tick: u32) -> Order {
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

fn make_sell(agent: u32, commodity: Commodity, price: f32, qty: f32, tick: u32) -> Order {
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

fn bench_order_matching(c: &mut Criterion) {
    c.bench_function("order_matching_full_book", |b| {
        b.iter_with_setup(
            || {
                let mut book = OrderBook::new(0, true); // market hall for 200 capacity
                // Fill with overlapping buy/sell orders across multiple commodities.
                for i in 0..50 {
                    for &commodity in &[
                        Commodity::Grain,
                        Commodity::Ore,
                        Commodity::Timber,
                        Commodity::Fish,
                    ] {
                        let buy_price = 10.0 + (i as f32 * 0.5);
                        let sell_price = 5.0 + (i as f32 * 0.3);
                        book.place_order(make_buy(i * 2, commodity, buy_price, 3.0, i));
                        book.place_order(make_sell(i * 2 + 1, commodity, sell_price, 2.0, i));
                    }
                }
                book
            },
            |mut book| {
                let result = book.match_orders(100, 0.05);
                black_box(result);
            },
        );
    });
}

fn bench_order_placement(c: &mut Criterion) {
    c.bench_function("order_placement_100", |b| {
        b.iter_with_setup(
            || OrderBook::new(0, true),
            |mut book| {
                for i in 0..100 {
                    let commodity = Commodity::ALL[i % Commodity::ALL.len()];
                    book.place_order(make_buy(i as u32, commodity, 10.0, 1.0, 0));
                }
                black_box(&book);
            },
        );
    });
}

fn bench_order_expiry(c: &mut Criterion) {
    c.bench_function("order_expiry_200_orders", |b| {
        b.iter_with_setup(
            || {
                let mut book = OrderBook::new(0, true);
                for i in 0..100 {
                    book.place_order(make_buy(i, Commodity::Grain, 10.0, 1.0, i));
                    book.place_order(make_sell(i + 1000, Commodity::Ore, 5.0, 1.0, i));
                }
                book
            },
            |mut book| {
                book.expire_orders(250);
                black_box(&book);
            },
        );
    });
}

fn bench_npc_demand(c: &mut Criterion) {
    c.bench_function("npc_demand_generation", |b| {
        b.iter_with_setup(
            || OrderBook::new(0, false),
            |mut book| {
                let result = book.generate_npc_demand(200.0, 500.0, 100, 200, 0.01);
                black_box(result);
            },
        );
    });
}

fn bench_matching_single_commodity(c: &mut Criterion) {
    c.bench_function("matching_single_commodity_dense", |b| {
        b.iter_with_setup(
            || {
                let mut book = OrderBook::new(0, true);
                for i in 0..100 {
                    book.place_order(make_buy(
                        i * 2,
                        Commodity::Grain,
                        15.0 - i as f32 * 0.1,
                        5.0,
                        i,
                    ));
                    book.place_order(make_sell(
                        i * 2 + 1,
                        Commodity::Grain,
                        5.0 + i as f32 * 0.1,
                        5.0,
                        i,
                    ));
                }
                book
            },
            |mut book| {
                let result = book.match_orders(200, 0.05);
                black_box(result);
            },
        );
    });
}

criterion_group!(
    benches,
    bench_order_matching,
    bench_order_placement,
    bench_order_expiry,
    bench_npc_demand,
    bench_matching_single_commodity,
);
criterion_main!(benches);
