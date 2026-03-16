use crate::agents::actions::MerchantAction;
use crate::agents::merchant::Merchant;
use crate::agents::sensory::SensoryInput;
use crate::types::{AgentState, Commodity, MarketAction, Season};

use super::interface::Brain;
use super::trader::{heading_delta_toward, terrain_avoidance};

/// Miner FSM: TRAVELING_TO_NODE → EXTRACTING → TRAVELING_TO_CITY → SELLING → RESTING
/// Targets ORE/CLAY. Avoids frozen CLAY in winter.
pub struct MinerBrain;

impl Brain for MinerBrain {
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

impl MinerBrain {
    /// Select resource node by proximity × yield × inverse competition.
    /// Prefer ORE/CLAY, avoid frozen CLAY in winter.
    fn traveling_to_node(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // Fatigue check
        if sensory.fatigue > 80.0 {
            merchant.state = AgentState::Resting;
            return MerchantAction::default();
        }

        if let Some((dir, dist, commodity)) = sensory.nearest_resource {
            // Skip frozen CLAY in winter
            let is_frozen_clay =
                commodity == Commodity::Clay && sensory.current_season == Season::Winter;

            let is_target = matches!(commodity, Commodity::Ore | Commodity::Clay) && !is_frozen_clay;

            if is_target {
                // At the node — start extracting
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

        // No suitable node found — wander toward resource signals
        let turn = heading_delta_toward(merchant.heading, sensory.profit_gradient);
        let terrain_turn = terrain_avoidance(sensory);
        MerchantAction {
            turn: turn * 0.5 + terrain_turn * 0.5,
            speed_mult: 0.7,
            ..Default::default()
        }
    }

    /// Extract 3 ticks/unit until full.
    fn extracting(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // If inventory full, go sell
        if sensory.inventory_fill_ratio > 0.9 {
            merchant.state = AgentState::TravelingToCity;
            return MerchantAction::default();
        }

        // Check we're still at a resource node
        if let Some((_, dist, _)) = sensory.nearest_resource {
            if dist > 15.0 {
                // Drifted away from node
                merchant.state = AgentState::TravelingToNode;
                return MerchantAction::default();
            }
        }

        MerchantAction {
            turn: 0.0,
            speed_mult: 0.0,
            extract: true,
            ..Default::default()
        }
    }

    fn traveling_to_city(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        if sensory.fatigue > 80.0 && sensory.nearest_city.1 < 25.0 {
            merchant.state = AgentState::Resting;
            return MerchantAction::default();
        }

        // Arrived at city
        if sensory.nearest_city.1 < 25.0 {
            merchant.state = AgentState::Selling;
            return MerchantAction::default();
        }

        // Head toward nearest city
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

        // Sell ORE first, then CLAY
        for &commodity in &[Commodity::Ore, Commodity::Clay] {
            let qty = merchant.inventory.get(&commodity).copied().unwrap_or(0.0);
            if qty > 0.01 {
                let min_price = match commodity.tier() {
                    0 => 3.0,
                    _ => 10.0,
                };
                return MerchantAction {
                    turn: 0.0,
                    speed_mult: 0.0,
                    market_action: MarketAction::Sell {
                        commodity,
                        min_price,
                        quantity: qty,
                    },
                    ..Default::default()
                };
            }
        }

        // Sell any remaining raw commodities
        if let Some((&commodity, &qty)) = merchant
            .inventory
            .iter()
            .find(|(_, &q)| q > 0.01)
        {
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

        // Nothing to sell — rest or go mine again
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
