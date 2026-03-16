use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

// ── Error type ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(toml::de::Error),
    Validation(Vec<String>),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config I/O error: {e}"),
            ConfigError::Parse(e) => write!(f, "config parse error: {e}"),
            ConfigError::Validation(errors) => {
                writeln!(f, "config validation errors:")?;
                for e in errors {
                    writeln!(f, "  - {e}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Parse(e)
    }
}

// ── Top-level config ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct EconomyConfig {
    pub world: WorldConfig,
    pub city: CityConfig,
    pub merchant: MerchantConfig,
    pub bandit: BanditConfig,
    pub reputation: ReputationConfig,
    pub road: RoadConfig,
    pub professions: ProfessionsConfig,
}

// ── Section structs ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct WorldConfig {
    pub width: u32,
    pub height: u32,
    pub terrain_seed: u32,
    pub terrain_octaves: u32,
    pub sea_level: f32,
    pub num_cities: u32,
    pub num_resource_nodes: u32,
    pub season_length_ticks: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CityConfig {
    pub radius: f32,
    pub population_range: [u32; 2],
    pub tax_rate_range: [f32; 2],
    pub warehouse_capacity: f32,
    pub warehouse_decay_rate: f32,
    pub npc_demand_base: f32,
    pub order_ttl: u32,
    pub upgrade_costs: UpgradeCosts,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpgradeCosts {
    pub market_hall: f32,
    pub walls: f32,
    pub harbor: f32,
    pub workshop: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MerchantConfig {
    pub initial_population: u32,
    pub max_population: u32,
    pub spawn_rate: f32,
    pub initial_gold: f32,
    pub base_speed: f32,
    pub max_carry: f32,
    pub shipwright_carry_mult: f32,
    pub shipwright_speed_mult: f32,
    pub fatigue_max: f32,
    pub fatigue_cost_base: f32,
    pub fatigue_cost_speed: f32,
    pub fatigue_cost_carry: f32,
    pub fatigue_recovery_rate: f32,
    pub scanner_angle_deg: f32,
    pub scanner_range: f32,
    pub neighbor_radius: f32,
    pub terrain_ray_count: u32,
    pub terrain_ray_range: f32,
    pub terrain_ray_arc_deg: f32,
    pub gossip_range: f32,
    pub price_memory_ttl: u32,
    pub caravan_join_range: f32,
    pub caravan_min_size: u32,
    pub caravan_safe_size: u32,
    pub bankruptcy_grace_ticks: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BanditConfig {
    pub num_camps: u32,
    pub patrol_radius_range: [f32; 2],
    pub agents_per_camp: [u32; 2],
    pub rob_gold_pct: [f32; 2],
    pub rob_goods_pct: [f32; 2],
    pub attack_range: f32,
    pub starvation_ticks: u32,
    pub respawn_interval: u32,
    pub seasonal_activity: SeasonalActivity,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeasonalActivity {
    pub spring: f32,
    pub summer: f32,
    pub autumn: f32,
    pub winter: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReputationConfig {
    pub cell_size: u32,
    pub channels: ReputationChannels,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReputationChannels {
    pub profit: ChannelConfig,
    pub demand: ChannelConfig,
    pub danger: ChannelConfig,
    pub opportunity: ChannelConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelConfig {
    pub decay: f32,
    pub diffusion_sigma: f32,
    pub color: [u8; 3],
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoadConfig {
    pub cell_size: u32,
    pub increment: f32,
    pub decay: f32,
    pub max_speed_bonus: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfessionsConfig {
    pub default_distribution: HashMap<String, f32>,
    pub rebalance_interval: u32,
}

// ── Loader + validation ────────────────────────────────────────────────────

impl EconomyConfig {
    /// Load config from a TOML file, parsing and validating all fields.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path)?;
        Self::from_str(&contents)
    }

    /// Parse and validate from a TOML string.
    pub fn from_str(toml_str: &str) -> Result<Self, ConfigError> {
        let config: EconomyConfig = toml::from_str(toml_str)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate ranges and constraints. Returns `Err(ConfigError::Validation)`
    /// with all errors collected if any check fails.
    fn validate(&self) -> Result<(), ConfigError> {
        let mut errors = Vec::new();

        // World
        if self.world.width == 0 || self.world.height == 0 {
            errors.push("world dimensions must be > 0".into());
        }
        if self.world.sea_level < 0.0 || self.world.sea_level > 1.0 {
            errors.push("sea_level must be in [0.0, 1.0]".into());
        }
        if self.world.num_cities == 0 {
            errors.push("num_cities must be > 0".into());
        }
        if self.world.season_length_ticks == 0 {
            errors.push("season_length_ticks must be > 0".into());
        }

        // City ranges
        validate_range_u32(
            &mut errors,
            "city.population_range",
            &self.city.population_range,
        );
        validate_range_f32(
            &mut errors,
            "city.tax_rate_range",
            &self.city.tax_rate_range,
        );
        if self.city.tax_rate_range[0] < 0.0 {
            errors.push("city.tax_rate_range min must be >= 0.0".into());
        }
        if self.city.tax_rate_range[1] > 1.0 {
            errors.push("city.tax_rate_range max must be <= 1.0".into());
        }
        if self.city.warehouse_capacity <= 0.0 {
            errors.push("city.warehouse_capacity must be > 0".into());
        }

        // Merchant
        if self.merchant.initial_population == 0 {
            errors.push("merchant.initial_population must be > 0".into());
        }
        if self.merchant.max_population < self.merchant.initial_population {
            errors.push(
                "merchant.max_population must be >= initial_population".into(),
            );
        }
        if self.merchant.base_speed <= 0.0 {
            errors.push("merchant.base_speed must be > 0".into());
        }
        if self.merchant.max_carry <= 0.0 {
            errors.push("merchant.max_carry must be > 0".into());
        }
        if self.merchant.fatigue_max <= 0.0 {
            errors.push("merchant.fatigue_max must be > 0".into());
        }

        // Bandit ranges
        validate_range_f32(
            &mut errors,
            "bandit.patrol_radius_range",
            &self.bandit.patrol_radius_range,
        );
        validate_range_u32(
            &mut errors,
            "bandit.agents_per_camp",
            &self.bandit.agents_per_camp,
        );
        validate_range_f32(
            &mut errors,
            "bandit.rob_gold_pct",
            &self.bandit.rob_gold_pct,
        );
        validate_range_f32(
            &mut errors,
            "bandit.rob_goods_pct",
            &self.bandit.rob_goods_pct,
        );
        for pct in [
            &self.bandit.rob_gold_pct,
            &self.bandit.rob_goods_pct,
        ] {
            if pct[0] < 0.0 || pct[1] > 1.0 {
                errors.push(format!(
                    "bandit percentage range must be within [0.0, 1.0], got [{}, {}]",
                    pct[0], pct[1]
                ));
            }
        }

        // Reputation
        if self.reputation.cell_size == 0 {
            errors.push("reputation.cell_size must be > 0".into());
        }
        for (name, ch) in [
            ("profit", &self.reputation.channels.profit),
            ("demand", &self.reputation.channels.demand),
            ("danger", &self.reputation.channels.danger),
            ("opportunity", &self.reputation.channels.opportunity),
        ] {
            if ch.decay < 0.0 || ch.decay > 1.0 {
                errors.push(format!("reputation.channels.{name}.decay must be in [0.0, 1.0]"));
            }
            if ch.diffusion_sigma < 0.0 {
                errors.push(format!(
                    "reputation.channels.{name}.diffusion_sigma must be >= 0.0"
                ));
            }
        }

        // Road
        if self.road.cell_size == 0 {
            errors.push("road.cell_size must be > 0".into());
        }
        if self.road.decay < 0.0 || self.road.decay > 1.0 {
            errors.push("road.decay must be in [0.0, 1.0]".into());
        }

        // Professions distribution sums to ~1.0
        let total: f32 = self.professions.default_distribution.values().sum();
        if (total - 1.0).abs() > 0.01 {
            errors.push(format!(
                "professions.default_distribution must sum to ~1.0, got {total:.4}"
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Validation(errors))
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn validate_range_u32(errors: &mut Vec<String>, name: &str, range: &[u32; 2]) {
    if range[0] > range[1] {
        errors.push(format!("{name}: min ({}) must be <= max ({})", range[0], range[1]));
    }
}

fn validate_range_f32(errors: &mut Vec<String>, name: &str, range: &[f32; 2]) {
    if range[0] > range[1] {
        errors.push(format!("{name}: min ({}) must be <= max ({})", range[0], range[1]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_default_config() {
        let config = EconomyConfig::load("economy_config.toml")
            .expect("default config should load and validate");
        assert_eq!(config.world.width, 1600);
        assert_eq!(config.world.num_cities, 10);
        assert_eq!(config.merchant.initial_population, 200);
    }

    #[test]
    fn test_invalid_range_detected() {
        let toml = include_str!("../economy_config.toml")
            .replace("population_range = [50, 500]", "population_range = [500, 50]");
        let result = EconomyConfig::from_str(&toml);
        assert!(result.is_err());
        if let Err(ConfigError::Validation(errors)) = result {
            assert!(errors.iter().any(|e| e.contains("population_range")));
        }
    }

    #[test]
    fn test_invalid_tax_rate_detected() {
        let toml = include_str!("../economy_config.toml")
            .replace("tax_rate_range = [0.0, 0.15]", "tax_rate_range = [0.0, 1.5]");
        let result = EconomyConfig::from_str(&toml);
        assert!(result.is_err());
        if let Err(ConfigError::Validation(errors)) = result {
            assert!(errors.iter().any(|e| e.contains("tax_rate_range")));
        }
    }

    #[test]
    fn test_profession_distribution_must_sum_to_one() {
        let toml = include_str!("../economy_config.toml")
            .replace("trader = 0.40", "trader = 0.90");
        let result = EconomyConfig::from_str(&toml);
        assert!(result.is_err());
        if let Err(ConfigError::Validation(errors)) = result {
            assert!(errors.iter().any(|e| e.contains("default_distribution")));
        }
    }
}
