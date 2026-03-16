use crate::agents::actions::MerchantAction;
use crate::agents::merchant::Merchant;
use crate::agents::sensory::SensoryInput;

/// Trait implemented by each profession's FSM brain.
///
/// The simulation calls `decide` once per tick per merchant. The brain
/// inspects the merchant's current `state` field, the sensory input, and
/// the merchant's traits / inventory / price memory to produce a
/// `MerchantAction`. It also updates `merchant.state` to drive FSM
/// transitions.
pub trait Brain: Send + Sync {
    /// Produce one tick's action and mutate `merchant.state` for FSM transitions.
    fn decide(&self, sensory: &SensoryInput, merchant: &mut Merchant) -> MerchantAction;
}
