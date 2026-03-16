use crate::agents::actions::MerchantAction;
use crate::agents::merchant::Merchant;
use crate::agents::sensory::SensoryInput;
use crate::types::{AgentState, Commodity, MarketAction, Recipe};

use super::interface::Brain;
use super::trader::{heading_delta_toward, terrain_avoidance};

/// Craftsman FSM: BUYING_MATERIALS → CRAFTING → SELLING_GOODS
/// Evaluates recipes by margin/tick, buys inputs, crafts at city using
/// specialization bonus, may travel if local price is low.
pub struct CraftsmanBrain;

impl Brain for CraftsmanBrain {
    fn decide(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        for _ in 0..8 {
            let prev_state = merchant.state;
            let action = match merchant.state {
                AgentState::BuyingMaterials => self.buying_materials(sensory, merchant),
                AgentState::Crafting => self.crafting(sensory, merchant),
                AgentState::SellingGoods => self.selling_goods(sensory, merchant),
                AgentState::Resting => self.resting(sensory, merchant),
                _ => {
                    merchant.state = AgentState::BuyingMaterials;
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

/// All Tier 1 recipe definitions for inline evaluation.
const RECIPE_PAIRS: &[(Commodity, Commodity, Commodity, u32)] = &[
    (Commodity::Timber, Commodity::Ore, Commodity::Tools, 5),
    (Commodity::Grain, Commodity::Herbs, Commodity::Medicine, 5),
    (Commodity::Clay, Commodity::Timber, Commodity::Bricks, 5),
    (Commodity::Ore, Commodity::Clay, Commodity::Metalwork, 8),
    (Commodity::Grain, Commodity::Fish, Commodity::Provisions, 3),
    (Commodity::Herbs, Commodity::Clay, Commodity::Pottery, 4),
];

impl CraftsmanBrain {
    /// Find the best raw material to buy based on what we hold least of.
    fn best_material_to_buy(sensory: &SensoryInput, merchant: &Merchant) -> Option<Commodity> {
        let mut candidates: Vec<(Commodity, f32)> = Commodity::RAW
            .iter()
            .map(|&c| {
                let held = merchant.inventory.get(&c).copied().unwrap_or(0.0);
                (c, held)
            })
            .collect();
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        for (commodity, _) in candidates {
            let price_est = match commodity.tier() {
                0 => 5.0,
                _ => 15.0,
            };
            if sensory.gold > price_est {
                return Some(commodity);
            }
        }
        None
    }

    /// Check if we have enough materials for any Tier 1 recipe.
    fn has_craftable_materials(merchant: &Merchant) -> bool {
        let inv = &merchant.inventory;
        RECIPE_PAIRS.iter().any(|&(a, b, _, _)| {
            inv.get(&a).copied().unwrap_or(0.0) >= 1.0
                && inv.get(&b).copied().unwrap_or(0.0) >= 1.0
        })
    }

    /// Find the best recipe to craft from current inventory (highest margin/tick).
    fn best_craft_recipe(merchant: &Merchant) -> Option<Recipe> {
        let inv = &merchant.inventory;
        // Estimated margin/tick for each recipe (roughly ordered)
        let margins: &[f32] = &[2.0, 2.0, 1.5, 1.8, 2.5, 1.6];

        let mut best: Option<(Recipe, f32)> = None;
        for (i, &(a, b, output, ticks)) in RECIPE_PAIRS.iter().enumerate() {
            let qa = inv.get(&a).copied().unwrap_or(0.0);
            let qb = inv.get(&b).copied().unwrap_or(0.0);
            if qa >= 1.0 && qb >= 1.0 {
                let margin = margins[i];
                if best.as_ref().map_or(true, |(_, m)| margin > *m) {
                    best = Some((
                        Recipe {
                            inputs: vec![(a, 1.0), (b, 1.0)],
                            output,
                            output_quantity: 1.0,
                            craft_ticks: ticks,
                            tier: output.tier(),
                            requires_workshop: output.tier() >= 3,
                        },
                        margin,
                    ));
                }
            }
        }
        best.map(|(r, _)| r)
    }

    fn buying_materials(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // Need to be at a city to buy
        if sensory.nearest_city.1 > 25.0 {
            let turn = heading_delta_toward(merchant.heading, sensory.nearest_city.0);
            let terrain_turn = terrain_avoidance(sensory);
            return MerchantAction {
                turn: turn * 0.7 + terrain_turn * 0.3,
                speed_mult: 0.9,
                ..Default::default()
            };
        }

        // If we have enough materials, transition to crafting
        if Self::has_craftable_materials(merchant) {
            merchant.state = AgentState::Crafting;
            return MerchantAction::default();
        }

        // If inventory nearly full, craft what we can or sell
        if sensory.inventory_fill_ratio > 0.85 {
            if Self::has_craftable_materials(merchant) {
                merchant.state = AgentState::Crafting;
            } else {
                merchant.state = AgentState::SellingGoods;
            }
            return MerchantAction::default();
        }

        // Buy materials
        if let Some(commodity) = Self::best_material_to_buy(sensory, merchant) {
            let price_est: f32 = match commodity.tier() {
                0 => 8.0,
                _ => 20.0,
            };
            let affordable = (sensory.gold / price_est.max(0.01)).min(3.0);
            if affordable > 0.1 {
                return MerchantAction {
                    turn: 0.0,
                    speed_mult: 0.0,
                    market_action: MarketAction::Buy {
                        commodity,
                        max_price: price_est * 1.1,
                        quantity: affordable,
                    },
                    ..Default::default()
                };
            }
        }

        // No gold or nothing to buy — try crafting or sell what we have
        if Self::has_craftable_materials(merchant) {
            merchant.state = AgentState::Crafting;
        } else if sensory.inventory_fill_ratio > 0.05 {
            merchant.state = AgentState::SellingGoods;
        } else {
            merchant.state = AgentState::Resting;
        }
        MerchantAction::default()
    }

    fn crafting(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // Must be at a city to craft
        if sensory.nearest_city.1 > 25.0 {
            merchant.state = AgentState::BuyingMaterials;
            return MerchantAction::default();
        }

        // Find a recipe we can craft
        if let Some(recipe) = Self::best_craft_recipe(merchant) {
            return MerchantAction {
                turn: 0.0,
                speed_mult: 0.0,
                craft: Some(recipe),
                ..Default::default()
            };
        }

        // No craftable recipe — transition based on inventory
        if sensory.inventory_fill_ratio > 0.1 {
            merchant.state = AgentState::SellingGoods;
        } else {
            merchant.state = AgentState::BuyingMaterials;
        }
        MerchantAction::default()
    }

    fn selling_goods(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // If not at a city, travel — blend city direction with profit gradient for better prices
        if sensory.nearest_city.1 > 25.0 {
            let city_turn = heading_delta_toward(merchant.heading, sensory.nearest_city.0);
            let profit_turn = heading_delta_toward(merchant.heading, sensory.profit_gradient);
            let terrain_turn = terrain_avoidance(sensory);
            let turn = city_turn * 0.5 + profit_turn * 0.3 + terrain_turn * 0.2;
            return MerchantAction {
                turn: turn.clamp(-MerchantAction::MAX_TURN, MerchantAction::MAX_TURN),
                speed_mult: 1.0,
                ..Default::default()
            };
        }

        // Sell crafted goods first (higher tier), then raw leftovers
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

        // Nothing to sell
        if sensory.fatigue > 30.0 {
            merchant.state = AgentState::Resting;
        } else {
            merchant.state = AgentState::BuyingMaterials;
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

        merchant.state = AgentState::BuyingMaterials;
        MerchantAction::default()
    }
}
