use rand::Rng;

use crate::config::WorldConfig;
use crate::types::{Commodity, Season, TerrainType, Vec2};

/// Ticks required to extract one unit of resource.
pub const TICKS_PER_UNIT: u32 = 3;

/// Depletion added per unit extracted.
const DEPLETION_PER_UNIT: f32 = 0.02;

/// Regeneration rate per tick when not being harvested (0.05%).
const REGEN_RATE: f32 = 0.0005;

/// Ticks an exhausted node must rest before recovering.
const EXHAUSTION_RECOVERY_TICKS: u32 = 2000;

// ── ResourceNode ─────────────────────────────────────────────────────────────

pub struct ResourceNode {
    pub id: u32,
    pub position: Vec2,
    pub commodity: Commodity,
    /// Units yielded per extraction (1.0–5.0).
    pub base_yield: f32,
    /// 0.0 (fresh) to 1.0 (exhausted).
    pub depletion: f32,
    /// Set to `true` during any tick in which this node is being harvested.
    pub currently_harvested: bool,
    /// Consecutive idle ticks while fully exhausted (depletion >= 1.0).
    ticks_exhausted_idle: u32,
}

impl ResourceNode {
    pub fn new(id: u32, position: Vec2, commodity: Commodity, base_yield: f32) -> Self {
        debug_assert!(
            (1.0..=5.0).contains(&base_yield),
            "base_yield must be in [1, 5], got {base_yield}"
        );
        Self {
            id,
            position,
            commodity,
            base_yield,
            depletion: 0.0,
            currently_harvested: false,
            ticks_exhausted_idle: 0,
        }
    }

    /// Create a resource node at a specific position with default yield.
    pub fn new_at(id: u32, position: Vec2, commodity: Commodity) -> Self {
        Self {
            id,
            position,
            commodity,
            base_yield: 3.0,
            depletion: 0.0,
            currently_harvested: false,
            ticks_exhausted_idle: 0,
        }
    }

    // ── Extraction ───────────────────────────────────────────────────────────

    /// Perform one extraction. Returns the yield adjusted for depletion and
    /// season. Sets `currently_harvested` and advances depletion.
    ///
    /// `map_height` is the world height — needed for the CLAY/winter rule
    /// (CLAY yields 0 in winter when the node is in the northern 30%).
    ///
    /// Each extracted unit costs [`TICKS_PER_UNIT`] ticks to the harvester.
    pub fn extract(&mut self, season: Season, map_height: f32) -> f32 {
        self.currently_harvested = true;
        self.ticks_exhausted_idle = 0;

        if self.depletion >= 1.0 {
            return 0.0;
        }

        let depletion_mult = self.depletion_multiplier();
        let seasonal_mult = self.seasonal_modifier(season, map_height);
        let yield_amount = self.base_yield * depletion_mult * seasonal_mult;

        self.depletion = (self.depletion + yield_amount * DEPLETION_PER_UNIT).min(1.0);

        yield_amount
    }

    /// Piecewise depletion → yield multiplier.
    ///
    /// - `[0.0, 0.8)` depletion → `[1.0, 0.5)` yield (linear)
    /// - `[0.8, 1.0]` depletion → `[0.5, 0.0]` yield (linear)
    fn depletion_multiplier(&self) -> f32 {
        if self.depletion >= 1.0 {
            0.0
        } else if self.depletion >= 0.8 {
            2.5 * (1.0 - self.depletion)
        } else {
            1.0 - 0.625 * self.depletion
        }
    }

    /// Seasonal yield modifier for this node's commodity.
    fn seasonal_modifier(&self, season: Season, map_height: f32) -> f32 {
        match self.commodity {
            Commodity::Grain | Commodity::Herbs => match season {
                Season::Summer => 2.0,
                Season::Winter => 0.3,
                _ => 1.0,
            },
            Commodity::Fish => match season {
                Season::Spring => 1.5,
                Season::Autumn => 0.5,
                _ => 1.0,
            },
            Commodity::Clay => {
                if season == Season::Winter && self.position.y < map_height * 0.3 {
                    0.0
                } else {
                    1.0
                }
            }
            // Timber, Ore — unaffected by season.
            _ => 1.0,
        }
    }

    // ── Regeneration ─────────────────────────────────────────────────────────

    /// Call once per tick after all extractions for this tick are done.
    ///
    /// - If the node was harvested this tick, clears the flag and resets the
    ///   exhaustion idle counter.
    /// - Otherwise, regenerates depletion at 0.05 %/tick.
    /// - Fully exhausted nodes (depletion ≥ 1.0) snap to 80 % depletion
    ///   (20 % capacity) after 2 000 consecutive idle ticks.
    pub fn tick_regeneration(&mut self) {
        if self.currently_harvested {
            self.currently_harvested = false;
            self.ticks_exhausted_idle = 0;
            return;
        }

        if self.depletion >= 1.0 {
            self.ticks_exhausted_idle += 1;
            if self.ticks_exhausted_idle >= EXHAUSTION_RECOVERY_TICKS {
                self.depletion = 0.8;
                self.ticks_exhausted_idle = 0;
            }
        } else if self.depletion > 0.0 {
            self.depletion = (self.depletion - REGEN_RATE).max(0.0);
        }
    }

    // ── Placement ────────────────────────────────────────────────────────────

    /// Scatter-place resource nodes across the map, avoiding mountains and
    /// water. Guarantees at least one node per raw commodity type. Enforces a
    /// minimum of 6 nodes regardless of `count`.
    pub fn scatter(
        count: u32,
        world_w: f32,
        world_h: f32,
        terrain_at: impl Fn(Vec2) -> TerrainType,
        rng: &mut impl Rng,
    ) -> Vec<ResourceNode> {
        let count = count.max(6) as usize;
        let mut nodes = Vec::with_capacity(count);
        let max_attempts = 2000;

        // Guarantee one of each raw commodity.
        for (i, &commodity) in Commodity::RAW.iter().enumerate() {
            if let Some(pos) =
                Self::find_valid_position(world_w, world_h, &terrain_at, rng, max_attempts)
            {
                let base_yield = rng.gen_range(1.0_f32..=5.0);
                nodes.push(ResourceNode::new(i as u32, pos, commodity, base_yield));
            }
        }

        // Fill remaining slots with random raw commodities.
        for i in nodes.len()..count {
            if let Some(pos) =
                Self::find_valid_position(world_w, world_h, &terrain_at, rng, max_attempts)
            {
                let commodity = Commodity::RAW[rng.gen_range(0..Commodity::RAW.len())];
                let base_yield = rng.gen_range(1.0_f32..=5.0);
                nodes.push(ResourceNode::new(i as u32, pos, commodity, base_yield));
            }
        }

        nodes
    }

    /// Rejection-sample a passable position (not Mountains, not Water).
    fn find_valid_position(
        world_w: f32,
        world_h: f32,
        terrain_at: &impl Fn(Vec2) -> TerrainType,
        rng: &mut impl Rng,
        max_attempts: u32,
    ) -> Option<Vec2> {
        for _ in 0..max_attempts {
            let candidate = Vec2::new(rng.gen_range(0.0..world_w), rng.gen_range(0.0..world_h));
            if terrain_at(candidate).is_passable() {
                return Some(candidate);
            }
        }
        None
    }

    /// Convenience: generate nodes from world config.
    pub fn generate(
        config: &WorldConfig,
        terrain_at: impl Fn(Vec2) -> TerrainType,
        rng: &mut impl Rng,
    ) -> Vec<ResourceNode> {
        Self::scatter(
            config.num_resource_nodes,
            config.width as f32,
            config.height as f32,
            terrain_at,
            rng,
        )
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(commodity: Commodity) -> ResourceNode {
        ResourceNode::new(0, Vec2::new(50.0, 50.0), commodity, 3.0)
    }

    fn make_node_at(commodity: Commodity, pos: Vec2) -> ResourceNode {
        ResourceNode::new(0, pos, commodity, 3.0)
    }

    // ── Yield / depletion ────────────────────────────────────────────────

    #[test]
    fn fresh_node_full_yield() {
        let mut node = make_node(Commodity::Timber);
        let y = node.extract(Season::Spring, 100.0);
        assert!((y - 3.0).abs() < 1e-5);
    }

    #[test]
    fn exhausted_node_yields_zero() {
        let mut node = make_node(Commodity::Timber);
        node.depletion = 1.0;
        assert_eq!(node.extract(Season::Spring, 100.0), 0.0);
    }

    #[test]
    fn depletion_at_80_pct_halves_yield() {
        let mut node = make_node(Commodity::Timber);
        node.depletion = 0.8;
        // depletion_mult = 2.5 * 0.2 = 0.5  →  yield = 3.0 * 0.5 = 1.5
        let y = node.extract(Season::Spring, 100.0);
        assert!((y - 1.5).abs() < 0.05);
    }

    #[test]
    fn extraction_increases_depletion() {
        let mut node = make_node(Commodity::Timber);
        let before = node.depletion;
        node.extract(Season::Spring, 100.0);
        assert!(node.depletion > before);
    }

    #[test]
    fn extraction_sets_harvested_flag() {
        let mut node = make_node(Commodity::Timber);
        assert!(!node.currently_harvested);
        node.extract(Season::Spring, 100.0);
        assert!(node.currently_harvested);
    }

    #[test]
    fn depletion_clamped_to_one() {
        let mut node = make_node(Commodity::Grain);
        node.depletion = 0.99;
        node.extract(Season::Summer, 100.0);
        assert!(node.depletion <= 1.0);
    }

    // ── Seasonal modifiers ───────────────────────────────────────────────

    #[test]
    fn grain_summer_double() {
        let mut node = make_node(Commodity::Grain);
        assert!((node.extract(Season::Summer, 100.0) - 6.0).abs() < 1e-5);
    }

    #[test]
    fn grain_winter_reduced() {
        let mut node = make_node(Commodity::Grain);
        assert!((node.extract(Season::Winter, 100.0) - 0.9).abs() < 1e-5);
    }

    #[test]
    fn herbs_summer_double() {
        let mut node = make_node(Commodity::Herbs);
        assert!((node.extract(Season::Summer, 100.0) - 6.0).abs() < 1e-5);
    }

    #[test]
    fn herbs_winter_reduced() {
        let mut node = make_node(Commodity::Herbs);
        assert!((node.extract(Season::Winter, 100.0) - 0.9).abs() < 1e-5);
    }

    #[test]
    fn fish_spring_bonus() {
        let mut node = make_node(Commodity::Fish);
        assert!((node.extract(Season::Spring, 100.0) - 4.5).abs() < 1e-5);
    }

    #[test]
    fn fish_autumn_penalty() {
        let mut node = make_node(Commodity::Fish);
        assert!((node.extract(Season::Autumn, 100.0) - 1.5).abs() < 1e-5);
    }

    #[test]
    fn clay_winter_northern_zero() {
        let mut node = make_node_at(Commodity::Clay, Vec2::new(50.0, 10.0));
        assert_eq!(node.extract(Season::Winter, 100.0), 0.0);
    }

    #[test]
    fn clay_winter_southern_unaffected() {
        let mut node = make_node_at(Commodity::Clay, Vec2::new(50.0, 50.0));
        assert!((node.extract(Season::Winter, 100.0) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn clay_summer_unaffected() {
        let mut node = make_node_at(Commodity::Clay, Vec2::new(50.0, 10.0));
        assert!((node.extract(Season::Summer, 100.0) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn timber_ore_unaffected_by_season() {
        for commodity in [Commodity::Timber, Commodity::Ore] {
            for season in [Season::Spring, Season::Summer, Season::Autumn, Season::Winter] {
                let mut node = make_node(commodity);
                let y = node.extract(season, 100.0);
                assert!(
                    (y - 3.0).abs() < 1e-5,
                    "{commodity:?} should be unaffected in {season:?}"
                );
            }
        }
    }

    // ── Regeneration ─────────────────────────────────────────────────────

    #[test]
    fn regen_when_not_harvested() {
        let mut node = make_node(Commodity::Timber);
        node.depletion = 0.5;
        node.tick_regeneration();
        assert!((node.depletion - (0.5 - REGEN_RATE)).abs() < 1e-6);
    }

    #[test]
    fn no_regen_when_harvested() {
        let mut node = make_node(Commodity::Timber);
        node.depletion = 0.5;
        node.currently_harvested = true;
        node.tick_regeneration();
        assert!((node.depletion - 0.5).abs() < 1e-6);
        assert!(!node.currently_harvested);
    }

    #[test]
    fn regen_does_not_go_below_zero() {
        let mut node = make_node(Commodity::Timber);
        node.depletion = 0.0001;
        node.tick_regeneration();
        assert!(node.depletion >= 0.0);
    }

    #[test]
    fn exhaustion_recovery_after_2000_ticks() {
        let mut node = make_node(Commodity::Timber);
        node.depletion = 1.0;
        for _ in 0..EXHAUSTION_RECOVERY_TICKS {
            node.tick_regeneration();
        }
        assert!(
            (node.depletion - 0.8).abs() < 1e-5,
            "should recover to 80% depletion (20% capacity)"
        );
    }

    #[test]
    fn exhaustion_no_early_recovery() {
        let mut node = make_node(Commodity::Timber);
        node.depletion = 1.0;
        for _ in 0..EXHAUSTION_RECOVERY_TICKS - 1 {
            node.tick_regeneration();
        }
        assert!((node.depletion - 1.0).abs() < 1e-5);
    }

    #[test]
    fn exhaustion_counter_resets_on_harvest() {
        let mut node = make_node(Commodity::Timber);
        node.depletion = 1.0;
        // Idle for 1000 ticks.
        for _ in 0..1000 {
            node.tick_regeneration();
        }
        // Extraction resets idle counter.
        node.extract(Season::Spring, 100.0);
        node.tick_regeneration(); // clears currently_harvested
        // Another 1999 idle ticks — not enough after reset.
        for _ in 0..EXHAUSTION_RECOVERY_TICKS - 1 {
            node.tick_regeneration();
        }
        assert!((node.depletion - 1.0).abs() < 1e-5);
    }

    // ── Scatter placement ────────────────────────────────────────────────

    #[test]
    fn scatter_generates_correct_count() {
        let mut rng = rand::thread_rng();
        let nodes =
            ResourceNode::scatter(25, 200.0, 200.0, |_| TerrainType::Plains, &mut rng);
        assert_eq!(nodes.len(), 25);
    }

    #[test]
    fn scatter_at_least_one_of_each_commodity() {
        let mut rng = rand::thread_rng();
        let nodes =
            ResourceNode::scatter(25, 200.0, 200.0, |_| TerrainType::Plains, &mut rng);
        for &commodity in &Commodity::RAW {
            assert!(
                nodes.iter().any(|n| n.commodity == commodity),
                "missing {commodity:?}"
            );
        }
    }

    #[test]
    fn scatter_avoids_mountains_and_water() {
        let mut rng = rand::thread_rng();
        let nodes = ResourceNode::scatter(
            25,
            200.0,
            200.0,
            |pos| {
                if pos.x < 50.0 {
                    TerrainType::Mountains
                } else if pos.x < 100.0 {
                    TerrainType::Water
                } else {
                    TerrainType::Plains
                }
            },
            &mut rng,
        );
        for node in &nodes {
            assert!(
                node.position.x >= 100.0,
                "node placed on impassable terrain at x={}",
                node.position.x
            );
        }
    }

    #[test]
    fn scatter_enforces_minimum_six() {
        let mut rng = rand::thread_rng();
        let nodes =
            ResourceNode::scatter(3, 200.0, 200.0, |_| TerrainType::Plains, &mut rng);
        assert!(nodes.len() >= 6);
    }

    #[test]
    fn scatter_base_yield_in_range() {
        let mut rng = rand::thread_rng();
        let nodes =
            ResourceNode::scatter(25, 200.0, 200.0, |_| TerrainType::Plains, &mut rng);
        for node in &nodes {
            assert!(
                (1.0..=5.0).contains(&node.base_yield),
                "base_yield {} out of range",
                node.base_yield
            );
        }
    }

    #[test]
    fn ticks_per_unit_is_three() {
        assert_eq!(TICKS_PER_UNIT, 3);
    }
}
