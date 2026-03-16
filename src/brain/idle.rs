use crate::agents::actions::MerchantAction;
use crate::agents::merchant::Merchant;
use crate::agents::sensory::SensoryInput;
use crate::types::AgentState;

use super::interface::Brain;
use super::trader::{heading_delta_toward, terrain_avoidance};

/// Idle FSM: Wander toward nearest city. Await profession assignment.
pub struct IdleBrain;

impl Brain for IdleBrain {
    fn decide(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        merchant.state = AgentState::Idle;

        // Head toward nearest city
        let city_turn = heading_delta_toward(merchant.heading, sensory.nearest_city.0);
        let terrain_turn = terrain_avoidance(sensory);

        // At city: rest while waiting for assignment
        if sensory.nearest_city.1 < 25.0 {
            return MerchantAction {
                turn: 0.0,
                speed_mult: 0.0,
                rest: sensory.fatigue > 10.0,
                ..Default::default()
            };
        }

        // Slow wander toward city — conserve energy
        MerchantAction {
            turn: city_turn * 0.7 + terrain_turn * 0.3,
            speed_mult: 0.5,
            ..Default::default()
        }
    }
}
