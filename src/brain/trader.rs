use crate::agents::actions::MerchantAction;
use crate::agents::merchant::Merchant;
use crate::agents::sensory::SensoryInput;
use crate::types::{AgentState, Commodity, MarketAction, ReputationChannel, Vec2};

use super::interface::Brain;

/// Trader FSM: SCOUTING → BUYING → TRANSPORTING → SELLING → RESTING → SCOUTING
/// Plus FLEEING state triggered by nearby bandits.
pub struct TraderBrain;

impl Brain for TraderBrain {
    fn decide(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // Global flee check: bandit within 50px and not in a safe caravan
        if let Some((_bandit_dir, bandit_dist)) = sensory.nearest_bandit {
            let in_caravan = merchant.caravan_id.is_some();
            let flee_threshold = 50.0 * (1.0 - merchant.traits.risk_tolerance * 0.5);
            if bandit_dist < flee_threshold && !in_caravan {
                merchant.state = AgentState::Fleeing;
            }
        }

        for _ in 0..8 {
            let prev_state = merchant.state;
            let action = match merchant.state {
                AgentState::Scouting => self.scouting(sensory, merchant),
                AgentState::Buying => self.buying(sensory, merchant),
                AgentState::Transporting => self.transporting(sensory, merchant),
                AgentState::Selling => self.selling(sensory, merchant),
                AgentState::Resting => self.resting(sensory, merchant),
                AgentState::Fleeing => self.fleeing(sensory, merchant),
                _ => {
                    merchant.state = AgentState::Scouting;
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

impl TraderBrain {
    // ── SCOUTING ─────────────────────────────────────────────────────────

    /// Wander toward PROFIT/DEMAND signals, greed-weighted.
    /// Transition to BUYING when at a city with a profitable route (margin > 15%).
    fn scouting(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        let greed = merchant.traits.greed;

        // Blend PROFIT (index 0) and DEMAND (index 1) weighted by greed
        let profit_weight = 0.5 + greed * 0.5; // [0.5, 1.0]
        let demand_weight = 1.0 - profit_weight;

        let left_score = sensory.left_scanner[0] * profit_weight
            + sensory.left_scanner[1] * demand_weight;
        let right_score = sensory.right_scanner[0] * profit_weight
            + sensory.right_scanner[1] * demand_weight;

        // Steer toward stronger signal
        let turn = steer_toward_signal(left_score, right_score);

        // Also follow profit gradient
        let grad_turn = heading_delta_toward(merchant.heading, sensory.profit_gradient);
        let blended_turn = turn * 0.6 + grad_turn * 0.4;

        // Check transition: at a city and can find a profitable route
        if sensory.nearest_city.1 < 25.0 && self.has_profitable_route(sensory, merchant) {
            merchant.state = AgentState::Buying;
            return MerchantAction::default();
        }

        // Try to join caravan if sociable
        let join_caravan = merchant.traits.sociability > 0.5
            && merchant.caravan_id.is_none()
            && !sensory.neighbors.is_empty();

        MerchantAction {
            turn: blended_turn,
            speed_mult: 0.8,
            join_caravan,
            ..Default::default()
        }
    }

    // ── BUYING ───────────────────────────────────────────────────────────

    /// At city: rank commodities by margin, buy highest first.
    /// Greed affects diversification (high greed = go all-in on best commodity).
    fn buying(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // If not at a city, move to nearest
        if sensory.nearest_city.1 > 25.0 {
            merchant.state = AgentState::Scouting;
            return MerchantAction::default();
        }

        // If inventory is near full, transition to transporting
        if sensory.inventory_fill_ratio > 0.85 {
            merchant.state = AgentState::Transporting;
            return MerchantAction::default();
        }

        // Find best commodity to buy based on price memory margins
        if let Some((commodity, buy_price)) = self.best_buy_commodity(sensory, merchant) {
            let space = merchant.max_carry - merchant.inventory_weight();
            // Greed: high greed buys more of one commodity
            let max_buy = if merchant.traits.greed > 0.7 {
                space // go all-in
            } else {
                space * 0.5 // diversify
            };
            let affordable = (sensory.gold / buy_price.max(0.01)).min(max_buy);
            let quantity = affordable.max(0.0).min(space);

            if quantity > 0.1 {
                return MerchantAction {
                    turn: 0.0,
                    speed_mult: 0.0,
                    market_action: MarketAction::Buy {
                        commodity,
                        max_price: buy_price * 1.05, // slight overbid
                        quantity,
                    },
                    ..Default::default()
                };
            }
        }

        // Nothing profitable or no gold — transition to transporting if we have goods
        if sensory.inventory_fill_ratio > 0.1 {
            merchant.state = AgentState::Transporting;
        } else {
            // No goods, no money — go scout
            merchant.state = AgentState::Scouting;
        }
        MerchantAction::default()
    }

    // ── TRANSPORTING ─────────────────────────────────────────────────────

    /// Move toward sell city. Avoid DANGER modulated by risk_tolerance.
    /// Detour for fatigue > 70 toward nearest city to rest.
    fn transporting(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // Fatigue detour
        if sensory.fatigue > 70.0 {
            let turn = heading_delta_toward(merchant.heading, sensory.nearest_city.0);
            if sensory.nearest_city.1 < 25.0 {
                merchant.state = AgentState::Resting;
                return MerchantAction::default();
            }
            return MerchantAction {
                turn,
                speed_mult: 0.6,
                ..Default::default()
            };
        }

        // If at a city and have goods, try selling
        if sensory.nearest_city.1 < 25.0 && sensory.inventory_fill_ratio > 0.05 {
            merchant.state = AgentState::Selling;
            return MerchantAction::default();
        }

        // Navigate using profit gradient (toward high-profit areas = sell targets)
        let profit_turn = heading_delta_toward(merchant.heading, sensory.profit_gradient);

        // Avoid danger based on risk tolerance
        let danger_avoidance = if sensory.danger_gradient.length() > 0.01 {
            let avoid_turn =
                heading_delta_toward(merchant.heading, -sensory.danger_gradient);
            avoid_turn * (1.0 - merchant.traits.risk_tolerance)
        } else {
            0.0
        };

        let turn = profit_turn * 0.7 + danger_avoidance * 0.3;

        // Terrain avoidance via rays
        let terrain_turn = terrain_avoidance(sensory);
        let final_turn = turn * 0.7 + terrain_turn * 0.3;

        let join_caravan = merchant.traits.sociability > 0.4
            && merchant.caravan_id.is_none()
            && !sensory.neighbors.is_empty();

        MerchantAction {
            turn: final_turn,
            speed_mult: 1.0,
            join_caravan,
            ..Default::default()
        }
    }

    // ── SELLING ──────────────────────────────────────────────────────────

    /// Sell goods, deposit PROFIT/DANGER signals, update price memory.
    fn selling(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        if sensory.nearest_city.1 > 25.0 {
            merchant.state = AgentState::Transporting;
            return MerchantAction::default();
        }

        // Find most valuable commodity to sell
        if let Some((commodity, quantity)) = self.best_sell_commodity(sensory, merchant) {
            let min_price = self.estimated_sell_price(sensory, merchant, commodity) * 0.95;

            return MerchantAction {
                turn: 0.0,
                speed_mult: 0.0,
                market_action: MarketAction::Sell {
                    commodity,
                    min_price,
                    quantity,
                },
                deposit_signal: Some(ReputationChannel::Profit),
                signal_strength: 0.6,
                leave_caravan: merchant.caravan_id.is_some(),
                ..Default::default()
            };
        }

        // Nothing left to sell → rest or scout
        if sensory.fatigue > 30.0 {
            merchant.state = AgentState::Resting;
        } else {
            merchant.state = AgentState::Scouting;
        }
        MerchantAction::default()
    }

    // ── RESTING ──────────────────────────────────────────────────────────

    fn resting(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // Move toward nearest city if not there
        if sensory.nearest_city.1 > 25.0 {
            let turn = heading_delta_toward(merchant.heading, sensory.nearest_city.0);
            return MerchantAction {
                turn,
                speed_mult: 0.5,
                ..Default::default()
            };
        }

        // Rest until fatigue < 20
        if sensory.fatigue > 20.0 {
            return MerchantAction {
                turn: 0.0,
                speed_mult: 0.0,
                rest: true,
                ..Default::default()
            };
        }

        // Done resting → scout
        merchant.state = AgentState::Scouting;
        MerchantAction::default()
    }

    // ── FLEEING ──────────────────────────────────────────────────────────

    /// Turn away from bandit, max speed, head to nearest city, try join caravan.
    fn fleeing(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction {
        // If bandit is gone or far, stop fleeing
        match sensory.nearest_bandit {
            Some((_, dist)) if dist < 80.0 => {}
            _ => {
                merchant.state = AgentState::Scouting;
                return MerchantAction::default();
            }
        }

        let (bandit_dir, _) = sensory.nearest_bandit.unwrap();

        // Turn away from bandit, blend with toward nearest city
        let away_turn = heading_delta_toward(merchant.heading, -bandit_dir);
        let city_turn = heading_delta_toward(merchant.heading, sensory.nearest_city.0);
        let turn = away_turn * 0.6 + city_turn * 0.4;

        MerchantAction {
            turn,
            speed_mult: 1.0,
            join_caravan: true,
            deposit_signal: Some(ReputationChannel::Danger),
            signal_strength: 0.8,
            ..Default::default()
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Check if price memory suggests a route with > 15% margin.
    fn has_profitable_route(&self, _sensory: &SensoryInput, merchant: &Merchant) -> bool {
        let entries = merchant.price_memory.all_entries();
        // Look for any commodity where buy price at one city < sell price at another * 0.85
        for (_city_a, prices_a) in entries {
            for (_city_b, prices_b) in entries {
                for (&commodity, entry_a) in prices_a {
                    if let Some(entry_b) = prices_b.get(&commodity) {
                        let margin = (entry_b.price - entry_a.price) / entry_a.price.max(0.01);
                        if margin > 0.15 {
                            return true;
                        }
                    }
                }
            }
        }
        // Also transition if we've been scouting a while (age-based fallback)
        merchant.age % 200 == 0 && merchant.age > 0
    }

    /// Find the best commodity to buy here (highest expected margin).
    fn best_buy_commodity(
        &self,
        sensory: &SensoryInput,
        merchant: &Merchant,
    ) -> Option<(Commodity, f32)> {
        let entries = merchant.price_memory.all_entries();
        let mut best: Option<(Commodity, f32, f32)> = None; // (commodity, buy_price, margin)

        for &commodity in Commodity::ALL.iter() {
            // Estimate local buy price from what we know
            let buy_price = self.estimated_buy_price(sensory, merchant, commodity);
            if buy_price <= 0.0 {
                continue;
            }

            // Find best sell price across known cities
            let mut best_sell = 0.0f32;
            for (_city_id, prices) in entries {
                if let Some(entry) = prices.get(&commodity) {
                    best_sell = best_sell.max(entry.price);
                }
            }

            if best_sell > buy_price {
                let margin = (best_sell - buy_price) / buy_price;
                if best.map_or(true, |(_, _, m)| margin > m) {
                    best = Some((commodity, buy_price, margin));
                }
            }
        }

        best.map(|(c, p, _)| (c, p))
    }

    fn best_sell_commodity(
        &self,
        _sensory: &SensoryInput,
        merchant: &Merchant,
    ) -> Option<(Commodity, f32)> {
        merchant
            .inventory
            .iter()
            .filter(|(_, &qty)| qty > 0.01)
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(&c, &q)| (c, q))
    }

    fn estimated_buy_price(
        &self,
        _sensory: &SensoryInput,
        merchant: &Merchant,
        commodity: Commodity,
    ) -> f32 {
        // Use the minimum known price (cheapest city to buy from)
        let entries = merchant.price_memory.all_entries();
        let mut min_price = f32::MAX;
        for prices in entries.values() {
            if let Some(entry) = prices.get(&commodity) {
                min_price = min_price.min(entry.price);
            }
        }
        if min_price < f32::MAX {
            return min_price;
        }
        // Fallback: tier-based estimate
        match commodity.tier() {
            0 => 5.0,
            1 => 15.0,
            2 => 40.0,
            _ => 100.0,
        }
    }

    fn estimated_sell_price(
        &self,
        _sensory: &SensoryInput,
        merchant: &Merchant,
        commodity: Commodity,
    ) -> f32 {
        let entries = merchant.price_memory.all_entries();
        let mut best = 0.0f32;
        for prices in entries.values() {
            if let Some(entry) = prices.get(&commodity) {
                best = best.max(entry.price);
            }
        }
        if best > 0.0 {
            best
        } else {
            match commodity.tier() {
                0 => 8.0,
                1 => 20.0,
                2 => 55.0,
                _ => 130.0,
            }
        }
    }
}

// ── Shared steering utilities ─────────────────────────────────────────────

/// Compute a turn delta to steer toward a world-space direction vector.
pub(crate) fn heading_delta_toward(heading: f32, target_dir: Vec2) -> f32 {
    if target_dir.length_squared() < 1e-10 {
        return 0.0;
    }
    let target_angle = target_dir.angle();
    let mut delta = target_angle - heading;
    // Normalize to [-π, π]
    while delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    while delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    delta.clamp(-MerchantAction::MAX_TURN, MerchantAction::MAX_TURN)
}

/// Steer left or right based on which scanner cone has a stronger signal.
pub(crate) fn steer_toward_signal(left_score: f32, right_score: f32) -> f32 {
    let diff = right_score - left_score;
    let magnitude = diff.abs().min(1.0);
    diff.signum() * magnitude * MerchantAction::MAX_TURN * 0.5
}

/// Use terrain rays to avoid obstacles. Returns a turn correction.
pub(crate) fn terrain_avoidance(sensory: &SensoryInput) -> f32 {
    let rays = &sensory.terrain_rays;
    // Rays 0,1 are left side, 3,4 are right side, 2 is center
    let left_clearance = (rays[0].distance + rays[1].distance) / 2.0;
    let right_clearance = (rays[3].distance + rays[4].distance) / 2.0;
    let center_clearance = rays[2].distance;

    if center_clearance < 10.0 {
        // Obstacle dead ahead — turn toward more clearance
        if left_clearance > right_clearance {
            -MerchantAction::MAX_TURN * 0.8
        } else {
            MerchantAction::MAX_TURN * 0.8
        }
    } else if left_clearance < 15.0 {
        MerchantAction::MAX_TURN * 0.3 // steer right
    } else if right_clearance < 15.0 {
        -MerchantAction::MAX_TURN * 0.3 // steer left
    } else {
        0.0
    }
}
