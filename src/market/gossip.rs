use rand::Rng;
use std::collections::HashMap;

use crate::agents::merchant::PriceEntry;
use crate::types::{CityId, Commodity, Vec2};
use crate::world::bandit::BanditCampId;

// ── Constants ────────────────────────────────────────────────────────────────

/// Merchants within this range may exchange gossip each tick.
const GOSSIP_RANGE: f32 = 25.0;

/// Base probability of gossip between two merchants, before sociability scaling.
const BASE_GOSSIP_CHANCE: f32 = 0.5;

// ── Input ────────────────────────────────────────────────────────────────────

/// Lightweight view of a merchant for gossip processing.
pub struct GossipAgent {
    pub id: u32,
    pub pos: Vec2,
    pub sociability: f32,
    pub caravan_id: Option<u32>,
    /// All price entries from this merchant's memory (including possibly stale).
    pub prices: Vec<(CityId, Commodity, PriceEntry)>,
    /// Bandit camp locations this merchant is aware of.
    pub known_camps: HashMap<BanditCampId, Vec2>,
}

// ── Output ───────────────────────────────────────────────────────────────────

/// A price entry to be recorded in a merchant's price memory via gossip.
pub struct PriceShare {
    pub receiver_id: u32,
    pub city_id: CityId,
    pub commodity: Commodity,
    pub entry: PriceEntry,
}

/// A bandit camp location learned through gossip.
pub struct CampShare {
    pub receiver_id: u32,
    pub camp_id: BanditCampId,
    pub position: Vec2,
}

/// Aggregated gossip results for one tick.
pub struct GossipResult {
    /// Individual price entries to record in merchants' memories.
    pub price_shares: Vec<PriceShare>,
    /// Bandit camp locations to add to merchants' knowledge.
    pub camp_shares: Vec<CampShare>,
    /// Groups of merchant IDs (within the same caravan) whose price memories
    /// should be fully merged.
    pub caravan_merges: Vec<Vec<u32>>,
}

// ── Core ─────────────────────────────────────────────────────────────────────

/// Run one tick of gossip. Merchants within [`GOSSIP_RANGE`] exchange one
/// random non-stale price entry each, plus all known bandit camp locations.
/// Caravan members fully merge price memories instead.
pub fn tick_gossip(
    agents: &[GossipAgent],
    current_tick: u32,
    ttl: u32,
    rng: &mut impl Rng,
) -> GossipResult {
    let mut result = GossipResult {
        price_shares: Vec::new(),
        camp_shares: Vec::new(),
        caravan_merges: Vec::new(),
    };

    // Collect caravan groups for full merge.
    let mut caravan_groups: HashMap<u32, Vec<u32>> = HashMap::new();
    for agent in agents {
        if let Some(cid) = agent.caravan_id {
            caravan_groups.entry(cid).or_default().push(agent.id);
        }
    }

    for (_, members) in &caravan_groups {
        if members.len() >= 2 {
            result.caravan_merges.push(members.clone());

            // Share all camp knowledge within caravan.
            let caravan_agents: Vec<&GossipAgent> =
                agents.iter().filter(|a| members.contains(&a.id)).collect();
            for i in 0..caravan_agents.len() {
                for j in (i + 1)..caravan_agents.len() {
                    share_all_camps(caravan_agents[i], caravan_agents[j], &mut result.camp_shares);
                    share_all_camps(caravan_agents[j], caravan_agents[i], &mut result.camp_shares);
                }
            }
        }
    }

    // Pairwise gossip for non-caravan-mates.
    for i in 0..agents.len() {
        for j in (i + 1)..agents.len() {
            let a = &agents[i];
            let b = &agents[j];

            // Skip caravan mates (handled above via full merge).
            if let (Some(ca), Some(cb)) = (a.caravan_id, b.caravan_id) {
                if ca == cb {
                    continue;
                }
            }

            if a.pos.distance(b.pos) > GOSSIP_RANGE {
                continue;
            }

            // Sociability-scaled probability gate.
            let chance = BASE_GOSSIP_CHANCE * (a.sociability + b.sociability) / 2.0;
            if rng.gen::<f32>() >= chance {
                continue;
            }

            // Exchange one random non-stale price entry each way.
            share_random_price(a, b.id, current_tick, ttl, rng, &mut result.price_shares);
            share_random_price(b, a.id, current_tick, ttl, rng, &mut result.price_shares);

            // Share all known camp locations.
            share_all_camps(a, b, &mut result.camp_shares);
            share_all_camps(b, a, &mut result.camp_shares);
        }
    }

    result
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Pick one random non-stale price entry from `sender` and push it for
/// `receiver_id`.
fn share_random_price(
    sender: &GossipAgent,
    receiver_id: u32,
    current_tick: u32,
    ttl: u32,
    rng: &mut impl Rng,
    shares: &mut Vec<PriceShare>,
) {
    let valid: Vec<_> = sender
        .prices
        .iter()
        .filter(|(_, _, entry)| current_tick.saturating_sub(entry.observed_tick) <= ttl)
        .collect();

    if valid.is_empty() {
        return;
    }

    let idx = rng.gen_range(0..valid.len());
    let &(city_id, commodity, entry) = valid[idx];
    shares.push(PriceShare {
        receiver_id,
        city_id,
        commodity,
        entry,
    });
}

/// Share all of `sender`'s known camp locations with `receiver`, skipping
/// camps the receiver already knows about.
fn share_all_camps(
    sender: &GossipAgent,
    receiver: &GossipAgent,
    shares: &mut Vec<CampShare>,
) {
    for (&camp_id, &position) in &sender.known_camps {
        if !receiver.known_camps.contains_key(&camp_id) {
            shares.push(CampShare {
                receiver_id: receiver.id,
                camp_id,
                position,
            });
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

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

    #[test]
    fn no_gossip_when_out_of_range() {
        let mut rng = StdRng::seed_from_u64(42);
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
                Vec2::new(50.0, 0.0),
                1.0,
                None,
                vec![(1, Commodity::Timber, price(3.0, 100))],
                HashMap::new(),
            ),
        ];
        let result = tick_gossip(&agents, 100, 200, &mut rng);
        assert!(result.price_shares.is_empty());
    }

    #[test]
    fn gossip_exchanges_one_price_entry_each_way() {
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
                Vec2::new(10.0, 0.0),
                1.0,
                None,
                vec![(1, Commodity::Timber, price(3.0, 100))],
                HashMap::new(),
            ),
        ];

        // With max sociability, chance = 0.5 × (1.0 + 1.0) / 2 = 0.5
        // Run multiple seeds to get a successful gossip.
        let mut got_shares = false;
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = tick_gossip(&agents, 100, 200, &mut rng);
            if !result.price_shares.is_empty() {
                assert_eq!(result.price_shares.len(), 2);
                got_shares = true;
                break;
            }
        }
        assert!(got_shares, "gossip should eventually succeed with max sociability");
    }

    #[test]
    fn stale_entries_not_shared() {
        let agents = vec![
            make_agent(
                1,
                Vec2::new(0.0, 0.0),
                1.0,
                None,
                vec![(0, Commodity::Ore, price(5.0, 10))], // observed at tick 10
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

        // TTL = 50, current_tick = 100 → entry at tick 10 is stale (age 90 > 50).
        let mut any_stale_shared = false;
        for seed in 0..500 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = tick_gossip(&agents, 100, 50, &mut rng);
            for share in &result.price_shares {
                if share.receiver_id == 2 && share.commodity == Commodity::Ore {
                    any_stale_shared = true;
                }
            }
        }
        assert!(!any_stale_shared, "stale entries should never be shared");
    }

    #[test]
    fn camp_locations_shared_on_gossip() {
        let mut camps = HashMap::new();
        camps.insert(0u32, Vec2::new(100.0, 100.0));

        let agents = vec![
            make_agent(1, Vec2::new(0.0, 0.0), 1.0, None, vec![], camps),
            make_agent(2, Vec2::new(5.0, 0.0), 1.0, None, vec![], HashMap::new()),
        ];

        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = tick_gossip(&agents, 100, 200, &mut rng);
            if !result.camp_shares.is_empty() {
                let share = &result.camp_shares[0];
                assert_eq!(share.receiver_id, 2);
                assert_eq!(share.camp_id, 0);
                return;
            }
        }
        panic!("camp location should eventually be shared");
    }

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
        let result = tick_gossip(&agents, 100, 200, &mut rng);

        assert_eq!(result.caravan_merges.len(), 1);
        assert_eq!(result.caravan_merges[0].len(), 3);

        // No individual price_shares for caravan mates.
        let caravan_shares: Vec<_> = result
            .price_shares
            .iter()
            .filter(|s| [1, 2, 3].contains(&s.receiver_id))
            .collect();
        assert!(caravan_shares.is_empty());
    }

    #[test]
    fn caravan_shares_camp_knowledge() {
        let mut camps = HashMap::new();
        camps.insert(5u32, Vec2::new(200.0, 200.0));

        let agents = vec![
            make_agent(1, Vec2::new(0.0, 0.0), 0.5, Some(10), vec![], camps),
            make_agent(
                2,
                Vec2::new(5.0, 0.0),
                0.5,
                Some(10),
                vec![],
                HashMap::new(),
            ),
        ];

        let mut rng = StdRng::seed_from_u64(42);
        let result = tick_gossip(&agents, 100, 200, &mut rng);

        // Agent 2 should learn about camp 5 from agent 1.
        let camp_for_2: Vec<_> = result
            .camp_shares
            .iter()
            .filter(|s| s.receiver_id == 2 && s.camp_id == 5)
            .collect();
        assert_eq!(camp_for_2.len(), 1);
    }

    #[test]
    fn zero_sociability_prevents_gossip() {
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
                0.0,
                None,
                vec![(1, Commodity::Timber, price(3.0, 100))],
                HashMap::new(),
            ),
        ];

        // chance = 0.5 × (0 + 0) / 2 = 0 → gossip never happens.
        let mut any_gossip = false;
        for seed in 0..500 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = tick_gossip(&agents, 100, 200, &mut rng);
            if !result.price_shares.is_empty() {
                any_gossip = true;
                break;
            }
        }
        assert!(!any_gossip, "zero sociability should prevent all gossip");
    }

    #[test]
    fn known_camps_not_re_shared() {
        let mut camps = HashMap::new();
        camps.insert(0u32, Vec2::new(100.0, 100.0));

        // Both agents already know about camp 0.
        let agents = vec![
            make_agent(1, Vec2::new(0.0, 0.0), 1.0, None, vec![], camps.clone()),
            make_agent(2, Vec2::new(5.0, 0.0), 1.0, None, vec![], camps),
        ];

        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = tick_gossip(&agents, 100, 200, &mut rng);
            assert!(
                result.camp_shares.is_empty(),
                "should not re-share already known camps"
            );
        }
    }
}
