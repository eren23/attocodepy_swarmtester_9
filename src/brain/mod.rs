pub mod interface;
pub mod trader;
pub mod miner;
pub mod farmer;
pub mod craftsman;
pub mod soldier;
pub mod shipwright;
pub mod idle;

use crate::types::Profession;
use interface::Brain;
use trader::TraderBrain;
use miner::MinerBrain;
use farmer::FarmerBrain;
use craftsman::CraftsmanBrain;
use soldier::SoldierBrain;
use shipwright::ShipwrightBrain;
use idle::IdleBrain;

/// Factory: create the appropriate Brain implementation for a profession.
pub fn brain_for_profession(profession: Profession) -> Box<dyn Brain> {
    match profession {
        Profession::Trader => Box::new(TraderBrain),
        Profession::Miner => Box::new(MinerBrain),
        Profession::Farmer => Box::new(FarmerBrain),
        Profession::Craftsman => Box::new(CraftsmanBrain),
        Profession::Soldier => Box::new(SoldierBrain),
        Profession::Shipwright => Box::new(ShipwrightBrain),
        Profession::Idle => Box::new(IdleBrain),
    }
}
