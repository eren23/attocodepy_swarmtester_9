use std::collections::{HashMap, HashSet};

use rand::Rng;

use crate::config::{CityConfig, UpgradeCosts, WorldConfig};
use crate::types::{CityId, CityUpgrade, Commodity, TerrainType, Vec2};

// ── City ──────────────────────────────────────────────────────────────────

pub struct City {
    pub id: CityId,
    pub position: Vec2,
    pub radius: f32,
    pub population: f32,
    pub tax_rate: f32,
    pub treasury: f32,
    pub warehouse: HashMap<Commodity, f32>,
    pub prosperity: f32,
    pub specialization: Commodity,
    pub upgrades: HashSet<CityUpgrade>,
    pub ticks_without_food: u32,
    pub is_coastal: bool,
    /// Running trade-volume counter, reset each tax-adjustment window.
    pub trade_volume: f32,
    /// Snapshot of the previous window's average trade volume across cities.
    pub avg_trade_volume: f32,
}

impl City {
    /// Create a new city with randomized starting values drawn from config ranges.
    pub fn new(
        id: CityId,
        position: Vec2,
        is_coastal: bool,
        config: &CityConfig,
        rng: &mut impl Rng,
    ) -> Self {
        let pop_lo = config.population_range[0] as f32;
        let pop_hi = config.population_range[1] as f32;
        let population = rng.gen_range(pop_lo..=pop_hi);

        let tax_rate = rng.gen_range(config.tax_rate_range[0]..=config.tax_rate_range[1]);

        // Random specialization from all commodities.
        let spec_idx = rng.gen_range(0..Commodity::ALL.len());
        let specialization = Commodity::ALL[spec_idx];

        Self {
            id,
            position,
            radius: config.radius,
            population,
            tax_rate,
            treasury: 0.0,
            warehouse: HashMap::new(),
            prosperity: 50.0,
            specialization,
            upgrades: HashSet::new(),
            ticks_without_food: 0,
            is_coastal,
            trade_volume: 0.0,
            avg_trade_volume: 0.0,
        }
    }

    // ── Tick methods ──────────────────────────────────────────────────────

    /// Grow or shrink population based on prosperity and food stocks.
    ///
    /// Growth: +0.01/tick when prosperity > 60 AND at least one food commodity
    /// (Grain, Fish, Provisions) is stocked above zero.
    /// Decline: -0.02/tick when no food for 200+ consecutive ticks.
    pub fn tick_population(&mut self, config: &CityConfig) {
        let has_food = self.warehouse.get(&Commodity::Grain).copied().unwrap_or(0.0) > 0.0
            || self.warehouse.get(&Commodity::Fish).copied().unwrap_or(0.0) > 0.0
            || self.warehouse.get(&Commodity::Provisions).copied().unwrap_or(0.0) > 0.0;

        if has_food {
            self.ticks_without_food = 0;
            if self.prosperity > 60.0 {
                self.population += 0.01;
            }
        } else {
            self.ticks_without_food += 1;
            if self.ticks_without_food >= 200 {
                self.population -= 0.02;
            }
        }

        let pop_min = config.population_range[0] as f32;
        let pop_max = config.population_range[1] as f32;
        self.population = self.population.clamp(pop_min, pop_max);
    }

    /// Decay overstocked goods by 0.1% per tick.
    /// "Overstocked" means total warehouse quantity exceeds capacity.
    pub fn tick_warehouse(&mut self, config: &CityConfig) {
        let total: f32 = self.warehouse.values().sum();
        if total > config.warehouse_capacity {
            let decay = config.warehouse_decay_rate;
            for qty in self.warehouse.values_mut() {
                *qty *= 1.0 - decay;
            }
            // Remove entries that have decayed to negligible amounts.
            self.warehouse.retain(|_, q| *q > 0.001);
        }
    }

    /// Every 500 ticks: raise tax by 1% if trade volume above average,
    /// lower by 1% if below. Clamp to [0.0, 0.15].
    pub fn tick_tax_adjustment(&mut self, tick: u32, avg_trade_volume: f32) {
        if tick % 500 != 0 {
            return;
        }
        self.avg_trade_volume = avg_trade_volume;

        if self.trade_volume > avg_trade_volume {
            self.tax_rate += 0.01;
        } else if self.trade_volume < avg_trade_volume {
            self.tax_rate -= 0.01;
        }
        self.tax_rate = self.tax_rate.clamp(0.0, 0.15);

        // Reset trade volume for the next window.
        self.trade_volume = 0.0;
    }

    /// Compute prosperity from population, trade volume, warehouse fullness,
    /// and commodity diversity. Result in [0, 100].
    pub fn compute_prosperity(&mut self, config: &CityConfig) {
        let pop_max = config.population_range[1] as f32;
        let pop_score = (self.population / pop_max) * 25.0;

        // Trade volume score — cap contribution at 25 so it saturates nicely.
        let trade_score = (self.trade_volume / 100.0).min(1.0) * 25.0;

        // Warehouse fullness score.
        let total: f32 = self.warehouse.values().sum();
        let fullness = (total / config.warehouse_capacity).min(1.0);
        let fullness_score = fullness * 25.0;

        // Commodity diversity score — how many distinct commodities are stocked.
        let diversity = self.warehouse.len() as f32;
        let diversity_score = (diversity / Commodity::ALL.len() as f32).min(1.0) * 25.0;

        self.prosperity = (pop_score + trade_score + fullness_score + diversity_score)
            .clamp(0.0, 100.0);
    }

    /// Record a trade of `value` gold at this city, accumulating tax revenue.
    pub fn record_trade(&mut self, value: f32) {
        self.trade_volume += value;
        self.treasury += value * self.tax_rate;
    }

    // ── Upgrades ──────────────────────────────────────────────────────────

    /// Try to purchase an upgrade, deducting cost from treasury.
    /// Returns `true` if the upgrade was purchased.
    pub fn try_purchase_upgrade(
        &mut self,
        upgrade: CityUpgrade,
        costs: &UpgradeCosts,
    ) -> bool {
        if self.upgrades.contains(&upgrade) {
            return false;
        }
        let cost = match upgrade {
            CityUpgrade::MarketHall => costs.market_hall,
            CityUpgrade::Walls => costs.walls,
            CityUpgrade::Harbor => costs.harbor,
            CityUpgrade::Workshop => costs.workshop,
        };
        if self.treasury >= cost {
            self.treasury -= cost;
            self.upgrades.insert(upgrade);
            true
        } else {
            false
        }
    }

    /// Crafting speed multiplier for the given commodity.
    /// 1.5× for the city's specialization commodity; further 1.25× if Workshop upgrade.
    pub fn crafting_speed(&self, commodity: Commodity) -> f32 {
        let mut mult = 1.0;
        if commodity == self.specialization {
            mult *= 1.5;
        }
        if self.upgrades.contains(&CityUpgrade::Workshop) {
            mult *= 1.25;
        }
        mult
    }

    // ── Placement ─────────────────────────────────────────────────────────

    /// Generate `n` city positions via Poisson-disk sampling with `min_spacing`.
    /// Positions are placed only on passable, non-water terrain.
    /// Coastal status is derived from the terrain at each position.
    ///
    /// `terrain_at` should return the `TerrainType` for a given world position.
    pub fn poisson_disk_placement(
        n: u32,
        min_spacing: f32,
        world_w: f32,
        world_h: f32,
        terrain_at: impl Fn(Vec2) -> TerrainType,
        rng: &mut impl Rng,
    ) -> Vec<(Vec2, bool)> {
        let mut points: Vec<Vec2> = Vec::with_capacity(n as usize);
        let mut results: Vec<(Vec2, bool)> = Vec::with_capacity(n as usize);
        let max_attempts = 1000;

        while points.len() < n as usize {
            let mut placed = false;
            for _ in 0..max_attempts {
                let candidate = Vec2::new(
                    rng.gen_range(min_spacing..world_w - min_spacing),
                    rng.gen_range(min_spacing..world_h - min_spacing),
                );

                let terrain = terrain_at(candidate);
                if !terrain.is_passable() {
                    continue;
                }

                let too_close = points
                    .iter()
                    .any(|p| p.distance(candidate) < min_spacing);
                if too_close {
                    continue;
                }

                let is_coastal = terrain == TerrainType::Coast;
                points.push(candidate);
                results.push((candidate, is_coastal));
                placed = true;
                break;
            }
            if !placed {
                // Cannot place more cities — return what we have.
                break;
            }
        }
        results
    }

    /// Convenience: generate cities from config using Poisson-disk sampling.
    pub fn generate(
        world_config: &WorldConfig,
        city_config: &CityConfig,
        terrain_at: impl Fn(Vec2) -> TerrainType,
        rng: &mut impl Rng,
    ) -> Vec<City> {
        let min_spacing = city_config.radius * 4.0;
        let placements = Self::poisson_disk_placement(
            world_config.num_cities,
            min_spacing,
            world_config.width as f32,
            world_config.height as f32,
            terrain_at,
            rng,
        );

        placements
            .into_iter()
            .enumerate()
            .map(|(i, (pos, coastal))| City::new(i as CityId, pos, coastal, city_config, rng))
            .collect()
    }
}
