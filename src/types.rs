use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// ── Commodity ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Commodity {
    // Tier 0 — Raw
    Timber,
    Ore,
    Grain,
    Fish,
    Clay,
    Herbs,
    // Tier 1 — Basic Refined
    Tools,
    Medicine,
    Bricks,
    Metalwork,
    Provisions,
    Pottery,
    // Tier 2 — Advanced
    Weapons,
    Furniture,
    Armor,
    Alchemy,
    Machinery,
    FeastGoods,
    // Tier 3 — Elite
    EliteGear,
    Automaton,
    Elixir,
    LuxurySet,
}

impl Commodity {
    pub const ALL: [Commodity; 22] = [
        Commodity::Timber,
        Commodity::Ore,
        Commodity::Grain,
        Commodity::Fish,
        Commodity::Clay,
        Commodity::Herbs,
        Commodity::Tools,
        Commodity::Medicine,
        Commodity::Bricks,
        Commodity::Metalwork,
        Commodity::Provisions,
        Commodity::Pottery,
        Commodity::Weapons,
        Commodity::Furniture,
        Commodity::Armor,
        Commodity::Alchemy,
        Commodity::Machinery,
        Commodity::FeastGoods,
        Commodity::EliteGear,
        Commodity::Automaton,
        Commodity::Elixir,
        Commodity::LuxurySet,
    ];

    pub const RAW: [Commodity; 6] = [
        Commodity::Timber,
        Commodity::Ore,
        Commodity::Grain,
        Commodity::Fish,
        Commodity::Clay,
        Commodity::Herbs,
    ];

    pub fn tier(self) -> u8 {
        match self {
            Commodity::Timber
            | Commodity::Ore
            | Commodity::Grain
            | Commodity::Fish
            | Commodity::Clay
            | Commodity::Herbs => 0,
            Commodity::Tools
            | Commodity::Medicine
            | Commodity::Bricks
            | Commodity::Metalwork
            | Commodity::Provisions
            | Commodity::Pottery => 1,
            Commodity::Weapons
            | Commodity::Furniture
            | Commodity::Armor
            | Commodity::Alchemy
            | Commodity::Machinery
            | Commodity::FeastGoods => 2,
            Commodity::EliteGear
            | Commodity::Automaton
            | Commodity::Elixir
            | Commodity::LuxurySet => 3,
        }
    }

    /// NPC demand necessity weight.
    pub fn necessity_weight(self) -> f32 {
        match self {
            Commodity::Provisions => 3.0,
            Commodity::Grain => 2.0,
            Commodity::Tools => 1.5,
            Commodity::Medicine => 1.5,
            Commodity::Bricks => 1.0,
            _ => 0.5,
        }
    }
}

impl fmt::Display for Commodity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// ── Season ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub fn next(self) -> Season {
        match self {
            Season::Spring => Season::Summer,
            Season::Summer => Season::Autumn,
            Season::Autumn => Season::Winter,
            Season::Winter => Season::Spring,
        }
    }

    /// Global travel speed modifier for this season.
    pub fn travel_speed_modifier(self) -> f32 {
        match self {
            Season::Winter => 0.7,
            _ => 1.0,
        }
    }

    /// Food consumption multiplier.
    pub fn food_consumption_modifier(self) -> f32 {
        match self {
            Season::Winter => 1.5,
            _ => 1.0,
        }
    }

    /// Bandit activity multiplier.
    pub fn bandit_activity_modifier(self) -> f32 {
        match self {
            Season::Summer => 1.3,
            Season::Winter => 0.5,
            _ => 1.0,
        }
    }
}

impl fmt::Display for Season {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// ── Terrain ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerrainType {
    Plains,
    Forest,
    Hills,
    Mountains,
    Water,
    Coast,
}

impl TerrainType {
    /// Movement speed multiplier (on foot). Mountains and Water are impassable.
    pub fn speed_multiplier(self) -> f32 {
        match self {
            TerrainType::Plains => 1.0,
            TerrainType::Forest => 0.6,
            TerrainType::Hills => 0.4,
            TerrainType::Mountains => 0.0, // impassable
            TerrainType::Water => 0.0,     // impassable on foot
            TerrainType::Coast => 0.8,
        }
    }

    pub fn is_passable(self) -> bool {
        !matches!(self, TerrainType::Mountains | TerrainType::Water)
    }

    /// Classify from a Perlin height value in [0, 1] and whether the cell
    /// is adjacent to land (for coast detection). `sea_level` is typically 0.25.
    pub fn from_height(height: f32, sea_level: f32, adjacent_to_land: bool) -> Self {
        if height < sea_level {
            if adjacent_to_land {
                TerrainType::Coast
            } else {
                TerrainType::Water
            }
        } else if height < 0.3 {
            TerrainType::Plains
        } else if height < 0.5 {
            TerrainType::Forest
        } else if height < 0.7 {
            TerrainType::Hills
        } else {
            TerrainType::Mountains
        }
    }
}

// ── Profession ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Profession {
    Trader,
    Miner,
    Farmer,
    Craftsman,
    Soldier,
    Shipwright,
    Idle,
}

impl Profession {
    pub const ALL: [Profession; 7] = [
        Profession::Trader,
        Profession::Miner,
        Profession::Farmer,
        Profession::Craftsman,
        Profession::Soldier,
        Profession::Shipwright,
        Profession::Idle,
    ];
}

impl fmt::Display for Profession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// ── Market Side ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

// ── Reputation Channel ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReputationChannel {
    Profit,
    Demand,
    Danger,
    Opportunity,
}

impl ReputationChannel {
    pub const ALL: [ReputationChannel; 4] = [
        ReputationChannel::Profit,
        ReputationChannel::Demand,
        ReputationChannel::Danger,
        ReputationChannel::Opportunity,
    ];
}

// ── Agent State (FSM) ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentState {
    // Trader
    Scouting,
    Buying,
    Transporting,
    Selling,
    Resting,
    Fleeing,
    // Miner / Farmer
    TravelingToNode,
    Extracting,
    TravelingToCity,
    // Craftsman
    BuyingMaterials,
    Crafting,
    SellingGoods,
    // Soldier
    Patrolling,
    Escorting,
    Fighting,
    // Shipwright
    Loading,
    Sailing,
    Unloading,
    // Shared
    Idle,
}

// ── Vec2 wrapper ───────────────────────────────────────────────────────────
// macroquad provides its own Vec2, but we define a lightweight version for
// use in lib code that doesn't depend on macroquad.

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn normalized(self) -> Self {
        let len = self.length();
        if len < 1e-10 {
            Self::ZERO
        } else {
            Self {
                x: self.x / len,
                y: self.y / len,
            }
        }
    }

    pub fn dot(self, other: Vec2) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub fn distance(self, other: Vec2) -> f32 {
        (self - other).length()
    }

    pub fn angle(self) -> f32 {
        self.y.atan2(self.x)
    }

    pub fn from_angle(radians: f32) -> Self {
        Self {
            x: radians.cos(),
            y: radians.sin(),
        }
    }

    pub fn lerp(self, other: Vec2, t: f32) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2 {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl std::ops::AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Vec2) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, rhs: Vec2) -> Vec2 {
        Vec2 {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, rhs: f32) -> Vec2 {
        Vec2 {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl std::ops::Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2 {
            x: -self.x,
            y: -self.y,
        }
    }
}

// ── CityId ─────────────────────────────────────────────────────────────────

pub type CityId = u32;

// ── Market Action ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MarketAction {
    None,
    Buy {
        commodity: Commodity,
        max_price: f32,
        quantity: f32,
    },
    Sell {
        commodity: Commodity,
        min_price: f32,
        quantity: f32,
    },
}

impl Default for MarketAction {
    fn default() -> Self {
        MarketAction::None
    }
}

// ── Inventory helper type ──────────────────────────────────────────────────

pub type Inventory = HashMap<Commodity, f32>;

// ── Merchant Traits ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MerchantTraits {
    /// 0–1: low = avoids DANGER zones, high = ignores.
    pub risk_tolerance: f32,
    /// 0–1: high = chases bigger margins, low = takes safe trades.
    pub greed: f32,
    /// 0–1: high = forms caravans easily, shares gossip more.
    pub sociability: f32,
    /// 0–1: high = stays with home city, low = migrates freely.
    pub loyalty: f32,
}

impl Default for MerchantTraits {
    fn default() -> Self {
        Self {
            risk_tolerance: 0.5,
            greed: 0.5,
            sociability: 0.5,
            loyalty: 0.5,
        }
    }
}

// ── Recipe ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub inputs: Vec<(Commodity, f32)>,
    pub output: Commodity,
    pub output_quantity: f32,
    pub craft_ticks: u32,
    pub tier: u8,
    pub requires_workshop: bool,
}

// ── Order ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub agent_id: u32,
    pub commodity: Commodity,
    pub side: Side,
    pub price: f32,
    pub quantity: f32,
    pub tick_placed: u32,
    pub ttl: u32,
}

// ── Transaction ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub tick: u32,
    pub commodity: Commodity,
    pub price: f32,
    pub quantity: f32,
    pub buyer_id: u32,
    pub seller_id: u32,
    pub city_id: CityId,
}

// ── City Upgrade ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CityUpgrade {
    MarketHall,
    Walls,
    Harbor,
    Workshop,
}

impl CityUpgrade {
    pub const ALL: [CityUpgrade; 4] = [
        CityUpgrade::MarketHall,
        CityUpgrade::Walls,
        CityUpgrade::Harbor,
        CityUpgrade::Workshop,
    ];
}

// ── Terrain Ray (sensory) ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TerrainRay {
    pub distance: f32,
    pub terrain_type: TerrainType,
    pub road_value: f32,
}

// ── Neighbor Info (sensory) ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NeighborInfo {
    pub relative_pos: Vec2,
    pub profession: Profession,
    pub inventory_fullness: f32,
    pub reputation: f32,
    pub caravan_id: Option<u32>,
}
