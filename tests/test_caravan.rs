mod common;

use swarm_economy::agents::caravan::*;
use swarm_economy::types::Vec2;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn candidate(
    id: u32,
    pos: Vec2,
    heading: f32,
    speed: f32,
    sociability: f32,
    caravan_id: Option<u32>,
) -> CaravanCandidate {
    CaravanCandidate {
        id,
        pos,
        heading,
        speed,
        sociability,
        caravan_id,
    }
}

/// Build a minimal set of 3+ candidates that satisfy all formation requirements:
/// - initiator sociability > 0.5
/// - 2+ others within 30px
/// - heading diff < pi/4
fn formable_candidates() -> Vec<CaravanCandidate> {
    vec![
        candidate(1, Vec2::new(10.0, 10.0), 1.0, 2.0, 0.8, None),
        candidate(2, Vec2::new(15.0, 10.0), 1.1, 3.0, 0.3, None),
        candidate(3, Vec2::new(12.0, 15.0), 0.9, 1.5, 0.4, None),
    ]
}

// ── Formation requirements ──────────────────────────────────────────────────

#[test]
fn caravan_forms_with_valid_candidates() {
    let mut system = CaravanSystem::new();
    let events = system.try_form_caravan(&formable_candidates());
    assert_eq!(events.len(), 1, "exactly one caravan should form");
    assert_eq!(events[0].member_ids.len(), 3, "all three should join");
    assert_eq!(events[0].leader_id, 1, "highest sociability leads");
}

#[test]
fn formation_requires_initiator_sociability_above_half() {
    let mut system = CaravanSystem::new();
    // Initiator at exactly 0.5 should NOT initiate (needs > 0.5).
    let candidates = vec![
        candidate(1, Vec2::new(10.0, 10.0), 1.0, 2.0, 0.5, None),
        candidate(2, Vec2::new(15.0, 10.0), 1.1, 3.0, 0.5, None),
        candidate(3, Vec2::new(12.0, 15.0), 0.9, 1.5, 0.5, None),
    ];
    let events = system.try_form_caravan(&candidates);
    assert!(events.is_empty(), "sociability=0.5 is not > 0.5");
}

#[test]
fn formation_requires_two_others_minimum() {
    let mut system = CaravanSystem::new();
    let candidates = vec![
        candidate(1, Vec2::new(10.0, 10.0), 1.0, 2.0, 0.8, None),
        candidate(2, Vec2::new(15.0, 10.0), 1.1, 3.0, 0.3, None),
    ];
    let events = system.try_form_caravan(&candidates);
    assert!(events.is_empty(), "only 1 other, need 2+");
}

#[test]
fn formation_requires_proximity_within_30px() {
    let mut system = CaravanSystem::new();
    let candidates = vec![
        candidate(1, Vec2::new(0.0, 0.0), 1.0, 2.0, 0.8, None),
        candidate(2, Vec2::new(50.0, 0.0), 1.0, 2.0, 0.3, None),
        candidate(3, Vec2::new(60.0, 0.0), 1.0, 2.0, 0.4, None),
    ];
    let events = system.try_form_caravan(&candidates);
    assert!(events.is_empty(), "others are > 30px away from initiator");
}

#[test]
fn formation_requires_similar_heading_within_pi_over_4() {
    let mut system = CaravanSystem::new();
    let candidates = vec![
        candidate(1, Vec2::new(10.0, 10.0), 0.0, 2.0, 0.8, None),
        candidate(2, Vec2::new(15.0, 10.0), 2.0, 2.0, 0.3, None),  // diff ~2.0 >> pi/4
        candidate(3, Vec2::new(12.0, 15.0), 3.0, 2.0, 0.4, None),  // diff ~3.0 >> pi/4
    ];
    let events = system.try_form_caravan(&candidates);
    assert!(events.is_empty(), "heading differences too large");
}

// ── Sociability threshold ───────────────────────────────────────────────────

#[test]
fn no_caravan_when_all_sociability_at_or_below_half() {
    let mut system = CaravanSystem::new();
    let candidates = vec![
        candidate(1, Vec2::new(10.0, 10.0), 1.0, 2.0, 0.3, None),
        candidate(2, Vec2::new(15.0, 10.0), 1.1, 3.0, 0.4, None),
        candidate(3, Vec2::new(12.0, 15.0), 0.9, 1.5, 0.5, None),
        candidate(4, Vec2::new(13.0, 12.0), 1.0, 2.0, 0.2, None),
    ];
    let events = system.try_form_caravan(&candidates);
    assert!(events.is_empty(), "no one has sociability > 0.5");
}

#[test]
fn highest_sociability_becomes_leader() {
    let mut system = CaravanSystem::new();
    let candidates = vec![
        candidate(1, Vec2::new(10.0, 10.0), 1.0, 2.0, 0.6, None),
        candidate(2, Vec2::new(15.0, 10.0), 1.05, 2.0, 0.9, None),
        candidate(3, Vec2::new(12.0, 15.0), 0.95, 2.0, 0.3, None),
        candidate(4, Vec2::new(13.0, 12.0), 1.0, 2.0, 0.4, None),
    ];
    let events = system.try_form_caravan(&candidates);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].leader_id, 2, "merchant 2 has highest sociability (0.9)");
}

// ── Movement speed ──────────────────────────────────────────────────────────

#[test]
fn caravan_moves_at_slowest_member_speed() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    // speeds: 2.0, 3.0, 1.5 => slowest = 1.5
    system.try_form_caravan(&candidates);

    let caravan = &system.caravans()[0];
    let cid = caravan.id;
    let members = caravan.member_ids.clone();

    let updated: Vec<_> = candidates
        .iter()
        .map(|c| {
            let cid_opt = if members.contains(&c.id) {
                Some(cid)
            } else {
                None
            };
            candidate(c.id, c.pos, c.heading, c.speed, c.sociability, cid_opt)
        })
        .collect();

    let directives = system.tick_caravan_movement(&updated);
    assert_eq!(directives.len(), 3);
    for d in &directives {
        assert!(
            (d.max_speed - 1.5).abs() < 1e-5,
            "should use slowest speed 1.5, got {}",
            d.max_speed
        );
    }
}

#[test]
fn caravan_follows_leader_heading() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    system.try_form_caravan(&candidates);

    let caravan = &system.caravans()[0];
    let leader_heading = candidates
        .iter()
        .find(|c| c.id == caravan.leader_id)
        .unwrap()
        .heading;

    let cid = caravan.id;
    let members = caravan.member_ids.clone();
    let updated: Vec<_> = candidates
        .iter()
        .map(|c| {
            let cid_opt = if members.contains(&c.id) {
                Some(cid)
            } else {
                None
            };
            candidate(c.id, c.pos, c.heading, c.speed, c.sociability, cid_opt)
        })
        .collect();

    let directives = system.tick_caravan_movement(&updated);
    for d in &directives {
        assert!(
            (d.target_heading - leader_heading).abs() < 1e-5,
            "all members should follow leader heading"
        );
    }
}

// ── Dissolution conditions ──────────────────────────────────────────────────

#[test]
fn dissolution_when_spread_exceeds_100px() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    system.try_form_caravan(&candidates);

    // Move members far apart.
    let spread_candidates = vec![
        candidate(1, Vec2::new(0.0, 0.0), 1.0, 2.0, 0.8, Some(0)),
        candidate(2, Vec2::new(150.0, 0.0), 1.0, 2.0, 0.3, Some(0)),
        candidate(3, Vec2::new(50.0, 50.0), 1.0, 2.0, 0.4, Some(0)),
    ];

    let events = system.check_dissolution(&spread_candidates, &[]);
    assert_eq!(events.len(), 1, "should dissolve when spread > 100px");
    assert!(system.caravans().is_empty());
}

#[test]
fn dissolution_when_reaching_a_city() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    system.try_form_caravan(&candidates);

    // All members close together but one is in a city.
    let near_city = vec![
        candidate(1, Vec2::new(100.0, 100.0), 1.0, 2.0, 0.8, Some(0)),
        candidate(2, Vec2::new(102.0, 100.0), 1.0, 2.0, 0.3, Some(0)),
        candidate(3, Vec2::new(101.0, 102.0), 1.0, 2.0, 0.4, Some(0)),
    ];
    let cities = vec![(Vec2::new(100.0, 100.0), 15.0)];

    let events = system.check_dissolution(&near_city, &cities);
    assert_eq!(events.len(), 1, "should dissolve when at a city");
}

#[test]
fn no_dissolution_when_cohesive_and_away_from_cities() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    system.try_form_caravan(&candidates);

    // All members close (spread < 100), away from any city.
    let cohesive = vec![
        candidate(1, Vec2::new(200.0, 200.0), 1.0, 2.0, 0.8, Some(0)),
        candidate(2, Vec2::new(205.0, 200.0), 1.0, 2.0, 0.3, Some(0)),
        candidate(3, Vec2::new(202.0, 203.0), 1.0, 2.0, 0.4, Some(0)),
    ];

    let events = system.check_dissolution(&cohesive, &[]);
    assert!(events.is_empty(), "should not dissolve");
    assert_eq!(system.caravans().len(), 1);
}

#[test]
fn dissolution_event_contains_all_member_ids() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    system.try_form_caravan(&candidates);

    let spread = vec![
        candidate(1, Vec2::new(0.0, 0.0), 1.0, 2.0, 0.8, Some(0)),
        candidate(2, Vec2::new(200.0, 0.0), 1.0, 2.0, 0.3, Some(0)),
        candidate(3, Vec2::new(100.0, 100.0), 1.0, 2.0, 0.4, Some(0)),
    ];

    let events = system.check_dissolution(&spread, &[]);
    assert_eq!(events.len(), 1);
    let mut ids = events[0].member_ids.clone();
    ids.sort();
    assert_eq!(ids, vec![1, 2, 3]);
}

// ── Safety sizes ────────────────────────────────────────────────────────────

#[test]
fn caravan_group_size_reflects_members() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    system.try_form_caravan(&candidates);

    let caravan = &system.caravans()[0];
    assert_eq!(
        caravan.member_ids.len(),
        3,
        "caravan size = number of members"
    );
}

#[test]
fn larger_caravan_provides_more_safety() {
    let mut system = CaravanSystem::new();
    // 5 merchants close together, same heading.
    let candidates = vec![
        candidate(1, Vec2::new(10.0, 10.0), 1.0, 2.0, 0.9, None),
        candidate(2, Vec2::new(12.0, 10.0), 1.0, 2.0, 0.3, None),
        candidate(3, Vec2::new(14.0, 10.0), 1.0, 2.0, 0.3, None),
        candidate(4, Vec2::new(11.0, 12.0), 1.0, 2.0, 0.3, None),
        candidate(5, Vec2::new(13.0, 12.0), 1.0, 2.0, 0.3, None),
    ];
    let events = system.try_form_caravan(&candidates);
    assert_eq!(events.len(), 1);
    assert!(
        events[0].member_ids.len() >= 4,
        "caravan_safe_size (4) can be reached with 5 candidates"
    );
}

// ── Soldier escort ──────────────────────────────────────────────────────────

#[test]
fn soldier_within_30px_auto_attaches_as_escort() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    system.try_form_caravan(&candidates);

    let cid = system.caravans()[0].id;
    let members = system.caravans()[0].member_ids.clone();
    let updated: Vec<_> = candidates
        .iter()
        .map(|c| {
            let cid_opt = if members.contains(&c.id) {
                Some(cid)
            } else {
                None
            };
            candidate(c.id, c.pos, c.heading, c.speed, c.sociability, cid_opt)
        })
        .collect();

    let soldiers = vec![SoldierView {
        id: 100,
        pos: Vec2::new(11.0, 11.0), // within 30px of members
    }];

    let fees = system.add_soldier_escort(&soldiers, &updated);

    // 3 members * 1 soldier = 3 fee entries.
    assert_eq!(fees.len(), 3, "each member pays each soldier");
    assert!(system.caravans()[0].escort_ids.contains(&100));
}

#[test]
fn soldier_beyond_30px_not_attached() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    system.try_form_caravan(&candidates);

    let cid = system.caravans()[0].id;
    let members = system.caravans()[0].member_ids.clone();
    let updated: Vec<_> = candidates
        .iter()
        .map(|c| {
            let cid_opt = if members.contains(&c.id) {
                Some(cid)
            } else {
                None
            };
            candidate(c.id, c.pos, c.heading, c.speed, c.sociability, cid_opt)
        })
        .collect();

    let soldiers = vec![SoldierView {
        id: 100,
        pos: Vec2::new(500.0, 500.0), // far away
    }];

    let fees = system.add_soldier_escort(&soldiers, &updated);
    assert!(fees.is_empty());
    assert!(system.caravans()[0].escort_ids.is_empty());
}

#[test]
fn escort_fee_is_0_01_per_tick_per_member() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    system.try_form_caravan(&candidates);

    let cid = system.caravans()[0].id;
    let members = system.caravans()[0].member_ids.clone();
    let updated: Vec<_> = candidates
        .iter()
        .map(|c| {
            let cid_opt = if members.contains(&c.id) {
                Some(cid)
            } else {
                None
            };
            candidate(c.id, c.pos, c.heading, c.speed, c.sociability, cid_opt)
        })
        .collect();

    let soldiers = vec![SoldierView {
        id: 100,
        pos: Vec2::new(11.0, 11.0),
    }];

    let fees = system.add_soldier_escort(&soldiers, &updated);
    for fee in &fees {
        assert!(
            (fee.amount - 0.01).abs() < 1e-6,
            "each fee should be 0.01"
        );
        assert_eq!(fee.soldier_id, 100);
    }
}

#[test]
fn multiple_soldiers_multiply_fees() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    system.try_form_caravan(&candidates);

    let cid = system.caravans()[0].id;
    let members = system.caravans()[0].member_ids.clone();
    let updated: Vec<_> = candidates
        .iter()
        .map(|c| {
            let cid_opt = if members.contains(&c.id) {
                Some(cid)
            } else {
                None
            };
            candidate(c.id, c.pos, c.heading, c.speed, c.sociability, cid_opt)
        })
        .collect();

    let soldiers = vec![
        SoldierView {
            id: 100,
            pos: Vec2::new(11.0, 11.0),
        },
        SoldierView {
            id: 101,
            pos: Vec2::new(14.0, 14.0),
        },
    ];

    let fees = system.add_soldier_escort(&soldiers, &updated);
    // 3 members * 2 soldiers = 6 fee entries.
    assert_eq!(fees.len(), 6);
    assert_eq!(system.caravans()[0].escort_ids.len(), 2);
}

// ── Price merge (caravan members share memory) ──────────────────────────────

#[test]
fn caravan_members_listed_for_price_merge() {
    let mut system = CaravanSystem::new();
    let candidates = formable_candidates();
    let events = system.try_form_caravan(&candidates);

    assert_eq!(events.len(), 1);
    // All members in the same caravan can share price memory.
    let member_ids = &events[0].member_ids;
    assert!(member_ids.contains(&1));
    assert!(member_ids.contains(&2));
    assert!(member_ids.contains(&3));

    // The caravan_for_merchant accessor finds the caravan for any member.
    for &mid in member_ids {
        let caravan = system.caravan_for_merchant(mid);
        assert!(caravan.is_some(), "member {} should be findable", mid);
        assert_eq!(caravan.unwrap().id, events[0].caravan_id);
    }
}

// ── Edge cases ──────────────────────────────────────────────────────────────

#[test]
fn already_assigned_merchants_cannot_join_new_caravan() {
    let mut system = CaravanSystem::new();
    // Form first caravan.
    let candidates = formable_candidates();
    system.try_form_caravan(&candidates);
    assert_eq!(system.active_count(), 1);

    // Try to form another with the same merchants (now assigned).
    let assigned: Vec<_> = candidates
        .iter()
        .map(|c| candidate(c.id, c.pos, c.heading, c.speed, c.sociability, Some(0)))
        .collect();
    let events = system.try_form_caravan(&assigned);
    assert!(events.is_empty(), "already-assigned merchants skip formation");
}

#[test]
fn empty_candidates_produces_no_caravan() {
    let mut system = CaravanSystem::new();
    let events = system.try_form_caravan(&[]);
    assert!(events.is_empty());
}
