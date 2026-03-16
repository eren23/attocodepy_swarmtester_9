use std::collections::HashMap;

use crate::config::MerchantConfig;
use crate::types::{
    CityId, Commodity, NeighborInfo, ReputationChannel, Season, TerrainRay, TerrainType, Vec2,
};
use crate::world::city::City;
use crate::world::reputation::ReputationGrid;
use crate::world::road::RoadGrid;
use crate::world::terrain::Terrain;

use super::merchant::Merchant;

// ── SensoryInput ────────────────────────────────────────────────────────────

/// Everything a merchant can perceive in a single tick.
/// Built by `SensoryInputBuilder` from world state.
#[derive(Debug, Clone)]
pub struct SensoryInput {
    // Scanner cones: left/right, ±35° from heading, range 60px.
    // Each array holds [PROFIT, DEMAND, DANGER, OPPORTUNITY] averages.
    pub left_scanner: [f32; 4],
    pub right_scanner: [f32; 4],

    // Terrain raycasts: 5 rays, 120° arc, range 40px.
    pub terrain_rays: [TerrainRay; 5],

    // Nearby agents within 30px.
    pub neighbors: Vec<NeighborInfo>,

    // Navigation
    pub nearest_city: (Vec2, f32),
    pub home_city: (Vec2, f32),
    pub nearest_resource: Option<(Vec2, f32, Commodity)>,

    // Market intelligence
    pub profit_gradient: Vec2,
    pub danger_gradient: Vec2,

    // Self state
    pub gold: f32,
    pub fatigue: f32,
    pub inventory_fill_ratio: f32,
    pub inventory_breakdown: HashMap<Commodity, f32>,
    pub current_terrain: TerrainType,
    pub current_season: Season,
    pub reputation: f32,

    // Bandit proximity (within 80px)
    pub nearest_bandit: Option<(Vec2, f32)>,
}

/// Bandit position for sensory queries.
pub struct BanditInfo {
    pub pos: Vec2,
}

/// Resource node info for sensory queries.
pub struct ResourceNodeInfo {
    pub pos: Vec2,
    pub commodity: Commodity,
}

// ── Builder ─────────────────────────────────────────────────────────────────

/// Builds a `SensoryInput` for a merchant from the world state.
pub struct SensoryInputBuilder<'a> {
    merchant: &'a Merchant,
    config: &'a MerchantConfig,
    terrain: &'a Terrain,
    roads: &'a RoadGrid,
    reputation_grid: &'a ReputationGrid,
    cities: &'a [City],
    season: Season,
}

impl<'a> SensoryInputBuilder<'a> {
    pub fn new(
        merchant: &'a Merchant,
        config: &'a MerchantConfig,
        terrain: &'a Terrain,
        roads: &'a RoadGrid,
        reputation_grid: &'a ReputationGrid,
        cities: &'a [City],
        season: Season,
    ) -> Self {
        Self {
            merchant,
            config,
            terrain,
            roads,
            reputation_grid,
            cities,
            season,
        }
    }

    /// Build the full sensory input, given nearby merchants, bandits, and
    /// resource nodes.
    pub fn build(
        &self,
        other_merchants: &[&Merchant],
        bandits: &[BanditInfo],
        resource_nodes: &[ResourceNodeInfo],
    ) -> SensoryInput {
        let pos = self.merchant.pos;
        let heading = self.merchant.heading;

        // Scanner cones
        let (left_scanner, right_scanner) = self.build_scanner_cones(pos, heading);

        // Terrain raycasts
        let terrain_rays = self.build_terrain_rays(pos, heading);

        // Neighbors within neighbor_radius
        let neighbors = self.build_neighbors(pos, other_merchants);

        // Navigation
        let nearest_city = self.find_nearest_city(pos);
        let home_city = self.find_home_city(pos);
        let nearest_resource = self.find_nearest_resource(pos, resource_nodes);

        // Reputation gradients
        let profit_gradient = self.reputation_grid.gradient(ReputationChannel::Profit, pos);
        let danger_gradient = self.reputation_grid.gradient(ReputationChannel::Danger, pos);

        // Self state
        let tx = (pos.x as u32).min(self.terrain.width().saturating_sub(1));
        let ty = (pos.y as u32).min(self.terrain.height().saturating_sub(1));
        let current_terrain = self.terrain.terrain_at(tx, ty);

        // Bandit proximity (within 80px)
        let nearest_bandit = self.find_nearest_bandit(pos, bandits, 80.0);

        SensoryInput {
            left_scanner,
            right_scanner,
            terrain_rays,
            neighbors,
            nearest_city,
            home_city,
            nearest_resource,
            profit_gradient,
            danger_gradient,
            gold: self.merchant.gold,
            fatigue: self.merchant.fatigue,
            inventory_fill_ratio: self.merchant.inventory_fill_ratio(),
            inventory_breakdown: self.merchant.inventory.clone(),
            current_terrain,
            current_season: self.season,
            reputation: self.merchant.reputation,
            nearest_bandit,
        }
    }

    // ── Scanner cones ───────────────────────────────────────────────────────

    /// Sample left/right reputation cones for all 4 channels.
    fn build_scanner_cones(
        &self,
        pos: Vec2,
        heading: f32,
    ) -> ([f32; 4], [f32; 4]) {
        let half_angle = self.config.scanner_angle_deg.to_radians();
        let range = self.config.scanner_range;

        let mut left = [0.0f32; 4];
        let mut right = [0.0f32; 4];

        for (i, &channel) in ReputationChannel::ALL.iter().enumerate() {
            let (l, r) = self.reputation_grid.scanner_sample(
                channel, pos, heading, half_angle, range,
            );
            left[i] = l;
            right[i] = r;
        }

        (left, right)
    }

    // ── Terrain raycasts ────────────────────────────────────────────────────

    /// Cast 5 rays in a 120° arc centered on heading, each up to 40px.
    fn build_terrain_rays(&self, pos: Vec2, heading: f32) -> [TerrainRay; 5] {
        let n = self.config.terrain_ray_count.max(1) as usize;
        let arc = self.config.terrain_ray_arc_deg.to_radians();
        let range = self.config.terrain_ray_range;
        let step = if n > 1 { arc / (n - 1) as f32 } else { 0.0 };
        let start_angle = heading - arc / 2.0;

        let mut rays = [TerrainRay {
            distance: range,
            terrain_type: TerrainType::Plains,
            road_value: 0.0,
        }; 5];

        for i in 0..5.min(n) {
            let angle = start_angle + step * i as f32;
            let dir = Vec2::from_angle(angle);

            // March along the ray in small steps
            let step_dist = 2.0;
            let mut hit_dist = range;
            let mut hit_terrain = TerrainType::Plains;
            let mut hit_road = 0.0;
            let mut d = step_dist;
            while d <= range {
                let sample = pos + dir * d;
                let sx = (sample.x as u32).min(self.terrain.width().saturating_sub(1));
                let sy = (sample.y as u32).min(self.terrain.height().saturating_sub(1));
                let tt = self.terrain.terrain_at(sx, sy);

                if !tt.is_passable() {
                    hit_dist = d;
                    hit_terrain = tt;
                    hit_road = self.roads.road_value(sample);
                    break;
                }

                // Record the terrain at the tip of the ray
                hit_terrain = tt;
                hit_road = self.roads.road_value(sample);
                d += step_dist;
            }

            rays[i] = TerrainRay {
                distance: hit_dist,
                terrain_type: hit_terrain,
                road_value: hit_road,
            };
        }

        rays
    }

    // ── Neighbors ───────────────────────────────────────────────────────────

    fn build_neighbors(&self, pos: Vec2, others: &[&Merchant]) -> Vec<NeighborInfo> {
        let radius = self.config.neighbor_radius;
        let r2 = radius * radius;
        others
            .iter()
            .filter(|m| m.id != self.merchant.id && m.alive)
            .filter_map(|m| {
                let d2 = (m.pos - pos).length_squared();
                if d2 <= r2 {
                    Some(NeighborInfo {
                        relative_pos: m.pos - pos,
                        profession: m.profession,
                        inventory_fullness: m.inventory_fill_ratio(),
                        reputation: m.reputation,
                        caravan_id: m.caravan_id,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    // ── Navigation helpers ──────────────────────────────────────────────────

    fn find_nearest_city(&self, pos: Vec2) -> (Vec2, f32) {
        let start = (
            (pos.x as u32).min(self.terrain.width().saturating_sub(1)),
            (pos.y as u32).min(self.terrain.height().saturating_sub(1)),
        );

        // Prefer reachable cities (same connected component).
        let reachable = self
            .cities
            .iter()
            .filter(|c| {
                let goal = (
                    (c.position.x as u32).min(self.terrain.width().saturating_sub(1)),
                    (c.position.y as u32).min(self.terrain.height().saturating_sub(1)),
                );
                self.terrain.is_reachable(start, goal)
            })
            .map(|c| {
                let delta = c.position - pos;
                let dist = delta.length();
                (if dist > 1e-6 { delta.normalized() } else { Vec2::ZERO }, dist)
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        if let Some(result) = reachable {
            return result;
        }

        // Fallback: nearest by Euclidean distance (no reachable city found).
        self.cities
            .iter()
            .map(|c| {
                let delta = c.position - pos;
                let dist = delta.length();
                (if dist > 1e-6 { delta.normalized() } else { Vec2::ZERO }, dist)
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap_or((Vec2::ZERO, f32::MAX))
    }

    /// Compute an A* path from the merchant's position to a specific city.
    /// Returns grid-coordinate waypoints, or `None` if unreachable.
    pub fn find_path_to_city(&self, merchant_pos: Vec2, city_id: CityId) -> Option<Vec<(u32, u32)>> {
        let city = self.cities.iter().find(|c| c.id == city_id)?;
        let start = (
            (merchant_pos.x as u32).min(self.terrain.width().saturating_sub(1)),
            (merchant_pos.y as u32).min(self.terrain.height().saturating_sub(1)),
        );
        let goal = (
            (city.position.x as u32).min(self.terrain.width().saturating_sub(1)),
            (city.position.y as u32).min(self.terrain.height().saturating_sub(1)),
        );
        self.terrain.find_path(start, goal)
    }

    fn find_home_city(&self, pos: Vec2) -> (Vec2, f32) {
        self.cities
            .iter()
            .find(|c| c.id == self.merchant.home_city)
            .map(|c| {
                let delta = c.position - pos;
                let dist = delta.length();
                (if dist > 1e-6 { delta.normalized() } else { Vec2::ZERO }, dist)
            })
            .unwrap_or((Vec2::ZERO, f32::MAX))
    }

    fn find_nearest_resource(
        &self,
        pos: Vec2,
        nodes: &[ResourceNodeInfo],
    ) -> Option<(Vec2, f32, Commodity)> {
        nodes
            .iter()
            .map(|n| {
                let delta = n.pos - pos;
                let dist = delta.length();
                let dir = if dist > 1e-6 { delta.normalized() } else { Vec2::ZERO };
                (dir, dist, n.commodity)
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
    }

    fn find_nearest_bandit(
        &self,
        pos: Vec2,
        bandits: &[BanditInfo],
        max_range: f32,
    ) -> Option<(Vec2, f32)> {
        let r2 = max_range * max_range;
        bandits
            .iter()
            .filter_map(|b| {
                let delta = b.pos - pos;
                let d2 = delta.length_squared();
                if d2 <= r2 {
                    let dist = d2.sqrt();
                    let dir = if dist > 1e-6 { delta.normalized() } else { Vec2::ZERO };
                    Some((dir, dist))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EconomyConfig;
    use crate::types::Profession;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn setup() -> (EconomyConfig, Terrain, RoadGrid, ReputationGrid, Vec<City>) {
        let cfg = EconomyConfig::load("economy_config.toml").expect("test config");
        let terrain = Terrain::new(&cfg.world);
        let roads = RoadGrid::new(&cfg.road, cfg.world.width, cfg.world.height);
        let rep = ReputationGrid::new(&cfg.reputation, cfg.world.width, cfg.world.height);
        let mut rng = StdRng::seed_from_u64(42);
        let cities = City::generate(&cfg.world, &cfg.city, |pos| {
            let tx = (pos.x as u32).min(terrain.width().saturating_sub(1));
            let ty = (pos.y as u32).min(terrain.height().saturating_sub(1));
            terrain.terrain_at(tx, ty)
        }, &mut rng);
        (cfg, terrain, roads, rep, cities)
    }

    #[test]
    fn sensory_input_builds_without_panic() {
        let (cfg, terrain, roads, rep, cities) = setup();
        let mut rng = StdRng::seed_from_u64(99);
        let m = Merchant::new(
            1,
            Vec2::new(400.0, 300.0),
            cities[0].id,
            Profession::Trader,
            &cfg.merchant,
            &mut rng,
        );

        let builder = SensoryInputBuilder::new(
            &m, &cfg.merchant, &terrain, &roads, &rep, &cities, Season::Spring,
        );
        let input = builder.build(&[], &[], &[]);

        // Basic sanity checks
        assert_eq!(input.terrain_rays.len(), 5);
        assert!(input.neighbors.is_empty());
        assert!(input.nearest_bandit.is_none());
        assert!(input.nearest_resource.is_none());
        assert!((input.gold - cfg.merchant.initial_gold).abs() < 1e-6);
        assert!((input.fatigue).abs() < 1e-6);
    }

    #[test]
    fn nearest_city_returns_closest() {
        let (cfg, terrain, roads, rep, cities) = setup();
        let mut rng = StdRng::seed_from_u64(99);

        // Place merchant at first city's position
        let m = Merchant::new(
            1,
            cities[0].position,
            cities[0].id,
            Profession::Trader,
            &cfg.merchant,
            &mut rng,
        );

        let builder = SensoryInputBuilder::new(
            &m, &cfg.merchant, &terrain, &roads, &rep, &cities, Season::Spring,
        );
        let input = builder.build(&[], &[], &[]);

        // Distance to nearest city should be ~0 since we're at city[0]
        assert!(input.nearest_city.1 < 1.0);
    }

    #[test]
    fn neighbors_detected_within_range() {
        let (cfg, terrain, roads, rep, cities) = setup();
        let mut rng = StdRng::seed_from_u64(99);

        let m1 = Merchant::new(
            1,
            Vec2::new(400.0, 300.0),
            cities[0].id,
            Profession::Trader,
            &cfg.merchant,
            &mut rng,
        );
        let m2 = Merchant::new(
            2,
            Vec2::new(410.0, 300.0), // 10px away, within 30px range
            cities[0].id,
            Profession::Miner,
            &cfg.merchant,
            &mut rng,
        );
        let far = Merchant::new(
            3,
            Vec2::new(500.0, 500.0), // far away
            cities[0].id,
            Profession::Farmer,
            &cfg.merchant,
            &mut rng,
        );

        let builder = SensoryInputBuilder::new(
            &m1, &cfg.merchant, &terrain, &roads, &rep, &cities, Season::Spring,
        );
        let others: Vec<&Merchant> = vec![&m2, &far];
        let input = builder.build(&others, &[], &[]);

        assert_eq!(input.neighbors.len(), 1);
        assert_eq!(input.neighbors[0].profession, Profession::Miner);
    }

    #[test]
    fn bandit_detected_within_80px() {
        let (cfg, terrain, roads, rep, cities) = setup();
        let mut rng = StdRng::seed_from_u64(99);
        let m = Merchant::new(
            1,
            Vec2::new(400.0, 300.0),
            cities[0].id,
            Profession::Trader,
            &cfg.merchant,
            &mut rng,
        );

        let bandits = vec![
            BanditInfo { pos: Vec2::new(430.0, 300.0) }, // 30px away
            BanditInfo { pos: Vec2::new(600.0, 600.0) }, // far
        ];

        let builder = SensoryInputBuilder::new(
            &m, &cfg.merchant, &terrain, &roads, &rep, &cities, Season::Spring,
        );
        let input = builder.build(&[], &bandits, &[]);

        assert!(input.nearest_bandit.is_some());
        let (_, dist) = input.nearest_bandit.unwrap();
        assert!((dist - 30.0).abs() < 1.0);
    }
}
