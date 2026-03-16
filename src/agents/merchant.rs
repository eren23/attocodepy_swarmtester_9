use std::collections::{HashMap, VecDeque};

use rand::Rng;

use crate::config::MerchantConfig;
use crate::types::{
    AgentState, CityId, Commodity, Inventory, MerchantTraits, Profession, Season, Transaction,
    Vec2,
};
use crate::world::city::City;
use crate::world::road::RoadGrid;
use crate::world::terrain::Terrain;

use super::actions::MerchantAction;

// ── Price Memory ────────────────────────────────────────────────────────────

/// A single remembered price observation.
#[derive(Debug, Clone, Copy)]
pub struct PriceEntry {
    pub price: f32,
    /// Tick at which the price was observed.
    pub observed_tick: u32,
}

/// Per-city, per-commodity last-known prices with a TTL.
#[derive(Debug, Clone)]
pub struct PriceMemory {
    /// city_id → commodity → entry
    entries: HashMap<CityId, HashMap<Commodity, PriceEntry>>,
    /// Number of ticks before an entry is considered stale.
    ttl: u32,
}

impl PriceMemory {
    pub fn new(ttl: u32) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
        }
    }

    /// Record a price observation.
    pub fn record(&mut self, city_id: CityId, commodity: Commodity, price: f32, tick: u32) {
        self.entries
            .entry(city_id)
            .or_default()
            .insert(commodity, PriceEntry { price, observed_tick: tick });
    }

    /// Look up a remembered price, returning `None` if missing or expired.
    pub fn get(&self, city_id: CityId, commodity: Commodity, current_tick: u32) -> Option<f32> {
        let entry = self.entries.get(&city_id)?.get(&commodity)?;
        if current_tick.saturating_sub(entry.observed_tick) > self.ttl {
            None
        } else {
            Some(entry.price)
        }
    }

    /// Merge another merchant's price memory into ours (gossip / caravan).
    /// Only replaces entries that are newer than what we already have.
    pub fn merge(&mut self, other: &PriceMemory) {
        for (&city_id, commodities) in &other.entries {
            let local = self.entries.entry(city_id).or_default();
            for (&commodity, &entry) in commodities {
                let dominated = local
                    .get(&commodity)
                    .map_or(true, |e| e.observed_tick < entry.observed_tick);
                if dominated {
                    local.insert(commodity, entry);
                }
            }
        }
    }

    /// Remove all expired entries for a given tick.
    pub fn prune(&mut self, current_tick: u32) {
        for commodities in self.entries.values_mut() {
            commodities.retain(|_, e| current_tick.saturating_sub(e.observed_tick) <= self.ttl);
        }
        self.entries.retain(|_, m| !m.is_empty());
    }

    /// Get all entries for a city (regardless of staleness).
    pub fn city_prices(&self, city_id: CityId) -> Option<&HashMap<Commodity, PriceEntry>> {
        self.entries.get(&city_id)
    }

    /// Iterate all city entries.
    pub fn all_entries(&self) -> &HashMap<CityId, HashMap<Commodity, PriceEntry>> {
        &self.entries
    }
}

// ── Merchant ────────────────────────────────────────────────────────────────

/// Maximum number of transactions kept in the ledger.
const LEDGER_CAP: usize = 50;

/// Number of ticks with no significant movement before a merchant is considered stuck.
const STUCK_THRESHOLD: u32 = 10;
/// Distance threshold for considering a merchant "at" a waypoint (in tiles).
const WAYPOINT_ARRIVAL_DIST: f32 = 1.5;

#[derive(Debug, Clone)]
pub struct Merchant {
    pub id: u32,
    pub pos: Vec2,
    pub heading: f32,
    pub speed: f32,
    pub gold: f32,
    pub inventory: Inventory,
    pub max_carry: f32,
    pub profession: Profession,
    pub reputation: f32,
    pub fatigue: f32,
    pub alive: bool,
    pub age: u32,
    pub home_city: CityId,
    pub state: AgentState,
    pub price_memory: PriceMemory,
    pub ledger: VecDeque<Transaction>,
    pub caravan_id: Option<u32>,
    pub traits: MerchantTraits,

    /// A* waypoints toward the current destination city (grid coords).
    pub waypoints: VecDeque<(u32, u32)>,
    /// Which city the current waypoints lead to.
    pub waypoint_target: Option<CityId>,
    /// Consecutive ticks with negligible movement (stuck detection).
    stuck_ticks: u32,
    /// Position at the previous tick for stuck detection.
    prev_pos: Vec2,

    /// Consecutive ticks with gold < 0 (for bankruptcy detection).
    negative_gold_ticks: u32,
}

impl Merchant {
    /// Spawn a new merchant at `pos`, assigned to `home_city`.
    pub fn new(
        id: u32,
        pos: Vec2,
        home_city: CityId,
        profession: Profession,
        config: &MerchantConfig,
        rng: &mut impl Rng,
    ) -> Self {
        let max_carry = if profession == Profession::Shipwright {
            config.max_carry * config.shipwright_carry_mult
        } else {
            config.max_carry
        };

        Self {
            id,
            pos,
            heading: rng.gen_range(0.0..std::f32::consts::TAU),
            speed: config.base_speed,
            gold: config.initial_gold,
            inventory: HashMap::new(),
            max_carry,
            profession,
            reputation: 50.0,
            fatigue: 0.0,
            alive: true,
            age: 0,
            home_city,
            state: AgentState::Idle,
            price_memory: PriceMemory::new(config.price_memory_ttl),
            ledger: VecDeque::with_capacity(LEDGER_CAP),
            caravan_id: None,
            traits: MerchantTraits::random(rng),
            waypoints: VecDeque::new(),
            waypoint_target: None,
            stuck_ticks: 0,
            prev_pos: pos,
            negative_gold_ticks: 0,
        }
    }

    // ── Inventory helpers ───────────────────────────────────────────────────

    /// Total weight of all carried commodities.
    pub fn inventory_weight(&self) -> f32 {
        self.inventory.values().sum()
    }

    /// Ratio of current inventory weight to max_carry in [0, 1].
    pub fn inventory_fill_ratio(&self) -> f32 {
        if self.max_carry <= 0.0 {
            return 1.0;
        }
        (self.inventory_weight() / self.max_carry).min(1.0)
    }

    /// Add commodity to inventory. Returns the amount actually added
    /// (may be less if at capacity).
    pub fn add_to_inventory(&mut self, commodity: Commodity, quantity: f32) -> f32 {
        let space = (self.max_carry - self.inventory_weight()).max(0.0);
        let added = quantity.min(space);
        if added > 0.0 {
            *self.inventory.entry(commodity).or_insert(0.0) += added;
        }
        added
    }

    /// Remove commodity from inventory. Returns amount actually removed.
    pub fn remove_from_inventory(&mut self, commodity: Commodity, quantity: f32) -> f32 {
        let entry = self.inventory.entry(commodity).or_insert(0.0);
        let removed = quantity.min(*entry);
        *entry -= removed;
        if *entry < 1e-6 {
            self.inventory.remove(&commodity);
        }
        removed
    }

    /// Record a transaction in the ledger, evicting the oldest if at capacity.
    pub fn record_transaction(&mut self, tx: Transaction) {
        if self.ledger.len() >= LEDGER_CAP {
            self.ledger.pop_front();
        }
        self.ledger.push_back(tx);
    }

    // ── Waypoint navigation ───────────────────────────────────────────────

    /// Replace the current waypoint queue with a new A* path toward `target`.
    pub fn set_waypoints(&mut self, target: CityId, path: Vec<(u32, u32)>) {
        self.waypoints = path.into();
        self.waypoint_target = Some(target);
        self.stuck_ticks = 0;
    }

    /// Discard all waypoints and reset navigation state.
    pub fn clear_waypoints(&mut self) {
        self.waypoints.clear();
        self.waypoint_target = None;
        self.stuck_ticks = 0;
    }

    /// Peek at the next waypoint without consuming it.
    pub fn next_waypoint(&self) -> Option<(u32, u32)> {
        self.waypoints.front().copied()
    }

    /// Pop waypoints that are within `WAYPOINT_ARRIVAL_DIST` tiles of `pos`.
    /// Returns the new "next waypoint" direction as a world-space Vec2, if any remain.
    pub fn advance_waypoints(&mut self) -> Option<Vec2> {
        while let Some(&(wx, wy)) = self.waypoints.front() {
            let wp = Vec2::new(wx as f32 + 0.5, wy as f32 + 0.5);
            if self.pos.distance(wp) <= WAYPOINT_ARRIVAL_DIST {
                self.waypoints.pop_front();
            } else {
                let delta = wp - self.pos;
                return Some(delta.normalized());
            }
        }
        None
    }

    /// Update stuck detection. Call once per tick after movement.
    pub fn update_stuck_detection(&mut self) {
        if self.pos.distance(self.prev_pos) < 0.3 {
            self.stuck_ticks += 1;
        } else {
            self.stuck_ticks = 0;
        }
        self.prev_pos = self.pos;
    }

    /// Returns `true` if the merchant has been stuck for too long and needs
    /// a path recompute.
    pub fn is_stuck(&self) -> bool {
        self.stuck_ticks >= STUCK_THRESHOLD
    }

    /// Whether waypoints should be recomputed (stuck, empty, or target changed).
    pub fn needs_path_recompute(&self, desired_target: Option<CityId>) -> bool {
        if self.waypoints.is_empty() && desired_target.is_some() {
            return true;
        }
        if desired_target != self.waypoint_target {
            return true;
        }
        self.is_stuck()
    }

    // ── Physics / Movement ──────────────────────────────────────────────────

    /// fatigue_mult = max(0.3, 1.0 - fatigue / 200.0)
    pub fn fatigue_mult(&self) -> f32 {
        (1.0 - self.fatigue / 200.0).max(0.3)
    }

    /// Apply a `MerchantAction` to update heading, position, fatigue, and
    /// handle collisions. Returns any reputation deposit to be applied.
    ///
    /// Call order per tick:
    /// 1. brain produces `MerchantAction`
    /// 2. `apply_action` updates heading + position
    /// 3. caller applies reputation deposit, market orders, etc.
    pub fn apply_action(
        &mut self,
        action: &MerchantAction,
        terrain: &Terrain,
        roads: &RoadGrid,
        season: Season,
        world_width: f32,
        world_height: f32,
    ) {
        if !self.alive {
            return;
        }

        let mut action = action.clone();
        action.sanitize();

        // 1. Update heading
        self.heading += action.turn;
        // Normalise heading to [0, 2π)
        self.heading = self.heading.rem_euclid(std::f32::consts::TAU);

        // 2. Compute effective speed
        let terrain_x = (self.pos.x as u32).min(terrain.width().saturating_sub(1));
        let terrain_y = (self.pos.y as u32).min(terrain.height().saturating_sub(1));
        let terrain_mult = terrain.speed_at(terrain_x, terrain_y, season);
        let road_mult = roads.speed_multiplier(self.pos);
        let season_mult = season.travel_speed_modifier();
        let fatigue_mult = self.fatigue_mult();

        let effective_speed = self.speed
            * action.speed_mult
            * terrain_mult
            * road_mult
            * fatigue_mult
            * season_mult;

        // 3. Compute candidate position
        let dir = Vec2::from_angle(self.heading);
        let candidate = self.pos + dir * effective_speed;

        // 4. World-bounds reflection
        let (new_pos, new_heading) =
            Self::reflect_at_bounds(candidate, self.heading, world_width, world_height);

        // 5. Terrain collision (mountains / water)
        let final_pos =
            Self::resolve_terrain_collision(self.pos, new_pos, terrain);

        self.pos = final_pos;
        self.heading = new_heading;

        // 6. Stuck detection (before fatigue, after final position is set).
        self.update_stuck_detection();

        // 7. Fatigue cost
        let fatigue_cost = 0.03
            + 0.02 * action.speed_mult
            + 0.04 * (self.inventory_weight() / self.max_carry.max(1e-6));
        self.fatigue = (self.fatigue + fatigue_cost).min(100.0);

        // 8. Fatigue collapse
        if self.fatigue >= 100.0 {
            self.collapse();
        }

        self.age += 1;
    }

    /// Check if the merchant is within a city's radius.
    pub fn is_at_city(&self, city: &City) -> bool {
        self.pos.distance(city.position) <= city.radius
    }

    /// Recover fatigue while at a city (1.5/tick).
    pub fn recover_fatigue_at_city(&mut self) {
        self.fatigue = (self.fatigue - 1.5).max(0.0);
    }

    /// Tick bankruptcy counter. Returns `true` if the merchant goes bankrupt.
    pub fn tick_bankruptcy(&mut self, grace_ticks: u32) -> bool {
        if self.gold < 0.0 {
            self.negative_gold_ticks += 1;
        } else {
            self.negative_gold_ticks = 0;
        }
        if self.negative_gold_ticks >= grace_ticks {
            self.go_bankrupt();
            return true;
        }
        false
    }

    // ── Internal helpers ────────────────────────────────────────────────────

    /// Fatigue ≥ 100 → collapse: drop 15% inventory, fatigue → 80.
    fn collapse(&mut self) {
        let to_drop: Vec<(Commodity, f32)> = self
            .inventory
            .iter()
            .map(|(&c, &q)| (c, q * 0.15))
            .collect();
        for (commodity, drop_qty) in to_drop {
            self.remove_from_inventory(commodity, drop_qty);
        }
        self.fatigue = 80.0;
    }

    /// Gold < 0 for too long → bankrupt: remove from sim.
    fn go_bankrupt(&mut self) {
        self.alive = false;
        // All inventory is considered "dropped" — the caller should
        // deposit a DANGER signal and handle ground items.
        self.inventory.clear();
    }

    /// Reflect position and heading at world boundaries.
    fn reflect_at_bounds(
        pos: Vec2,
        heading: f32,
        w: f32,
        h: f32,
    ) -> (Vec2, f32) {
        let mut x = pos.x;
        let mut y = pos.y;
        let mut hd = heading;

        if x < 0.0 {
            x = -x;
            hd = std::f32::consts::PI - hd;
        } else if x >= w {
            x = 2.0 * w - x - 1.0;
            hd = std::f32::consts::PI - hd;
        }

        if y < 0.0 {
            y = -y;
            hd = -hd;
        } else if y >= h {
            y = 2.0 * h - y - 1.0;
            hd = -hd;
        }

        // Final clamp to stay in bounds
        x = x.clamp(0.0, w - 1.0);
        y = y.clamp(0.0, h - 1.0);

        hd = hd.rem_euclid(std::f32::consts::TAU);
        (Vec2::new(x, y), hd)
    }

    /// Slide along impassable terrain edges. If the candidate cell is
    /// impassable, try sliding along each axis independently. If both
    /// are blocked, stay in place.
    fn resolve_terrain_collision(
        old_pos: Vec2,
        new_pos: Vec2,
        terrain: &Terrain,
    ) -> Vec2 {
        let nx = (new_pos.x as u32).min(terrain.width().saturating_sub(1));
        let ny = (new_pos.y as u32).min(terrain.height().saturating_sub(1));

        if terrain.is_passable(nx, ny) {
            return new_pos;
        }

        // Try sliding along X only
        let slide_x = Vec2::new(new_pos.x, old_pos.y);
        let sx = (slide_x.x as u32).min(terrain.width().saturating_sub(1));
        let sy = (slide_x.y as u32).min(terrain.height().saturating_sub(1));
        if terrain.is_passable(sx, sy) {
            return slide_x;
        }

        // Try sliding along Y only
        let slide_y = Vec2::new(old_pos.x, new_pos.y);
        let sx2 = (slide_y.x as u32).min(terrain.width().saturating_sub(1));
        let sy2 = (slide_y.y as u32).min(terrain.height().saturating_sub(1));
        if terrain.is_passable(sx2, sy2) {
            return slide_y;
        }

        // Both blocked — stay put
        old_pos
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EconomyConfig;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn test_config() -> EconomyConfig {
        EconomyConfig::load("economy_config.toml").expect("test config")
    }

    fn make_merchant(config: &EconomyConfig) -> Merchant {
        let mut rng = StdRng::seed_from_u64(42);
        Merchant::new(
            1,
            Vec2::new(400.0, 300.0),
            0,
            Profession::Trader,
            &config.merchant,
            &mut rng,
        )
    }

    #[test]
    fn new_merchant_has_correct_defaults() {
        let cfg = test_config();
        let m = make_merchant(&cfg);
        assert!(m.alive);
        assert_eq!(m.gold, cfg.merchant.initial_gold);
        assert_eq!(m.fatigue, 0.0);
        assert_eq!(m.age, 0);
        assert!(m.inventory.is_empty());
        assert_eq!(m.state, AgentState::Idle);
        assert!(m.caravan_id.is_none());
        assert!(m.ledger.is_empty());
    }

    #[test]
    fn inventory_weight_tracking() {
        let cfg = test_config();
        let mut m = make_merchant(&cfg);
        m.add_to_inventory(Commodity::Ore, 3.0);
        m.add_to_inventory(Commodity::Timber, 2.0);
        assert!((m.inventory_weight() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn inventory_respects_max_carry() {
        let cfg = test_config();
        let mut m = make_merchant(&cfg);
        let added = m.add_to_inventory(Commodity::Ore, 100.0);
        assert!((added - m.max_carry).abs() < 1e-6);
        // Adding more should add nothing
        let added2 = m.add_to_inventory(Commodity::Timber, 5.0);
        assert!(added2 < 1e-6);
    }

    #[test]
    fn remove_from_inventory_cleans_up() {
        let cfg = test_config();
        let mut m = make_merchant(&cfg);
        m.add_to_inventory(Commodity::Grain, 5.0);
        let removed = m.remove_from_inventory(Commodity::Grain, 10.0);
        assert!((removed - 5.0).abs() < 1e-6);
        assert!(!m.inventory.contains_key(&Commodity::Grain));
    }

    #[test]
    fn fatigue_mult_formula() {
        let cfg = test_config();
        let mut m = make_merchant(&cfg);
        m.fatigue = 0.0;
        assert!((m.fatigue_mult() - 1.0).abs() < 1e-6);
        m.fatigue = 100.0;
        assert!((m.fatigue_mult() - 0.5).abs() < 1e-6);
        m.fatigue = 200.0; // extreme
        assert!((m.fatigue_mult() - 0.3).abs() < 1e-6, "should clamp at 0.3");
    }

    #[test]
    fn collapse_drops_15_percent() {
        let cfg = test_config();
        let mut m = make_merchant(&cfg);
        m.add_to_inventory(Commodity::Ore, 10.0);
        m.fatigue = 100.0;
        m.collapse();
        assert!((m.fatigue - 80.0).abs() < 1e-6);
        // 10 - 15% = 8.5
        let ore = *m.inventory.get(&Commodity::Ore).unwrap_or(&0.0);
        assert!((ore - 8.5).abs() < 0.1);
    }

    #[test]
    fn bankruptcy_after_grace_period() {
        let cfg = test_config();
        let mut m = make_merchant(&cfg);
        m.gold = -10.0;
        for _ in 0..199 {
            assert!(!m.tick_bankruptcy(200));
        }
        assert!(m.tick_bankruptcy(200));
        assert!(!m.alive);
    }

    #[test]
    fn bankruptcy_resets_when_gold_positive() {
        let cfg = test_config();
        let mut m = make_merchant(&cfg);
        m.gold = -1.0;
        for _ in 0..100 {
            m.tick_bankruptcy(200);
        }
        m.gold = 10.0;
        m.tick_bankruptcy(200);
        // Counter should have reset — going negative again needs full 200 ticks
        m.gold = -1.0;
        for _ in 0..199 {
            assert!(!m.tick_bankruptcy(200));
        }
        assert!(m.tick_bankruptcy(200));
    }

    #[test]
    fn ledger_caps_at_50() {
        let cfg = test_config();
        let mut m = make_merchant(&cfg);
        for i in 0..60 {
            m.record_transaction(Transaction {
                tick: i,
                commodity: Commodity::Ore,
                price: 10.0,
                quantity: 1.0,
                buyer_id: 0,
                seller_id: 1,
                city_id: 0,
            });
        }
        assert_eq!(m.ledger.len(), 50);
        // Oldest should be tick 10
        assert_eq!(m.ledger.front().unwrap().tick, 10);
    }

    #[test]
    fn price_memory_respects_ttl() {
        let mut pm = PriceMemory::new(100);
        pm.record(0, Commodity::Ore, 5.0, 10);
        assert_eq!(pm.get(0, Commodity::Ore, 50), Some(5.0));
        assert_eq!(pm.get(0, Commodity::Ore, 111), None); // expired
    }

    #[test]
    fn price_memory_merge_keeps_newer() {
        let mut pm1 = PriceMemory::new(1000);
        let mut pm2 = PriceMemory::new(1000);
        pm1.record(0, Commodity::Ore, 5.0, 100);
        pm2.record(0, Commodity::Ore, 8.0, 200);
        pm1.merge(&pm2);
        assert_eq!(pm1.get(0, Commodity::Ore, 300), Some(8.0));
    }

    #[test]
    fn reflect_at_bounds() {
        // Off the left edge
        let (pos, _) = Merchant::reflect_at_bounds(Vec2::new(-5.0, 50.0), 3.0, 100.0, 100.0);
        assert!(pos.x >= 0.0);
        // Off the bottom
        let (pos, _) = Merchant::reflect_at_bounds(Vec2::new(50.0, 105.0), 1.0, 100.0, 100.0);
        assert!(pos.y < 100.0);
    }

    #[test]
    fn shipwright_gets_extra_carry() {
        let cfg = test_config();
        let mut rng = StdRng::seed_from_u64(42);
        let m = Merchant::new(
            2,
            Vec2::new(100.0, 100.0),
            0,
            Profession::Shipwright,
            &cfg.merchant,
            &mut rng,
        );
        let expected = cfg.merchant.max_carry * cfg.merchant.shipwright_carry_mult;
        assert!((m.max_carry - expected).abs() < 1e-6);
    }

    #[test]
    fn inventory_fill_ratio_correct() {
        let cfg = test_config();
        let mut m = make_merchant(&cfg);
        assert!((m.inventory_fill_ratio()).abs() < 1e-6);
        m.add_to_inventory(Commodity::Ore, m.max_carry / 2.0);
        assert!((m.inventory_fill_ratio() - 0.5).abs() < 1e-2);
    }
}
