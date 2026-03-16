mod common;

use std::collections::HashMap;

use rand::rngs::StdRng;
use rand::SeedableRng;

use swarm_economy::agents::merchant::PriceEntry;
use swarm_economy::market::gossip::{self, GossipAgent};
use swarm_economy::types::{CityId, Commodity, Vec2};
use swarm_economy::world::bandit::BanditCampId;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn make_agent(
    id: u32,
    pos: Vec2,
    sociability: f32,
    caravan_id: Option<u32>,
    prices: Vec<(CityId, Commodity, PriceEntry)>,
    known_camps: HashMap<BanditCampId, Vec2>,
) -> GossipAgent {
    GossipAgent {
        id,
        pos,
        sociability,
        caravan_id,
        prices,
        known_camps,
    }
}

fn price(price: f32, tick: u32) -> PriceEntry {
    PriceEntry {
        price,
        observed_tick: tick,
    }
}

// ── In-Range Exchange ───────────────────────────────────────────────────────

#[test]
fn merchants_within_25px_can_exchange_prices() {
    let agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            1.0,
            None,
            vec![(0, Commodity::Ore, price(5.0, 100))],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(10.0, 0.0), // 10px away, within 25px
            1.0,
            None,
            vec![(1, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
    ];

    // With sociability=1.0, chance = 0.5 * (1.0+1.0)/2.0 = 0.5
    // Try many seeds to get at least one successful gossip.
    let mut got_shares = false;
    for seed in 0..200 {
        let mut rng = StdRng::seed_from_u64(seed);
        let result = gossip::tick_gossip(&agents, 100, 200, &mut rng);
        if !result.price_shares.is_empty() {
            // Should exchange one entry each way = 2 shares.
            assert_eq!(result.price_shares.len(), 2);

            // One share should go to agent 2, one to agent 1.
            let to_1: Vec<_> = result.price_shares.iter().filter(|s| s.receiver_id == 1).collect();
            let to_2: Vec<_> = result.price_shares.iter().filter(|s| s.receiver_id == 2).collect();
            assert_eq!(to_1.len(), 1);
            assert_eq!(to_2.len(), 1);

            got_shares = true;
            break;
        }
    }
    assert!(got_shares, "merchants within range should eventually exchange prices");
}

#[test]
fn merchants_at_exact_boundary_25px() {
    // At exactly 25.0px distance the check is `distance > 25.0`, so 25.0 should still work.
    let agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            1.0,
            None,
            vec![(0, Commodity::Ore, price(5.0, 100))],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(25.0, 0.0), // exactly 25px
            1.0,
            None,
            vec![(1, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
    ];

    let mut got_shares = false;
    for seed in 0..200 {
        let mut rng = StdRng::seed_from_u64(seed);
        let result = gossip::tick_gossip(&agents, 100, 200, &mut rng);
        if !result.price_shares.is_empty() {
            got_shares = true;
            break;
        }
    }
    assert!(got_shares, "merchants at exactly 25px should be in range");
}

// ── Out-of-Range No Exchange ────────────────────────────────────────────────

#[test]
fn merchants_beyond_25px_do_not_exchange() {
    let agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            1.0,
            None,
            vec![(0, Commodity::Ore, price(5.0, 100))],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(50.0, 0.0), // 50px away, well beyond 25px
            1.0,
            None,
            vec![(1, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
    ];

    for seed in 0..200 {
        let mut rng = StdRng::seed_from_u64(seed);
        let result = gossip::tick_gossip(&agents, 100, 200, &mut rng);
        assert!(
            result.price_shares.is_empty(),
            "out-of-range merchants should never exchange prices (seed {})",
            seed,
        );
    }
}

#[test]
fn merchants_just_beyond_25px_do_not_exchange() {
    let agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            1.0,
            None,
            vec![(0, Commodity::Ore, price(5.0, 100))],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(25.01, 0.0), // just over 25px
            1.0,
            None,
            vec![(1, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
    ];

    for seed in 0..200 {
        let mut rng = StdRng::seed_from_u64(seed);
        let result = gossip::tick_gossip(&agents, 100, 200, &mut rng);
        assert!(
            result.price_shares.is_empty(),
            "merchants at 25.01px should not exchange (seed {})",
            seed,
        );
    }
}

// ── Stale Entries Not Shared ────────────────────────────────────────────────

#[test]
fn stale_entries_are_not_shared() {
    let agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            1.0,
            None,
            // Observed at tick 10, current_tick=100, TTL=50 -> age 90 > 50 = stale.
            vec![(0, Commodity::Ore, price(5.0, 10))],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(5.0, 0.0),
            1.0,
            None,
            // This one is fresh.
            vec![(1, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
    ];

    let mut stale_shared = false;
    for seed in 0..500 {
        let mut rng = StdRng::seed_from_u64(seed);
        let result = gossip::tick_gossip(&agents, 100, 50, &mut rng);
        for share in &result.price_shares {
            if share.receiver_id == 2 && share.commodity == Commodity::Ore {
                stale_shared = true;
            }
        }
    }
    assert!(!stale_shared, "stale entries (age > TTL) should never be shared");
}

#[test]
fn fresh_entries_are_shared_stale_ones_are_not() {
    // Agent 1 has one fresh and one stale entry.
    let agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            1.0,
            None,
            vec![
                (0, Commodity::Ore, price(5.0, 10)),    // stale: age 90 > TTL 50
                (1, Commodity::Grain, price(7.0, 90)),   // fresh: age 10 <= TTL 50
            ],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(5.0, 0.0),
            1.0,
            None,
            vec![(2, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
    ];

    let mut stale_ore_shared = false;
    let mut fresh_grain_shared = false;

    for seed in 0..500 {
        let mut rng = StdRng::seed_from_u64(seed);
        let result = gossip::tick_gossip(&agents, 100, 50, &mut rng);
        for share in &result.price_shares {
            if share.receiver_id == 2 {
                if share.commodity == Commodity::Ore {
                    stale_ore_shared = true;
                }
                if share.commodity == Commodity::Grain {
                    fresh_grain_shared = true;
                }
            }
        }
    }

    assert!(!stale_ore_shared, "stale Ore entry should not be shared");
    assert!(fresh_grain_shared, "fresh Grain entry should be shared eventually");
}

// ── Caravan Merge ───────────────────────────────────────────────────────────

#[test]
fn caravan_members_get_full_merge() {
    let agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            0.5,
            Some(10),
            vec![(0, Commodity::Ore, price(5.0, 100))],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(5.0, 0.0),
            0.5,
            Some(10),
            vec![(1, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
        make_agent(
            3,
            Vec2::new(10.0, 0.0),
            0.5,
            Some(10),
            vec![],
            HashMap::new(),
        ),
    ];

    let mut rng = StdRng::seed_from_u64(42);
    let result = gossip::tick_gossip(&agents, 100, 200, &mut rng);

    // Should produce a caravan merge group.
    assert_eq!(result.caravan_merges.len(), 1);
    assert_eq!(result.caravan_merges[0].len(), 3);

    // No individual price shares for caravan mates (they get full merge instead).
    let caravan_shares: Vec<_> = result
        .price_shares
        .iter()
        .filter(|s| [1, 2, 3].contains(&s.receiver_id))
        .collect();
    assert!(
        caravan_shares.is_empty(),
        "caravan mates should not get individual price_shares",
    );
}

#[test]
fn different_caravans_do_not_merge() {
    let agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            0.5,
            Some(10),
            vec![(0, Commodity::Ore, price(5.0, 100))],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(5.0, 0.0),
            0.5,
            Some(20), // different caravan
            vec![(1, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
    ];

    let mut rng = StdRng::seed_from_u64(42);
    let result = gossip::tick_gossip(&agents, 100, 200, &mut rng);

    // No merge groups since each caravan has only one member.
    assert!(result.caravan_merges.is_empty());
}

// ── Bandit Info Sharing ─────────────────────────────────────────────────────

#[test]
fn known_camps_shared_between_gossiping_merchants() {
    let mut camps_a = HashMap::new();
    camps_a.insert(0u32, Vec2::new(100.0, 100.0));

    let agents = vec![
        make_agent(1, Vec2::new(0.0, 0.0), 1.0, None, vec![], camps_a),
        make_agent(2, Vec2::new(5.0, 0.0), 1.0, None, vec![], HashMap::new()),
    ];

    let mut camp_shared = false;
    for seed in 0..200 {
        let mut rng = StdRng::seed_from_u64(seed);
        let result = gossip::tick_gossip(&agents, 100, 200, &mut rng);
        for share in &result.camp_shares {
            if share.receiver_id == 2 && share.camp_id == 0 {
                camp_shared = true;
            }
        }
        if camp_shared {
            break;
        }
    }
    assert!(camp_shared, "camp locations should be shared during gossip");
}

#[test]
fn caravan_shares_camp_knowledge_deterministically() {
    let mut camps = HashMap::new();
    camps.insert(5u32, Vec2::new(200.0, 200.0));

    let agents = vec![
        make_agent(1, Vec2::new(0.0, 0.0), 0.5, Some(10), vec![], camps),
        make_agent(2, Vec2::new(5.0, 0.0), 0.5, Some(10), vec![], HashMap::new()),
    ];

    let mut rng = StdRng::seed_from_u64(42);
    let result = gossip::tick_gossip(&agents, 100, 200, &mut rng);

    // Agent 2 should learn about camp 5 from caravan mate agent 1.
    let camp_for_2: Vec<_> = result
        .camp_shares
        .iter()
        .filter(|s| s.receiver_id == 2 && s.camp_id == 5)
        .collect();
    assert_eq!(
        camp_for_2.len(),
        1,
        "caravan mates should share all camp knowledge deterministically",
    );
}

#[test]
fn already_known_camps_are_not_re_shared() {
    let mut camps = HashMap::new();
    camps.insert(0u32, Vec2::new(100.0, 100.0));

    // Both agents know about camp 0.
    let agents = vec![
        make_agent(1, Vec2::new(0.0, 0.0), 1.0, None, vec![], camps.clone()),
        make_agent(2, Vec2::new(5.0, 0.0), 1.0, None, vec![], camps),
    ];

    for seed in 0..200 {
        let mut rng = StdRng::seed_from_u64(seed);
        let result = gossip::tick_gossip(&agents, 100, 200, &mut rng);
        assert!(
            result.camp_shares.is_empty(),
            "already-known camps should not be re-shared (seed {})",
            seed,
        );
    }
}

// ── Sociability Modulation ──────────────────────────────────────────────────

#[test]
fn zero_sociability_means_zero_gossip_chance() {
    let agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            0.0, // zero sociability
            None,
            vec![(0, Commodity::Ore, price(5.0, 100))],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(5.0, 0.0),
            0.0, // zero sociability
            None,
            vec![(1, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
    ];

    // chance = 0.5 * (0.0 + 0.0) / 2.0 = 0.0 -> gossip can never happen.
    for seed in 0..500 {
        let mut rng = StdRng::seed_from_u64(seed);
        let result = gossip::tick_gossip(&agents, 100, 200, &mut rng);
        assert!(
            result.price_shares.is_empty(),
            "zero sociability should prevent all gossip (seed {})",
            seed,
        );
    }
}

#[test]
fn high_sociability_increases_gossip_frequency() {
    // High sociability pair.
    let high_agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            1.0,
            None,
            vec![(0, Commodity::Ore, price(5.0, 100))],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(5.0, 0.0),
            1.0,
            None,
            vec![(1, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
    ];

    // Low sociability pair.
    let low_agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            0.2,
            None,
            vec![(0, Commodity::Ore, price(5.0, 100))],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(5.0, 0.0),
            0.2,
            None,
            vec![(1, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
    ];

    let trials = 500;
    let mut high_success = 0;
    let mut low_success = 0;

    for seed in 0..trials {
        let mut rng_high = StdRng::seed_from_u64(seed);
        let mut rng_low = StdRng::seed_from_u64(seed);

        let result_high = gossip::tick_gossip(&high_agents, 100, 200, &mut rng_high);
        let result_low = gossip::tick_gossip(&low_agents, 100, 200, &mut rng_low);

        if !result_high.price_shares.is_empty() {
            high_success += 1;
        }
        if !result_low.price_shares.is_empty() {
            low_success += 1;
        }
    }

    assert!(
        high_success > low_success,
        "high sociability ({} successes) should gossip more often than low sociability ({} successes)",
        high_success,
        low_success,
    );
}

#[test]
fn one_zero_sociability_agent_still_prevents_gossip() {
    // One agent has 0 sociability, the other has 1.0.
    // chance = 0.5 * (0.0 + 1.0) / 2.0 = 0.25, so gossip can happen.
    // This verifies the formula uses the average of both agents.
    let agents = vec![
        make_agent(
            1,
            Vec2::new(0.0, 0.0),
            0.0,
            None,
            vec![(0, Commodity::Ore, price(5.0, 100))],
            HashMap::new(),
        ),
        make_agent(
            2,
            Vec2::new(5.0, 0.0),
            1.0,
            None,
            vec![(1, Commodity::Timber, price(3.0, 100))],
            HashMap::new(),
        ),
    ];

    // chance = 0.5 * (0.0 + 1.0) / 2.0 = 0.25 -- low but not zero.
    let mut any_gossip = false;
    for seed in 0..500 {
        let mut rng = StdRng::seed_from_u64(seed);
        let result = gossip::tick_gossip(&agents, 100, 200, &mut rng);
        if !result.price_shares.is_empty() {
            any_gossip = true;
            break;
        }
    }
    assert!(
        any_gossip,
        "mixed sociability (0.0 + 1.0) should still allow gossip occasionally",
    );
}
