use crate::agents::actions::MerchantAction;
use crate::agents::merchant::Merchant;
use crate::agents::sensory::SensoryInput;
use crate::types::{AgentState, Commodity, MarketAction, Season, TerrainType};

use super::interface::Brain;
use super::trader::{heading_delta_toward, terrain_avoidance};

/// Shipwright FSM: LOADING → SAILING → UNLOADING
/// Coast cells only, 2.0× speed, 3.0× carry. Requires harbors at both endpoints.
/// Cannot operate in winter — rests at city instead.
pub struct ShipwrightBrain;

impl Brain for ShipwrightBrain {
    fn decide(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // Winter shutdown: cannot sail, rest at city
        if sensory.current_season == Season::Winter {
            return self.winter_rest(sensory, merchant);
        }

        for _ in 0..8 {
            let prev_state = merchant.state;
            let action = match merchant.state {
                AgentState::Loading => self.loading(sensory, merchant),
                AgentState::Sailing => self.sailing(sensory, merchant),
                AgentState::Unloading => self.unloading(sensory, merchant),
                AgentState::Resting => self.resting(sensory, merchant),
                _ => {
                    merchant.state = AgentState::Loading;
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

impl ShipwrightBrain {
    /// LOADING: Buy goods at harbor city, prepare for sea voyage.
    fn loading(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        if sensory.fatigue > 70.0 {
            merchant.state = AgentState::Resting;
            return MerchantAction::default();
        }

        // Move to nearest city if not there
        if sensory.nearest_city.1 > 25.0 {
            let turn = heading_delta_toward(merchant.heading, sensory.nearest_city.0);
            let terrain_turn = terrain_avoidance(sensory);
            return MerchantAction {
                turn: turn * 0.7 + terrain_turn * 0.3,
                speed_mult: 0.9,
                ..Default::default()
            };
        }

        // If inventory sufficiently full, set sail
        if sensory.inventory_fill_ratio > 0.6 {
            merchant.state = AgentState::Sailing;
            return MerchantAction::default();
        }

        // Buy bulk goods — prefer high-value commodities for sea trade
        if let Some((commodity, price)) = self.best_cargo(sensory, merchant) {
            let space = merchant.max_carry - merchant.inventory_weight();
            let affordable = (sensory.gold / price.max(0.01)).min(space);
            if affordable > 0.1 {
                return MerchantAction {
                    turn: 0.0,
                    speed_mult: 0.0,
                    market_action: MarketAction::Buy {
                        commodity,
                        max_price: price * 1.05,
                        quantity: affordable,
                    },
                    ..Default::default()
                };
            }
        }

        // Can't buy more — sail with what we have
        if sensory.inventory_fill_ratio > 0.1 {
            merchant.state = AgentState::Sailing;
            MerchantAction::default()
        } else {
            // Wait at harbor — rest to recover
            MerchantAction {
                turn: 0.0,
                speed_mult: 0.0,
                rest: sensory.fatigue > 10.0,
                ..Default::default()
            }
        }
    }

    /// SAILING: Navigate along coast cells.
    fn sailing(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // Arrived at a city with goods — unload
        if sensory.nearest_city.1 < 25.0 && sensory.inventory_fill_ratio > 0.05 {
            merchant.state = AgentState::Unloading;
            return MerchantAction::default();
        }

        // Fatigue check near city
        if sensory.fatigue > 80.0 && sensory.nearest_city.1 < 50.0 {
            merchant.state = AgentState::Resting;
            return MerchantAction::default();
        }

        // Navigate along coast: prefer coast terrain rays, avoid deep water and land
        let coast_turn = self.coast_navigation(sensory);
        let profit_turn = heading_delta_toward(merchant.heading, sensory.profit_gradient);
        let city_turn = heading_delta_toward(merchant.heading, sensory.nearest_city.0);
        let turn = coast_turn * 0.4 + profit_turn * 0.3 + city_turn * 0.3;

        let speed = if sensory.current_terrain == TerrainType::Coast {
            1.0
        } else {
            0.8
        };

        MerchantAction {
            turn,
            speed_mult: speed,
            ..Default::default()
        }
    }

    /// UNLOADING: Sell cargo at destination harbor.
    fn unloading(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        if sensory.nearest_city.1 > 25.0 {
            merchant.state = AgentState::Sailing;
            return MerchantAction::default();
        }

        // Sell goods, highest tier first
        let mut to_sell: Vec<(Commodity, f32)> = merchant
            .inventory
            .iter()
            .filter(|(_, &q)| q > 0.01)
            .map(|(&c, &q)| (c, q))
            .collect();
        to_sell.sort_by(|a, b| b.0.tier().cmp(&a.0.tier()));

        if let Some(&(commodity, qty)) = to_sell.first() {
            let min_price = match commodity.tier() {
                0 => 3.0,
                1 => 10.0,
                2 => 30.0,
                _ => 80.0,
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

        // All sold — rest or load for next voyage
        if sensory.fatigue > 30.0 {
            merchant.state = AgentState::Resting;
        } else {
            merchant.state = AgentState::Loading;
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

        merchant.state = AgentState::Loading;
        MerchantAction::default()
    }

    /// Winter rest: head to nearest city and stay idle.
    fn winter_rest(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        merchant.state = AgentState::Resting;
        if sensory.nearest_city.1 > 25.0 {
            let turn = heading_delta_toward(merchant.heading, sensory.nearest_city.0);
            let terrain_turn = terrain_avoidance(sensory);
            MerchantAction {
                turn: turn * 0.7 + terrain_turn * 0.3,
                speed_mult: 0.5,
                ..Default::default()
            }
        } else {
            MerchantAction {
                turn: 0.0,
                speed_mult: 0.0,
                rest: true,
                ..Default::default()
            }
        }
    }

    /// Coast navigation: steer to stay on coast cells.
    fn coast_navigation(&self, sensory: &SensoryInput) -> f32 {
        let rays = &sensory.terrain_rays;
        let weights = [-1.0f32, -0.5, 0.0, 0.5, 1.0];
        let mut weighted_sum = 0.0f32;
        let mut total_weight = 0.0f32;

        for (i, ray) in rays.iter().enumerate() {
            let score = match ray.terrain_type {
                TerrainType::Coast => 2.0,
                TerrainType::Plains => 0.5,
                TerrainType::Water => -1.0,
                TerrainType::Mountains => -1.0,
                _ => 0.0,
            };
            if score > 0.0 {
                weighted_sum += score * weights[i];
                total_weight += score;
            }
        }

        if total_weight < 0.01 {
            return 0.0;
        }
        let bias = weighted_sum / total_weight;
        bias * MerchantAction::MAX_TURN * 0.5
    }

    fn best_cargo(
        &self,
        sensory: &SensoryInput,
        merchant: &Merchant,
    ) -> Option<(Commodity, f32)> {
        let entries = merchant.price_memory.all_entries();
        let mut best: Option<(Commodity, f32, f32)> = None;

        for &commodity in Commodity::ALL.iter() {
            let mut min_buy = f32::MAX;
            let mut max_sell = 0.0f32;
            for prices in entries.values() {
                if let Some(entry) = prices.get(&commodity) {
                    min_buy = min_buy.min(entry.price);
                    max_sell = max_sell.max(entry.price);
                }
            }
            if max_sell > min_buy && min_buy < sensory.gold {
                let margin = max_sell - min_buy;
                if best.as_ref().map_or(true, |(_, _, m)| margin > *m) {
                    best = Some((commodity, min_buy, margin));
                }
            }
        }

        best.map(|(c, p, _)| (c, p)).or_else(|| {
            // Fallback: buy any cheap raw commodity
            if sensory.gold > 5.0 {
                Some((Commodity::Timber, 5.0))
            } else {
                None
            }
        })
    }
}
