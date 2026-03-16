use crate::agents::actions::MerchantAction;
use crate::agents::merchant::Merchant;
use crate::agents::sensory::SensoryInput;
use crate::types::{AgentState, Commodity, MarketAction, ReputationChannel, Season};

use super::interface::Brain;
use super::trader::{heading_delta_toward, terrain_avoidance};

/// Farmer FSM: TRAVELING_TO_NODE → EXTRACTING → TRAVELING_TO_CITY → SELLING → RESTING
/// Targets GRAIN/HERBS/FISH. Summer: prioritize GRAIN/HERBS. Winter: switch to FISH.
/// Deposits OPPORTUNITY at abundant sites.
pub struct FarmerBrain;

impl Brain for FarmerBrain {
    fn decide(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        for _ in 0..8 {
            let prev_state = merchant.state;
            let action = match merchant.state {
                AgentState::TravelingToNode => self.traveling_to_node(sensory, merchant),
                AgentState::Extracting => self.extracting(sensory, merchant),
                AgentState::TravelingToCity => self.traveling_to_city(sensory, merchant),
                AgentState::Selling => self.selling(sensory, merchant),
                AgentState::Resting => self.resting(sensory, merchant),
                _ => {
                    merchant.state = AgentState::TravelingToNode;
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

impl FarmerBrain {
    /// Returns true if the commodity is a valid farming target for the current season.
    fn is_seasonal_target(commodity: Commodity, season: Season) -> bool {
        match season {
            Season::Winter => commodity == Commodity::Fish,
            Season::Summer => matches!(commodity, Commodity::Grain | Commodity::Herbs),
            _ => matches!(commodity, Commodity::Grain | Commodity::Herbs | Commodity::Fish),
        }
    }

    fn traveling_to_node(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        if sensory.fatigue > 80.0 {
            merchant.state = AgentState::Resting;
            return MerchantAction::default();
        }

        if let Some((dir, dist, commodity)) = sensory.nearest_resource {
            if Self::is_seasonal_target(commodity, sensory.current_season) {
                if dist < 10.0 {
                    merchant.state = AgentState::Extracting;
                    return MerchantAction::default();
                }

                let turn = heading_delta_toward(merchant.heading, dir);
                let terrain_turn = terrain_avoidance(sensory);
                return MerchantAction {
                    turn: turn * 0.7 + terrain_turn * 0.3,
                    speed_mult: 0.9,
                    ..Default::default()
                };
            }
        }

        // No suitable node — wander toward opportunity signals
        let left_opp = sensory.left_scanner[3]; // OPPORTUNITY index
        let right_opp = sensory.right_scanner[3];
        let opp_turn = if (right_opp - left_opp).abs() > 0.01 {
            (right_opp - left_opp).signum() * MerchantAction::MAX_TURN * 0.3
        } else {
            0.0
        };
        let terrain_turn = terrain_avoidance(sensory);
        MerchantAction {
            turn: opp_turn * 0.5 + terrain_turn * 0.5,
            speed_mult: 0.7,
            ..Default::default()
        }
    }

    fn extracting(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        if sensory.inventory_fill_ratio > 0.9 {
            merchant.state = AgentState::TravelingToCity;
            return MerchantAction::default();
        }

        if let Some((_, dist, _)) = sensory.nearest_resource {
            if dist > 15.0 {
                merchant.state = AgentState::TravelingToNode;
                return MerchantAction::default();
            }
        }

        // Deposit OPPORTUNITY signal at abundant extraction sites
        MerchantAction {
            turn: 0.0,
            speed_mult: 0.0,
            extract: true,
            deposit_signal: Some(ReputationChannel::Opportunity),
            signal_strength: 0.5,
            ..Default::default()
        }
    }

    fn traveling_to_city(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        if sensory.fatigue > 80.0 && sensory.nearest_city.1 < 25.0 {
            merchant.state = AgentState::Resting;
            return MerchantAction::default();
        }

        if sensory.nearest_city.1 < 25.0 {
            merchant.state = AgentState::Selling;
            return MerchantAction::default();
        }

        let turn = heading_delta_toward(merchant.heading, sensory.nearest_city.0);
        let terrain_turn = terrain_avoidance(sensory);
        MerchantAction {
            turn: turn * 0.7 + terrain_turn * 0.3,
            speed_mult: 1.0,
            ..Default::default()
        }
    }

    fn selling(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        if sensory.nearest_city.1 > 25.0 {
            merchant.state = AgentState::TravelingToCity;
            return MerchantAction::default();
        }

        // Sell farm commodities in priority order
        for &commodity in &[Commodity::Grain, Commodity::Herbs, Commodity::Fish] {
            let qty = merchant.inventory.get(&commodity).copied().unwrap_or(0.0);
            if qty > 0.01 {
                return MerchantAction {
                    turn: 0.0,
                    speed_mult: 0.0,
                    market_action: MarketAction::Sell {
                        commodity,
                        min_price: 3.0,
                        quantity: qty,
                    },
                    ..Default::default()
                };
            }
        }

        // Sell any remaining
        if let Some((&commodity, &qty)) = merchant.inventory.iter().find(|(_, &q)| q > 0.01) {
            return MerchantAction {
                turn: 0.0,
                speed_mult: 0.0,
                market_action: MarketAction::Sell {
                    commodity,
                    min_price: 1.0,
                    quantity: qty,
                },
                ..Default::default()
            };
        }

        if sensory.fatigue > 30.0 {
            merchant.state = AgentState::Resting;
        } else {
            merchant.state = AgentState::TravelingToNode;
        }
        MerchantAction::default()
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

        merchant.state = AgentState::TravelingToNode;
        MerchantAction::default()
    }
}
