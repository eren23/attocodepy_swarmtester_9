use crate::agents::actions::MerchantAction;
use crate::agents::merchant::Merchant;
use crate::agents::sensory::SensoryInput;
use crate::types::{AgentState, Profession, ReputationChannel};

use super::interface::Brain;
use super::trader::{heading_delta_toward, steer_toward_signal, terrain_avoidance};

/// Soldier FSM: PATROLLING → ESCORTING → FIGHTING → PATROLLING
/// Patrols high road-value cells between cities. Risk tolerance affects patrol range.
/// Escorts caravans for fees. Engages bandits within 30px (50/50 outcome).
pub struct SoldierBrain;

impl Brain for SoldierBrain {
    fn decide(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // Priority interrupt: bandit within engage range → fight
        if let Some((_, bandit_dist)) = sensory.nearest_bandit {
            let engage_range = 30.0 + merchant.traits.risk_tolerance * 20.0;
            if bandit_dist < engage_range && merchant.state != AgentState::Escorting {
                merchant.state = AgentState::Fighting;
            }
        }

        for _ in 0..8 {
            let prev_state = merchant.state;
            let action = match merchant.state {
                AgentState::Patrolling => self.patrolling(sensory, merchant),
                AgentState::Escorting => self.escorting(sensory, merchant),
                AgentState::Fighting => self.fighting(sensory, merchant),
                AgentState::Resting => self.resting(sensory, merchant),
                _ => {
                    merchant.state = AgentState::Patrolling;
                    MerchantAction::default()
                }
            };
            if merchant.state == prev_state {
                return action;
            }
        }
        MerchantAction::default()
    }
}

impl SoldierBrain {
    /// Patrol high road-value cells between cities. Risk tolerance affects range.
    fn patrolling(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        if sensory.fatigue > 70.0 {
            merchant.state = AgentState::Resting;
            return MerchantAction::default();
        }

        // Check for caravans to escort
        let caravan_nearby = sensory
            .neighbors
            .iter()
            .any(|n| n.caravan_id.is_some() && n.profession != Profession::Soldier);
        if caravan_nearby && merchant.traits.sociability > 0.3 {
            merchant.state = AgentState::Escorting;
            return MerchantAction::default();
        }

        // Follow roads: steer toward terrain rays with high road values
        let road_turn = self.road_following_turn(sensory);

        // Patrol toward danger signals to intercept bandits
        let danger_left = sensory.left_scanner[2];
        let danger_right = sensory.right_scanner[2];
        let danger_turn =
            steer_toward_signal(danger_left, danger_right) * merchant.traits.risk_tolerance;

        // Pull back toward cities if too far out
        let city_pull =
            if sensory.nearest_city.1 > 150.0 * (1.0 + merchant.traits.risk_tolerance) {
                heading_delta_toward(merchant.heading, sensory.nearest_city.0) * 0.5
            } else {
                0.0
            };

        let terrain_turn = terrain_avoidance(sensory);
        let turn = road_turn * 0.3 + danger_turn * 0.3 + city_pull * 0.2 + terrain_turn * 0.2;

        MerchantAction {
            turn,
            speed_mult: 0.7,
            ..Default::default()
        }
    }

    /// Attach to caravan, receive fees. Follow caravan members.
    fn escorting(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        if sensory.fatigue > 80.0 {
            merchant.state = AgentState::Resting;
            return MerchantAction {
                leave_caravan: true,
                ..MerchantAction::default()
            };
        }

        // If no caravan members nearby, go back to patrolling
        let has_caravan_member = sensory.neighbors.iter().any(|n| n.caravan_id.is_some());
        if !has_caravan_member {
            merchant.state = AgentState::Patrolling;
            return MerchantAction::default();
        }

        // Follow the caravan — move toward center of caravan members
        let caravan_members: Vec<_> = sensory
            .neighbors
            .iter()
            .filter(|n| n.caravan_id.is_some())
            .collect();

        let avg_dir = if !caravan_members.is_empty() {
            let sum = caravan_members
                .iter()
                .fold(crate::types::Vec2::ZERO, |acc, n| acc + n.relative_pos);
            sum * (1.0 / caravan_members.len() as f32)
        } else {
            crate::types::Vec2::ZERO
        };

        let turn = heading_delta_toward(merchant.heading, avg_dir);

        MerchantAction {
            turn,
            speed_mult: 0.8,
            join_caravan: merchant.caravan_id.is_none(),
            ..Default::default()
        }
    }

    /// Engage bandits within range. 50/50 outcome handled by simulation.
    fn fighting(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        match sensory.nearest_bandit {
            Some((bandit_dir, bandit_dist)) => {
                if bandit_dist > 50.0 {
                    merchant.state = AgentState::Patrolling;
                    return MerchantAction::default();
                }

                // Charge toward bandit
                let turn = heading_delta_toward(merchant.heading, bandit_dir);

                MerchantAction {
                    turn,
                    speed_mult: 1.0,
                    deposit_signal: Some(ReputationChannel::Danger),
                    signal_strength: 0.6,
                    ..Default::default()
                }
            }
            None => {
                merchant.state = AgentState::Patrolling;
                MerchantAction::default()
            }
        }
    }

    fn resting(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        if sensory.nearest_city.1 > 25.0 {
            let turn = heading_delta_toward(merchant.heading, sensory.nearest_city.0);
            return MerchantAction {
                turn,
                speed_mult: 0.5,
                ..Default::default()
            };
        }

        if sensory.fatigue > 20.0 {
            return MerchantAction {
                turn: 0.0,
                speed_mult: 0.0,
                rest: true,
                ..Default::default()
            };
        }

        merchant.state = AgentState::Patrolling;
        MerchantAction::default()
    }

    /// Steer toward terrain rays with highest road values.
    fn road_following_turn(&self, sensory: &SensoryInput) -> f32 {
        let rays = &sensory.terrain_rays;
        let weights = [-1.0f32, -0.5, 0.0, 0.5, 1.0];
        let mut weighted_sum = 0.0f32;
        let mut total_road = 0.0f32;
        for (i, ray) in rays.iter().enumerate() {
            weighted_sum += ray.road_value * weights[i];
            total_road += ray.road_value;
        }
        if total_road < 0.01 {
            return 0.0;
        }
        let bias = weighted_sum / total_road;
        bias * MerchantAction::MAX_TURN * 0.5
    }
}
