#![allow(dead_code)]

use std::collections::HashMap;

use rand::rngs::StdRng;
use rand::SeedableRng;

use swarm_economy::config::*;
use swarm_economy::types::*;
use swarm_economy::agents::merchant::Merchant;
use swarm_economy::agents::sensory::SensoryInput;
use swarm_economy::world::city::City;
use swarm_economy::world::terrain::Terrain;
use swarm_economy::world::road::RoadGrid;
use swarm_economy::world::reputation::ReputationGrid;

// ── Config Factories ─────────────────────────────────────────────────────

pub fn mini_world_config() -> WorldConfig {
    WorldConfig {
        width: 64,
        height: 64,
        terrain_seed: 42,
        terrain_octaves: 4,
        sea_level: 0.25,
        num_cities: 3,
        num_resource_nodes: 10,
        season_length_ticks: 100,
    }
}

pub fn all_land_world_config() -> WorldConfig {
    WorldConfig {
        sea_level: 0.0,
        ..mini_world_config()
    }
}

pub fn mini_city_config() -> CityConfig {
    CityConfig {
        radius: 15.0,
        population_range: [50, 200],
        tax_rate_range: [0.0, 0.15],
        warehouse_capacity: 200.0,
        warehouse_decay_rate: 0.001,
        npc_demand_base: 0.01,
        order_ttl: 200,
        upgrade_costs: UpgradeCosts {
            market_hall: 500.0,
            walls: 800.0,
            harbor: 1000.0,
            workshop: 600.0,
        },
    }
}

pub fn mini_merchant_config() -> MerchantConfig {
    MerchantConfig {
        initial_population: 10,
        max_population: 20,
        spawn_rate: 0.05,
        initial_gold: 100.0,
        base_speed: 1.5,
        max_carry: 10.0,
        shipwright_carry_mult: 3.0,
        shipwright_speed_mult: 2.0,
        fatigue_max: 100.0,
        fatigue_cost_base: 0.03,
        fatigue_cost_speed: 0.02,
        fatigue_cost_carry: 0.04,
        fatigue_recovery_rate: 1.5,
        scanner_angle_deg: 35.0,
        scanner_range: 60.0,
        neighbor_radius: 30.0,
        terrain_ray_count: 5,
        terrain_ray_range: 40.0,
        terrain_ray_arc_deg: 120.0,
        gossip_range: 25.0,
        price_memory_ttl: 1000,
        caravan_join_range: 30.0,
        caravan_min_size: 3,
        caravan_safe_size: 4,
        bankruptcy_grace_ticks: 200,
    }
}

pub fn mini_road_config() -> RoadConfig {
    RoadConfig {
        cell_size: 8,
        increment: 0.002,
        decay: 0.9998,
        max_speed_bonus: 0.6,
    }
}

pub fn mini_reputation_config() -> ReputationConfig {
    let ch = ChannelConfig {
        decay: 0.99,
        diffusion_sigma: 0.6,
        color: [255, 255, 255],
    };
    ReputationConfig {
        cell_size: 8,
        channels: ReputationChannels {
            profit: ch.clone(),
            demand: ch.clone(),
            danger: ch.clone(),
            opportunity: ch,
        },
    }
}

pub fn mini_bandit_config() -> BanditConfig {
    BanditConfig {
        num_camps: 2,
        patrol_radius_range: [30.0, 60.0],
        agents_per_camp: [2, 3],
        rob_gold_pct: [0.10, 0.30],
        rob_goods_pct: [0.20, 0.40],
        attack_range: 15.0,
        starvation_ticks: 3000,
        respawn_interval: 100,
        seasonal_activity: SeasonalActivity {
            spring: 1.0,
            summer: 1.3,
            autumn: 1.0,
            winter: 0.5,
        },
    }
}

pub fn mini_professions_config() -> ProfessionsConfig {
    let mut dist = HashMap::new();
    dist.insert("trader".to_string(), 0.40);
    dist.insert("miner".to_string(), 0.12);
    dist.insert("farmer".to_string(), 0.10);
    dist.insert("craftsman".to_string(), 0.18);
    dist.insert("soldier".to_string(), 0.08);
    dist.insert("shipwright".to_string(), 0.05);
    dist.insert("idle".to_string(), 0.07);
    ProfessionsConfig {
        default_distribution: dist,
        rebalance_interval: 500,
    }
}

pub fn mini_economy_config() -> EconomyConfig {
    EconomyConfig {
        world: mini_world_config(),
        city: mini_city_config(),
        merchant: mini_merchant_config(),
        bandit: mini_bandit_config(),
        reputation: mini_reputation_config(),
        road: mini_road_config(),
        professions: mini_professions_config(),
    }
}

// ── Object Factories ─────────────────────────────────────────────────────

pub fn make_terrain() -> Terrain {
    Terrain::new(&mini_world_config())
}

pub fn make_all_land_terrain() -> Terrain {
    Terrain::new(&all_land_world_config())
}

pub fn make_road_grid() -> RoadGrid {
    RoadGrid::new(&mini_road_config(), 64, 64)
}

pub fn make_reputation_grid() -> ReputationGrid {
    ReputationGrid::new(&mini_reputation_config(), 64, 64)
}

pub fn make_city(id: CityId, pos: Vec2) -> City {
    let config = mini_city_config();
    let mut rng = StdRng::seed_from_u64(id as u64 + 100);
    City::new(id, pos, false, &config, &mut rng)
}

pub fn make_coastal_city(id: CityId, pos: Vec2) -> City {
    let config = mini_city_config();
    let mut rng = StdRng::seed_from_u64(id as u64 + 100);
    City::new(id, pos, true, &config, &mut rng)
}

pub fn make_mini_cities() -> Vec<City> {
    vec![
        make_city(0, Vec2::new(16.0, 16.0)),
        make_city(1, Vec2::new(48.0, 16.0)),
        make_city(2, Vec2::new(32.0, 48.0)),
    ]
}

pub fn make_merchant_at(pos: Vec2, profession: Profession) -> Merchant {
    let config = mini_merchant_config();
    let mut rng = StdRng::seed_from_u64(42);
    Merchant::new(1, pos, 0, profession, &config, &mut rng)
}

pub fn make_merchant_with_id(id: u32, pos: Vec2, profession: Profession) -> Merchant {
    let config = mini_merchant_config();
    let mut rng = StdRng::seed_from_u64(id as u64);
    Merchant::new(id, pos, 0, profession, &config, &mut rng)
}

/// Find a passable cell in the terrain.
pub fn find_passable_pos(terrain: &Terrain) -> Vec2 {
    for y in 1..terrain.height() - 1 {
        for x in 1..terrain.width() - 1 {
            if terrain.is_passable(x, y) {
                return Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            }
        }
    }
    panic!("no passable cell found in terrain");
}

/// Find adjacent passable and impassable cells for collision tests.
pub fn find_terrain_boundary(terrain: &Terrain) -> (Vec2, Vec2) {
    for y in 1..terrain.height() - 1 {
        for x in 1..terrain.width() - 2 {
            if terrain.is_passable(x, y) && !terrain.is_passable(x + 1, y) {
                return (
                    Vec2::new(x as f32 + 0.5, y as f32 + 0.5),
                    Vec2::new(x as f32 + 1.5, y as f32 + 0.5),
                );
            }
        }
    }
    panic!("no terrain boundary found");
}

/// Default sensory input with all neutral values.
pub fn default_sensory_input() -> SensoryInput {
    SensoryInput {
        left_scanner: [0.0; 4],
        right_scanner: [0.0; 4],
        terrain_rays: [TerrainRay {
            distance: 40.0,
            terrain_type: TerrainType::Plains,
            road_value: 0.0,
        }; 5],
        neighbors: vec![],
        nearest_city: (Vec2::new(1.0, 0.0), 50.0),
        home_city: (Vec2::new(1.0, 0.0), 50.0),
        nearest_resource: None,
        profit_gradient: Vec2::ZERO,
        danger_gradient: Vec2::ZERO,
        gold: 100.0,
        fatigue: 0.0,
        inventory_fill_ratio: 0.0,
        inventory_breakdown: HashMap::new(),
        current_terrain: TerrainType::Plains,
        current_season: Season::Spring,
        reputation: 50.0,
        nearest_bandit: None,
    }
}

/// Sensory input simulating being at a city (distance ~5).
pub fn sensory_at_city() -> SensoryInput {
    SensoryInput {
        nearest_city: (Vec2::new(0.0, 0.0), 5.0),
        home_city: (Vec2::new(0.0, 0.0), 5.0),
        ..default_sensory_input()
    }
}

/// Sensory input simulating being at a resource node.
pub fn sensory_at_resource(commodity: Commodity) -> SensoryInput {
    SensoryInput {
        nearest_resource: Some((Vec2::new(1.0, 0.0), 5.0, commodity)),
        ..default_sensory_input()
    }
}

pub fn test_rng() -> StdRng {
    StdRng::seed_from_u64(42)
}
