use crate::types::{MarketAction, ReputationChannel, Recipe};

/// Output of the merchant brain for a single tick.
///
/// The brain (FSM) produces this struct each tick. The simulation then
/// applies it to the merchant via `Merchant::apply_action`.
#[derive(Debug, Clone)]
pub struct MerchantAction {
    /// Heading change in radians, clamped to [-π/6, π/6].
    pub turn: f32,
    /// Speed multiplier in [0.0, 1.0]. Applied on top of base_speed.
    pub speed_mult: f32,
    /// Optional reputation channel to deposit a signal at current position.
    pub deposit_signal: Option<ReputationChannel>,
    /// Strength of the deposited signal in [0.0, 1.0].
    pub signal_strength: f32,
    /// Market order to place this tick (Buy/Sell/None).
    pub market_action: MarketAction,
    /// Whether to harvest at a resource node this tick.
    pub extract: bool,
    /// Attempt to craft a recipe (if at a city with materials).
    pub craft: Option<Recipe>,
    /// Rest at current city to recover fatigue.
    pub rest: bool,
    /// Attempt to join or form a caravan with nearby merchants.
    pub join_caravan: bool,
    /// Leave the current caravan.
    pub leave_caravan: bool,
}

impl Default for MerchantAction {
    fn default() -> Self {
        Self {
            turn: 0.0,
            speed_mult: 1.0,
            deposit_signal: None,
            signal_strength: 0.0,
            market_action: MarketAction::None,
            extract: false,
            craft: None,
            rest: false,
            join_caravan: false,
            leave_caravan: false,
        }
    }
}

impl MerchantAction {
    /// Maximum allowed turn delta per tick (π/6 ≈ 30°).
    pub const MAX_TURN: f32 = std::f32::consts::FRAC_PI_6;

    /// Create a new action with only a turn and speed.
    pub fn movement(turn: f32, speed_mult: f32) -> Self {
        Self {
            turn: turn.clamp(-Self::MAX_TURN, Self::MAX_TURN),
            speed_mult: speed_mult.clamp(0.0, 1.0),
            ..Default::default()
        }
    }

    /// Clamp all fields to their valid ranges.
    pub fn sanitize(&mut self) {
        self.turn = self.turn.clamp(-Self::MAX_TURN, Self::MAX_TURN);
        self.speed_mult = self.speed_mult.clamp(0.0, 1.0);
        self.signal_strength = self.signal_strength.clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_action_is_idle() {
        let a = MerchantAction::default();
        assert_eq!(a.turn, 0.0);
        assert_eq!(a.speed_mult, 1.0);
        assert!(!a.extract);
        assert!(!a.rest);
        assert!(!a.join_caravan);
        assert!(!a.leave_caravan);
        assert!(a.deposit_signal.is_none());
        assert!(a.craft.is_none());
        assert_eq!(a.market_action, MarketAction::None);
    }

    #[test]
    fn movement_clamps_turn() {
        let a = MerchantAction::movement(999.0, 0.5);
        assert!((a.turn - MerchantAction::MAX_TURN).abs() < 1e-6);
    }

    #[test]
    fn movement_clamps_speed_mult() {
        let a = MerchantAction::movement(0.0, 2.5);
        assert!((a.speed_mult - 1.0).abs() < 1e-6);
        let b = MerchantAction::movement(0.0, -1.0);
        assert!((b.speed_mult).abs() < 1e-6);
    }

    #[test]
    fn sanitize_clamps_all_fields() {
        let mut a = MerchantAction {
            turn: 10.0,
            speed_mult: -5.0,
            signal_strength: 3.0,
            ..Default::default()
        };
        a.sanitize();
        assert!((a.turn - MerchantAction::MAX_TURN).abs() < 1e-6);
        assert!((a.speed_mult).abs() < 1e-6);
        assert!((a.signal_strength - 1.0).abs() < 1e-6);
    }
}
