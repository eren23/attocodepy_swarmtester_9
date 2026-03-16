use std::collections::HashSet;

use crate::types::Vec2;

// ── Constants ────────────────────────────────────────────────────────────────

/// Range within which merchants can form or join a caravan.
const FORMATION_RANGE: f32 = 30.0;

/// Maximum heading difference (radians) for caravan formation.
const HEADING_TOLERANCE: f32 = std::f32::consts::FRAC_PI_4; // π/4

/// Caravan dissolves if any two members are this far apart.
const DISSOLUTION_SPREAD: f32 = 100.0;

/// Range within which soldiers auto-attach as escorts.
const ESCORT_RANGE: f32 = 30.0;

/// Per-tick fee each caravan member pays to each escort soldier.
const ESCORT_FEE_PER_TICK: f32 = 0.01;

/// Spacing between formation positions (perpendicular to heading).
const FORMATION_OFFSET_SPACING: f32 = 5.0;

/// Minimum sociability for a merchant to initiate a caravan.
const INITIATOR_SOCIABILITY_MIN: f32 = 0.5;

// ── Types ────────────────────────────────────────────────────────────────────

/// A group of merchants traveling together.
#[derive(Debug, Clone)]
pub struct Caravan {
    pub id: u32,
    pub member_ids: Vec<u32>,
    pub leader_id: u32,
    pub escort_ids: Vec<u32>,
}

/// Lightweight merchant view for caravan operations.
pub struct CaravanCandidate {
    pub id: u32,
    pub pos: Vec2,
    pub heading: f32,
    pub speed: f32,
    pub sociability: f32,
    pub caravan_id: Option<u32>,
}

/// Lightweight soldier view for escort processing.
pub struct SoldierView {
    pub id: u32,
    pub pos: Vec2,
}

/// Movement instructions for a caravan member.
pub struct MovementDirective {
    pub merchant_id: u32,
    /// Heading to follow (leader's heading).
    pub target_heading: f32,
    /// Speed limit (slowest member's speed).
    pub max_speed: f32,
    /// Positional offset for visual formation spread.
    pub formation_offset: Vec2,
}

/// Event: a new caravan was formed.
pub struct FormationEvent {
    pub caravan_id: u32,
    pub member_ids: Vec<u32>,
    pub leader_id: u32,
}

/// Event: a caravan dissolved.
pub struct DissolutionEvent {
    pub caravan_id: u32,
    pub member_ids: Vec<u32>,
}

/// Fee payment from a caravan member to an escort soldier.
pub struct EscortFee {
    pub merchant_id: u32,
    pub soldier_id: u32,
    pub amount: f32,
}

// ── CaravanSystem ────────────────────────────────────────────────────────────

pub struct CaravanSystem {
    caravans: Vec<Caravan>,
    next_id: u32,
}

impl CaravanSystem {
    pub fn new() -> Self {
        Self {
            caravans: Vec::new(),
            next_id: 0,
        }
    }

    /// Attempt to form new caravans from unattached merchants.
    ///
    /// A merchant with sociability > 0.5 near 2+ others heading similarly
    /// (heading diff < π/4, within 30px) initiates a caravan as leader.
    pub fn try_form_caravan(
        &mut self,
        candidates: &[CaravanCandidate],
    ) -> Vec<FormationEvent> {
        let mut events = Vec::new();
        let mut assigned: HashSet<u32> = HashSet::new();

        // Mark merchants already in caravans.
        for c in candidates {
            if c.caravan_id.is_some() {
                assigned.insert(c.id);
            }
        }

        // Sort potential initiators by sociability (highest first).
        let mut initiators: Vec<usize> = (0..candidates.len())
            .filter(|&i| {
                candidates[i].sociability > INITIATOR_SOCIABILITY_MIN
                    && candidates[i].caravan_id.is_none()
            })
            .collect();
        initiators.sort_by(|&a, &b| {
            candidates[b]
                .sociability
                .partial_cmp(&candidates[a].sociability)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for &init_idx in &initiators {
            let initiator = &candidates[init_idx];
            if assigned.contains(&initiator.id) {
                continue;
            }

            // Find nearby unassigned merchants heading similarly.
            let mut recruits: Vec<u32> = Vec::new();
            for other in candidates {
                if other.id == initiator.id || assigned.contains(&other.id) {
                    continue;
                }
                if initiator.pos.distance(other.pos) > FORMATION_RANGE {
                    continue;
                }
                if heading_diff(initiator.heading, other.heading) > HEADING_TOLERANCE {
                    continue;
                }
                recruits.push(other.id);
            }

            // Need at least 2 others to form a caravan.
            if recruits.len() < 2 {
                continue;
            }

            let caravan_id = self.next_id;
            self.next_id += 1;

            let mut member_ids = vec![initiator.id];
            member_ids.extend(&recruits);

            for &mid in &member_ids {
                assigned.insert(mid);
            }

            self.caravans.push(Caravan {
                id: caravan_id,
                member_ids: member_ids.clone(),
                leader_id: initiator.id,
                escort_ids: Vec::new(),
            });

            events.push(FormationEvent {
                caravan_id,
                member_ids,
                leader_id: initiator.id,
            });
        }

        events
    }

    /// Compute movement directives for all caravan members.
    ///
    /// All members move at the speed of the slowest, following the leader's
    /// heading, with slight perpendicular offsets for visual spread.
    pub fn tick_caravan_movement(
        &self,
        candidates: &[CaravanCandidate],
    ) -> Vec<MovementDirective> {
        let mut directives = Vec::new();

        for caravan in &self.caravans {
            let leader = candidates.iter().find(|c| c.id == caravan.leader_id);
            let leader_heading = match leader {
                Some(l) => l.heading,
                None => continue,
            };

            // Slowest member speed.
            let min_speed = caravan
                .member_ids
                .iter()
                .filter_map(|&mid| candidates.iter().find(|c| c.id == mid))
                .map(|c| c.speed)
                .fold(f32::INFINITY, f32::min);

            if min_speed == f32::INFINITY {
                continue;
            }

            // Perpendicular direction for formation offsets.
            let perp = Vec2::from_angle(leader_heading + std::f32::consts::FRAC_PI_2);
            let num_members = caravan.member_ids.len();

            for (i, &mid) in caravan.member_ids.iter().enumerate() {
                let offset = if mid == caravan.leader_id {
                    Vec2::ZERO
                } else {
                    let spread = i as f32 - (num_members as f32 - 1.0) / 2.0;
                    perp * spread * FORMATION_OFFSET_SPACING
                };

                directives.push(MovementDirective {
                    merchant_id: mid,
                    target_heading: leader_heading,
                    max_speed: min_speed,
                    formation_offset: offset,
                });
            }
        }

        directives
    }

    /// Check dissolution conditions and remove qualifying caravans.
    ///
    /// Dissolves when members spread > 100px apart or any member reaches
    /// a city.
    pub fn check_dissolution(
        &mut self,
        candidates: &[CaravanCandidate],
        city_positions: &[(Vec2, f32)], // (center, radius)
    ) -> Vec<DissolutionEvent> {
        let mut events = Vec::new();
        let mut to_remove = Vec::new();

        for (idx, caravan) in self.caravans.iter().enumerate() {
            let member_positions: Vec<Vec2> = caravan
                .member_ids
                .iter()
                .filter_map(|&mid| candidates.iter().find(|c| c.id == mid))
                .map(|c| c.pos)
                .collect();

            if spread_exceeds(&member_positions, DISSOLUTION_SPREAD)
                || any_at_city(&member_positions, city_positions)
            {
                events.push(DissolutionEvent {
                    caravan_id: caravan.id,
                    member_ids: caravan.member_ids.clone(),
                });
                to_remove.push(idx);
            }
        }

        // Remove dissolved caravans (reverse to preserve indices).
        for &idx in to_remove.iter().rev() {
            self.caravans.swap_remove(idx);
        }

        events
    }

    /// Auto-attach nearby soldiers as escorts and compute protection fees.
    ///
    /// Each caravan member pays [`ESCORT_FEE_PER_TICK`] to each escorting
    /// soldier per tick.
    pub fn add_soldier_escort(
        &mut self,
        soldiers: &[SoldierView],
        candidates: &[CaravanCandidate],
    ) -> Vec<EscortFee> {
        let mut fees = Vec::new();

        for caravan in &mut self.caravans {
            let member_positions: Vec<(u32, Vec2)> = caravan
                .member_ids
                .iter()
                .filter_map(|&mid| {
                    candidates
                        .iter()
                        .find(|c| c.id == mid)
                        .map(|c| (mid, c.pos))
                })
                .collect();

            // Find soldiers within range of any caravan member.
            let mut escorts: Vec<u32> = Vec::new();
            for soldier in soldiers {
                let in_range = member_positions
                    .iter()
                    .any(|&(_, pos)| pos.distance(soldier.pos) <= ESCORT_RANGE);
                if in_range && !escorts.contains(&soldier.id) {
                    escorts.push(soldier.id);
                }
            }

            caravan.escort_ids = escorts.clone();

            // Each member pays each escort.
            for &(mid, _) in &member_positions {
                for &sid in &escorts {
                    fees.push(EscortFee {
                        merchant_id: mid,
                        soldier_id: sid,
                        amount: ESCORT_FEE_PER_TICK,
                    });
                }
            }
        }

        fees
    }

    // ── Accessors ────────────────────────────────────────────────────────

    pub fn caravans(&self) -> &[Caravan] {
        &self.caravans
    }

    pub fn get_caravan(&self, id: u32) -> Option<&Caravan> {
        self.caravans.iter().find(|c| c.id == id)
    }

    pub fn active_count(&self) -> usize {
        self.caravans.len()
    }

    pub fn caravan_for_merchant(&self, merchant_id: u32) -> Option<&Caravan> {
        self.caravans
            .iter()
            .find(|c| c.member_ids.contains(&merchant_id))
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Absolute angular difference between two headings, in [0, π].
fn heading_diff(a: f32, b: f32) -> f32 {
    let diff = (a - b).rem_euclid(std::f32::consts::TAU);
    if diff > std::f32::consts::PI {
        std::f32::consts::TAU - diff
    } else {
        diff
    }
}

/// Returns `true` if any two positions are more than `threshold` apart.
fn spread_exceeds(positions: &[Vec2], threshold: f32) -> bool {
    for i in 0..positions.len() {
        for j in (i + 1)..positions.len() {
            if positions[i].distance(positions[j]) > threshold {
                return true;
            }
        }
    }
    false
}

/// Returns `true` if any position is within a city's radius.
fn any_at_city(positions: &[Vec2], cities: &[(Vec2, f32)]) -> bool {
    for pos in positions {
        for &(city_pos, radius) in cities {
            if pos.distance(city_pos) <= radius {
                return true;
            }
        }
    }
    false
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── Formation tests ──────────────────────────────────────────────────

    #[test]
    fn form_caravan_with_three_nearby_merchants() {
        let mut system = CaravanSystem::new();
        let candidates = vec![
            candidate(1, Vec2::new(0.0, 0.0), 1.0, 2.0, 0.8, None),
            candidate(2, Vec2::new(10.0, 0.0), 1.1, 2.5, 0.3, None),
            candidate(3, Vec2::new(5.0, 5.0), 0.9, 2.0, 0.4, None),
        ];

        let events = system.try_form_caravan(&candidates);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].leader_id, 1);
        assert_eq!(events[0].member_ids.len(), 3);
        assert_eq!(system.caravans().len(), 1);
    }

    #[test]
    fn no_caravan_if_low_sociability() {
        let mut system = CaravanSystem::new();
        let candidates = vec![
            candidate(1, Vec2::new(0.0, 0.0), 1.0, 2.0, 0.3, None),
            candidate(2, Vec2::new(10.0, 0.0), 1.1, 2.5, 0.3, None),
            candidate(3, Vec2::new(5.0, 5.0), 0.9, 2.0, 0.4, None),
        ];
        let events = system.try_form_caravan(&candidates);
        assert!(events.is_empty());
    }

    #[test]
    fn no_caravan_if_headings_too_different() {
        let mut system = CaravanSystem::new();
        let candidates = vec![
            candidate(1, Vec2::new(0.0, 0.0), 0.0, 2.0, 0.8, None),
            candidate(2, Vec2::new(10.0, 0.0), 2.0, 2.5, 0.3, None),
            candidate(3, Vec2::new(5.0, 5.0), 3.0, 2.0, 0.4, None),
        ];
        let events = system.try_form_caravan(&candidates);
        assert!(events.is_empty());
    }

    #[test]
    fn no_caravan_if_too_far_apart() {
        let mut system = CaravanSystem::new();
        let candidates = vec![
            candidate(1, Vec2::new(0.0, 0.0), 1.0, 2.0, 0.8, None),
            candidate(2, Vec2::new(100.0, 0.0), 1.1, 2.5, 0.3, None),
            candidate(3, Vec2::new(50.0, 50.0), 0.9, 2.0, 0.4, None),
        ];
        let events = system.try_form_caravan(&candidates);
        assert!(events.is_empty());
    }

    #[test]
    fn no_caravan_with_only_one_other() {
        let mut system = CaravanSystem::new();
        let candidates = vec![
            candidate(1, Vec2::new(0.0, 0.0), 1.0, 2.0, 0.8, None),
            candidate(2, Vec2::new(10.0, 0.0), 1.1, 2.5, 0.3, None),
        ];
        let events = system.try_form_caravan(&candidates);
        assert!(events.is_empty(), "need 2+ others, not just 1");
    }

    #[test]
    fn already_in_caravan_cannot_join_new() {
        let mut system = CaravanSystem::new();
        system.caravans.push(Caravan {
            id: 0,
            member_ids: vec![2, 3, 4],
            leader_id: 2,
            escort_ids: Vec::new(),
        });

        let candidates = vec![
            candidate(1, Vec2::new(0.0, 0.0), 1.0, 2.0, 0.8, None),
            candidate(2, Vec2::new(5.0, 0.0), 1.1, 2.5, 0.3, Some(0)),
            candidate(3, Vec2::new(5.0, 5.0), 0.9, 2.0, 0.4, Some(0)),
            candidate(5, Vec2::new(10.0, 0.0), 1.0, 2.0, 0.4, None),
        ];

        let events = system.try_form_caravan(&candidates);
        // Merchant 1 wants to initiate, but only merchant 5 is available.
        assert!(events.is_empty());
    }

    #[test]
    fn highest_sociability_initiates_first() {
        let mut system = CaravanSystem::new();
        let candidates = vec![
            candidate(1, Vec2::new(0.0, 0.0), 1.0, 2.0, 0.6, None),
            candidate(2, Vec2::new(5.0, 0.0), 1.1, 2.5, 0.9, None),
            candidate(3, Vec2::new(5.0, 5.0), 0.9, 2.0, 0.3, None),
            candidate(4, Vec2::new(8.0, 3.0), 1.0, 2.0, 0.3, None),
        ];

        let events = system.try_form_caravan(&candidates);
        assert_eq!(events.len(), 1);
        // Merchant 2 has highest sociability, so they lead.
        assert_eq!(events[0].leader_id, 2);
    }

    // ── Movement tests ───────────────────────────────────────────────────

    #[test]
    fn movement_uses_slowest_speed_and_leader_heading() {
        let mut system = CaravanSystem::new();
        let candidates = vec![
            candidate(1, Vec2::new(0.0, 0.0), 1.5, 3.0, 0.8, None),
            candidate(2, Vec2::new(10.0, 0.0), 1.6, 1.0, 0.3, None),
            candidate(3, Vec2::new(5.0, 5.0), 1.4, 2.0, 0.4, None),
        ];

        system.try_form_caravan(&candidates);
        let cid = system.caravans()[0].id;
        let members = system.caravans()[0].member_ids.clone();

        // Rebuild candidates with caravan_id set.
        let updated: Vec<_> = candidates
            .iter()
            .map(|c| {
                let caravan_id = if members.contains(&c.id) {
                    Some(cid)
                } else {
                    None
                };
                candidate(c.id, c.pos, c.heading, c.speed, c.sociability, caravan_id)
            })
            .collect();

        let directives = system.tick_caravan_movement(&updated);
        assert_eq!(directives.len(), 3);
        for d in &directives {
            assert!(
                (d.max_speed - 1.0).abs() < 1e-6,
                "should use slowest speed"
            );
            assert!(
                (d.target_heading - 1.5).abs() < 1e-6,
                "should use leader heading"
            );
        }
    }

    #[test]
    fn leader_gets_zero_offset() {
        let mut system = CaravanSystem::new();
        system.caravans.push(Caravan {
            id: 0,
            member_ids: vec![1, 2, 3],
            leader_id: 1,
            escort_ids: Vec::new(),
        });

        let candidates = vec![
            candidate(1, Vec2::new(0.0, 0.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(2, Vec2::new(5.0, 0.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(3, Vec2::new(5.0, 5.0), 1.0, 2.0, 0.5, Some(0)),
        ];

        let directives = system.tick_caravan_movement(&candidates);
        let leader_dir = directives.iter().find(|d| d.merchant_id == 1).unwrap();
        assert!(
            leader_dir.formation_offset.length() < 1e-6,
            "leader should have zero offset"
        );
    }

    // ── Dissolution tests ────────────────────────────────────────────────

    #[test]
    fn dissolution_when_spread_too_far() {
        let mut system = CaravanSystem::new();
        system.caravans.push(Caravan {
            id: 0,
            member_ids: vec![1, 2, 3],
            leader_id: 1,
            escort_ids: Vec::new(),
        });

        let candidates = vec![
            candidate(1, Vec2::new(0.0, 0.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(2, Vec2::new(150.0, 0.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(3, Vec2::new(5.0, 5.0), 1.0, 2.0, 0.5, Some(0)),
        ];

        let events = system.check_dissolution(&candidates, &[]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].caravan_id, 0);
        assert!(system.caravans().is_empty());
    }

    #[test]
    fn dissolution_when_at_city() {
        let mut system = CaravanSystem::new();
        system.caravans.push(Caravan {
            id: 0,
            member_ids: vec![1, 2, 3],
            leader_id: 1,
            escort_ids: Vec::new(),
        });

        let candidates = vec![
            candidate(1, Vec2::new(100.0, 100.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(2, Vec2::new(105.0, 100.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(3, Vec2::new(102.0, 103.0), 1.0, 2.0, 0.5, Some(0)),
        ];

        let cities = vec![(Vec2::new(100.0, 100.0), 20.0)];
        let events = system.check_dissolution(&candidates, &cities);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn no_dissolution_when_close_together() {
        let mut system = CaravanSystem::new();
        system.caravans.push(Caravan {
            id: 0,
            member_ids: vec![1, 2, 3],
            leader_id: 1,
            escort_ids: Vec::new(),
        });

        let candidates = vec![
            candidate(1, Vec2::new(50.0, 50.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(2, Vec2::new(55.0, 50.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(3, Vec2::new(52.0, 53.0), 1.0, 2.0, 0.5, Some(0)),
        ];

        let events = system.check_dissolution(&candidates, &[]);
        assert!(events.is_empty());
        assert_eq!(system.caravans().len(), 1);
    }

    // ── Escort tests ─────────────────────────────────────────────────────

    #[test]
    fn soldier_escort_charges_fees() {
        let mut system = CaravanSystem::new();
        system.caravans.push(Caravan {
            id: 0,
            member_ids: vec![1, 2, 3],
            leader_id: 1,
            escort_ids: Vec::new(),
        });

        let candidates = vec![
            candidate(1, Vec2::new(50.0, 50.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(2, Vec2::new(55.0, 50.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(3, Vec2::new(52.0, 53.0), 1.0, 2.0, 0.5, Some(0)),
        ];

        let soldiers = vec![SoldierView {
            id: 100,
            pos: Vec2::new(53.0, 51.0),
        }];

        let fees = system.add_soldier_escort(&soldiers, &candidates);
        // 3 members × 1 soldier = 3 fee entries.
        assert_eq!(fees.len(), 3);
        for fee in &fees {
            assert_eq!(fee.soldier_id, 100);
            assert!((fee.amount - ESCORT_FEE_PER_TICK).abs() < 1e-6);
        }
        assert_eq!(system.caravans()[0].escort_ids, vec![100]);
    }

    #[test]
    fn soldier_out_of_range_not_attached() {
        let mut system = CaravanSystem::new();
        system.caravans.push(Caravan {
            id: 0,
            member_ids: vec![1, 2, 3],
            leader_id: 1,
            escort_ids: Vec::new(),
        });

        let candidates = vec![
            candidate(1, Vec2::new(50.0, 50.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(2, Vec2::new(55.0, 50.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(3, Vec2::new(52.0, 53.0), 1.0, 2.0, 0.5, Some(0)),
        ];

        let soldiers = vec![SoldierView {
            id: 100,
            pos: Vec2::new(200.0, 200.0),
        }];

        let fees = system.add_soldier_escort(&soldiers, &candidates);
        assert!(fees.is_empty());
        assert!(system.caravans()[0].escort_ids.is_empty());
    }

    #[test]
    fn multiple_soldiers_multiple_fees() {
        let mut system = CaravanSystem::new();
        system.caravans.push(Caravan {
            id: 0,
            member_ids: vec![1, 2],
            leader_id: 1,
            escort_ids: Vec::new(),
        });

        let candidates = vec![
            candidate(1, Vec2::new(50.0, 50.0), 1.0, 2.0, 0.5, Some(0)),
            candidate(2, Vec2::new(55.0, 50.0), 1.0, 2.0, 0.5, Some(0)),
        ];

        let soldiers = vec![
            SoldierView {
                id: 100,
                pos: Vec2::new(53.0, 51.0),
            },
            SoldierView {
                id: 101,
                pos: Vec2::new(52.0, 49.0),
            },
        ];

        let fees = system.add_soldier_escort(&soldiers, &candidates);
        // 2 members × 2 soldiers = 4 fees.
        assert_eq!(fees.len(), 4);
        assert_eq!(system.caravans()[0].escort_ids.len(), 2);
    }

    // ── Helper tests ─────────────────────────────────────────────────────

    #[test]
    fn heading_diff_same_direction() {
        assert!(heading_diff(1.0, 1.0).abs() < 1e-6);
    }

    #[test]
    fn heading_diff_opposite() {
        use std::f32::consts::PI;
        assert!((heading_diff(0.0, PI) - PI).abs() < 1e-6);
    }

    #[test]
    fn heading_diff_wraps_around() {
        use std::f32::consts::TAU;
        // 0.1 and TAU - 0.1 are 0.2 radians apart.
        assert!((heading_diff(0.1, TAU - 0.1) - 0.2).abs() < 1e-4);
    }

    #[test]
    fn spread_exceeds_threshold() {
        let positions = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(150.0, 0.0),
            Vec2::new(50.0, 0.0),
        ];
        assert!(spread_exceeds(&positions, 100.0));
    }

    #[test]
    fn spread_within_threshold() {
        let positions = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(50.0, 0.0),
            Vec2::new(25.0, 10.0),
        ];
        assert!(!spread_exceeds(&positions, 100.0));
    }
}
