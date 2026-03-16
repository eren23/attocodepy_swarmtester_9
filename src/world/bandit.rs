use rand::Rng;
use std::collections::HashMap;

use crate::config::BanditConfig;
use crate::types::{Inventory, Profession, Season, TerrainType, Vec2};

// ── Constants ────────────────────────────────────────────────────────────────

/// Soldiers within this range of a robbery site engage in combat.
const SOLDIER_COMBAT_RANGE: f32 = 25.0;

/// Probability that a soldier wins combat against a bandit.
const SOLDIER_WIN_CHANCE: f32 = 0.5;

/// Reputation gained by a soldier who wins combat.
const SOLDIER_REPUTATION_GAIN: f32 = 10.0;

/// Fraction of gold lost by a soldier defeated in combat.
const SOLDIER_GOLD_LOSS_PCT: f32 = 0.3;

/// Strength of the DANGER signal deposited after a robbery.
const DANGER_SIGNAL_STRENGTH: f32 = 0.8;

/// Minimum distance from any city center for bandit camp placement.
const MIN_CITY_DISTANCE: f32 = 150.0;

/// Caravans of this size or larger cannot be attacked.
const CARAVAN_IMMUNE_SIZE: u32 = 4;

/// Caravans of this size have a chance to repel attacks.
const CARAVAN_REPEL_SIZE: u32 = 3;

/// Probability that a group of `CARAVAN_REPEL_SIZE` repels an attack.
const CARAVAN_REPEL_CHANCE: f32 = 0.7;

/// Base per-bandit probability of attempting an attack each tick,
/// multiplied by the seasonal activity modifier.
const BASE_ATTACK_CHANCE: f32 = 0.7;

// ── Types ────────────────────────────────────────────────────────────────────

pub type BanditCampId = u32;

// ── BanditCamp ───────────────────────────────────────────────────────────────

pub struct BanditCamp {
    pub id: BanditCampId,
    pub position: Vec2,
    pub patrol_radius: f32,
    pub agent_count: u32,
    pub ticks_since_last_robbery: u32,
    pub alive: bool,
}

// ── Bandit ────────────────────────────────────────────────────────────────────

pub struct Bandit {
    pub position: Vec2,
    pub camp_id: BanditCampId,
    pub active: bool,
}

// ── External info ────────────────────────────────────────────────────────────

/// Lightweight view of a merchant/soldier, provided by the caller.
pub struct MerchantInfo {
    pub id: u32,
    pub position: Vec2,
    pub gold: f32,
    pub inventory: Inventory,
    pub profession: Profession,
    pub group_size: u32,
}

/// Lightweight view of a city, provided by the caller.
pub struct CityInfo {
    pub position: Vec2,
    pub has_walls: bool,
}

// ── Outcome types ────────────────────────────────────────────────────────────

pub struct RobberyOutcome {
    pub merchant_id: u32,
    pub bandit_camp_id: BanditCampId,
    pub gold_stolen: f32,
    pub goods_stolen: Inventory,
    pub position: Vec2,
}

pub struct CombatOutcome {
    pub soldier_id: u32,
    pub bandit_camp_id: BanditCampId,
    pub soldier_wins: bool,
    pub reputation_delta: f32,
    /// Gold lost by the soldier (nonzero only when the bandit wins).
    pub gold_lost: f32,
    /// Inventory lost by the soldier (populated only when the bandit wins).
    pub inventory_lost: Inventory,
}

pub struct TickResult {
    pub robberies: Vec<RobberyOutcome>,
    pub combats: Vec<CombatOutcome>,
    /// World positions where a DANGER signal should be deposited, with strength.
    pub danger_deposits: Vec<(Vec2, f32)>,
    pub camps_destroyed: Vec<BanditCampId>,
    pub camps_spawned: u32,
}

// ── BanditSystem ─────────────────────────────────────────────────────────────

pub struct BanditSystem {
    camps: Vec<BanditCamp>,
    bandits: Vec<Bandit>,
    next_camp_id: BanditCampId,
}

impl BanditSystem {
    /// Create the bandit system and place initial camps.
    pub fn new(
        config: &BanditConfig,
        cities: &[CityInfo],
        terrain_at: impl Fn(Vec2) -> TerrainType,
        world_w: f32,
        world_h: f32,
        rng: &mut impl Rng,
    ) -> Self {
        let mut system = Self {
            camps: Vec::new(),
            bandits: Vec::new(),
            next_camp_id: 0,
        };

        for _ in 0..config.num_camps {
            system.spawn_camp(config, cities, &terrain_at, world_w, world_h, rng);
        }

        system
    }

    // ── Main tick ────────────────────────────────────────────────────────

    /// Run one simulation tick. Returns events for the caller to apply to
    /// merchant state and the reputation grid.
    pub fn tick(
        &mut self,
        config: &BanditConfig,
        merchants: &[MerchantInfo],
        cities: &[CityInfo],
        season: Season,
        terrain_at: impl Fn(Vec2) -> TerrainType,
        world_w: f32,
        world_h: f32,
        rng: &mut impl Rng,
    ) -> TickResult {
        let mut result = TickResult {
            robberies: Vec::new(),
            combats: Vec::new(),
            danger_deposits: Vec::new(),
            camps_destroyed: Vec::new(),
            camps_spawned: 0,
        };

        self.tick_patrol(rng);

        let activity = seasonal_modifier(season, config);
        self.tick_attacks(config, merchants, cities, activity, &mut result, rng);

        self.tick_lifecycle(config, cities, &terrain_at, world_w, world_h, &mut result, rng);

        result
    }

    // ── Patrol ───────────────────────────────────────────────────────────

    /// Move each active bandit randomly within its camp's patrol radius.
    fn tick_patrol(&mut self, rng: &mut impl Rng) {
        // Snapshot camp data so we can mutate bandits freely.
        let camp_data: Vec<(BanditCampId, Vec2, f32, bool)> = self
            .camps
            .iter()
            .map(|c| (c.id, c.position, c.patrol_radius, c.alive))
            .collect();

        for bandit in &mut self.bandits {
            if !bandit.active {
                continue;
            }

            let camp = camp_data.iter().find(|c| c.0 == bandit.camp_id);
            let (camp_pos, radius) = match camp {
                Some(&(_, pos, r, true)) => (pos, r),
                _ => {
                    bandit.active = false;
                    continue;
                }
            };

            let angle = rng.gen_range(0.0..std::f32::consts::TAU);
            let step = rng.gen_range(1.0..3.0);
            let new_pos = bandit.position + Vec2::from_angle(angle) * step;

            if new_pos.distance(camp_pos) <= radius {
                bandit.position = new_pos;
            }
        }
    }

    // ── Attacks ──────────────────────────────────────────────────────────

    /// Attempt robberies and resolve soldier combat.
    fn tick_attacks(
        &mut self,
        config: &BanditConfig,
        merchants: &[MerchantInfo],
        cities: &[CityInfo],
        activity_modifier: f32,
        result: &mut TickResult,
        rng: &mut impl Rng,
    ) {
        let attack_range = config.attack_range;
        let attack_prob = (BASE_ATTACK_CHANCE * activity_modifier).min(1.0);

        // Snapshot bandit state to avoid borrow issues.
        let snapshot: Vec<(usize, Vec2, BanditCampId, bool)> = self
            .bandits
            .iter()
            .enumerate()
            .map(|(i, b)| (i, b.position, b.camp_id, b.active))
            .collect();

        let mut robbed_merchants: Vec<u32> = Vec::new();
        let mut camps_with_robbery: Vec<BanditCampId> = Vec::new();
        let mut bandits_to_deactivate: Vec<usize> = Vec::new();

        for &(bandit_idx, bandit_pos, camp_id, active) in &snapshot {
            if !active || bandits_to_deactivate.contains(&bandit_idx) {
                continue;
            }

            // Per-bandit seasonal probability gate.
            if rng.gen::<f32>() >= attack_prob {
                continue;
            }

            // Bandits avoid walled cities.
            let near_walled = cities
                .iter()
                .any(|c| c.has_walls && c.position.distance(bandit_pos) < MIN_CITY_DISTANCE);
            if near_walled {
                continue;
            }

            for merchant in merchants {
                if robbed_merchants.contains(&merchant.id) {
                    continue;
                }
                if merchant.position.distance(bandit_pos) > attack_range {
                    continue;
                }

                // Caravan safety: groups of 4+ are immune.
                if merchant.group_size >= CARAVAN_IMMUNE_SIZE {
                    continue;
                }
                // Groups of 3 have a 70% chance to repel.
                if merchant.group_size >= CARAVAN_REPEL_SIZE
                    && rng.gen::<f32>() < CARAVAN_REPEL_CHANCE
                {
                    continue;
                }

                // Soldier combat: soldier-profession agents within 25px of the
                // robbery site engage.
                let nearby_soldier = merchants.iter().find(|m| {
                    m.profession == Profession::Soldier
                        && m.position.distance(merchant.position) <= SOLDIER_COMBAT_RANGE
                });

                if let Some(soldier) = nearby_soldier {
                    let soldier_wins = rng.gen::<f32>() < SOLDIER_WIN_CHANCE;
                    if soldier_wins {
                        // Bandit removed; soldier gains reputation.
                        bandits_to_deactivate.push(bandit_idx);
                        result.combats.push(CombatOutcome {
                            soldier_id: soldier.id,
                            bandit_camp_id: camp_id,
                            soldier_wins: true,
                            reputation_delta: SOLDIER_REPUTATION_GAIN,
                            gold_lost: 0.0,
                            inventory_lost: HashMap::new(),
                        });
                        break; // Bandit is gone.
                    } else {
                        // Soldier loses 30% gold and all inventory.
                        let gold_lost = soldier.gold * SOLDIER_GOLD_LOSS_PCT;
                        result.combats.push(CombatOutcome {
                            soldier_id: soldier.id,
                            bandit_camp_id: camp_id,
                            soldier_wins: false,
                            reputation_delta: 0.0,
                            gold_lost,
                            inventory_lost: soldier.inventory.clone(),
                        });
                        // Bandit survives — proceed to robbery.
                    }
                }

                // Execute robbery.
                let gold_pct =
                    rng.gen_range(config.rob_gold_pct[0]..=config.rob_gold_pct[1]);
                let goods_pct =
                    rng.gen_range(config.rob_goods_pct[0]..=config.rob_goods_pct[1]);

                let gold_stolen = merchant.gold * gold_pct;
                let mut goods_stolen = HashMap::new();
                for (&commodity, &qty) in &merchant.inventory {
                    let stolen = qty * goods_pct;
                    if stolen > 0.001 {
                        goods_stolen.insert(commodity, stolen);
                    }
                }

                robbed_merchants.push(merchant.id);
                camps_with_robbery.push(camp_id);

                result.robberies.push(RobberyOutcome {
                    merchant_id: merchant.id,
                    bandit_camp_id: camp_id,
                    gold_stolen,
                    goods_stolen,
                    position: merchant.position,
                });

                // Robbed merchants deposit a strong DANGER signal.
                result
                    .danger_deposits
                    .push((merchant.position, DANGER_SIGNAL_STRENGTH));

                break; // One robbery per bandit per tick.
            }
        }

        // Apply deactivations from soldier combat.
        for &idx in &bandits_to_deactivate {
            self.bandits[idx].active = false;
        }

        // Update camp starvation counters.
        for camp in &mut self.camps {
            if !camp.alive {
                continue;
            }
            if camps_with_robbery.contains(&camp.id) {
                camp.ticks_since_last_robbery = 0;
            } else {
                camp.ticks_since_last_robbery += 1;
            }
        }
    }

    // ── Lifecycle ────────────────────────────────────────────────────────

    /// Destroy starved camps and spawn replacements to maintain target count.
    fn tick_lifecycle(
        &mut self,
        config: &BanditConfig,
        cities: &[CityInfo],
        terrain_at: &impl Fn(Vec2) -> TerrainType,
        world_w: f32,
        world_h: f32,
        result: &mut TickResult,
        rng: &mut impl Rng,
    ) {
        // Destroy camps that exceeded starvation threshold.
        for camp in &mut self.camps {
            if camp.alive && camp.ticks_since_last_robbery >= config.starvation_ticks {
                camp.alive = false;
                result.camps_destroyed.push(camp.id);
            }
        }

        // Deactivate bandits belonging to dead camps.
        let dead_ids: Vec<BanditCampId> = self
            .camps
            .iter()
            .filter(|c| !c.alive)
            .map(|c| c.id)
            .collect();

        for bandit in &mut self.bandits {
            if bandit.active && dead_ids.contains(&bandit.camp_id) {
                bandit.active = false;
            }
        }

        // Spawn new camps to maintain target count.
        let alive = self.camps.iter().filter(|c| c.alive).count() as u32;
        if alive < config.num_camps {
            let needed = config.num_camps - alive;
            for _ in 0..needed {
                if self.spawn_camp(config, cities, terrain_at, world_w, world_h, rng) {
                    result.camps_spawned += 1;
                }
            }
        }
    }

    // ── Camp placement ───────────────────────────────────────────────────

    /// Place a new camp in Forest or Hills terrain, away from cities and
    /// other alive camps. Returns `true` if placement succeeded.
    fn spawn_camp(
        &mut self,
        config: &BanditConfig,
        cities: &[CityInfo],
        terrain_at: &impl Fn(Vec2) -> TerrainType,
        world_w: f32,
        world_h: f32,
        rng: &mut impl Rng,
    ) -> bool {
        let max_attempts = 500;
        let margin = 50.0;

        for _ in 0..max_attempts {
            let pos = Vec2::new(
                rng.gen_range(margin..world_w - margin),
                rng.gen_range(margin..world_h - margin),
            );

            // Must be Forest or Hills.
            let terrain = terrain_at(pos);
            if terrain != TerrainType::Forest && terrain != TerrainType::Hills {
                continue;
            }

            // Must be away from all cities.
            let too_close_to_city = cities
                .iter()
                .any(|c| c.position.distance(pos) < MIN_CITY_DISTANCE);
            if too_close_to_city {
                continue;
            }

            // Must be away from other alive camps.
            let too_close_to_camp = self.camps.iter().any(|c| {
                c.alive && c.position.distance(pos) < config.patrol_radius_range[1] * 2.0
            });
            if too_close_to_camp {
                continue;
            }

            let patrol_radius =
                rng.gen_range(config.patrol_radius_range[0]..=config.patrol_radius_range[1]);
            let agent_count =
                rng.gen_range(config.agents_per_camp[0]..=config.agents_per_camp[1]);

            let camp_id = self.next_camp_id;
            self.next_camp_id += 1;

            self.camps.push(BanditCamp {
                id: camp_id,
                position: pos,
                patrol_radius,
                agent_count,
                ticks_since_last_robbery: 0,
                alive: true,
            });

            // Spawn bandit agents scattered around camp center.
            for _ in 0..agent_count {
                let offset = Vec2::from_angle(rng.gen_range(0.0..std::f32::consts::TAU))
                    * rng.gen_range(0.0..patrol_radius * 0.5);
                self.bandits.push(Bandit {
                    position: pos + offset,
                    camp_id,
                    active: true,
                });
            }

            return true;
        }

        false
    }

    // ── Accessors ────────────────────────────────────────────────────────

    pub fn camps(&self) -> &[BanditCamp] {
        &self.camps
    }

    pub fn bandits(&self) -> &[Bandit] {
        &self.bandits
    }

    pub fn bandits_mut(&mut self) -> &mut [Bandit] {
        &mut self.bandits
    }

    pub fn active_camp_count(&self) -> u32 {
        self.camps.iter().filter(|c| c.alive).count() as u32
    }

    pub fn active_bandit_count(&self) -> u32 {
        self.bandits.iter().filter(|b| b.active).count() as u32
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn seasonal_modifier(season: Season, config: &BanditConfig) -> f32 {
    match season {
        Season::Spring => config.seasonal_activity.spring,
        Season::Summer => config.seasonal_activity.summer,
        Season::Autumn => config.seasonal_activity.autumn,
        Season::Winter => config.seasonal_activity.winter,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SeasonalActivity;
    use crate::types::Commodity;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn test_config() -> BanditConfig {
        BanditConfig {
            num_camps: 5,
            patrol_radius_range: [100.0, 200.0],
            agents_per_camp: [2, 4],
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

    fn forest_terrain(_pos: Vec2) -> TerrainType {
        TerrainType::Forest
    }

    #[test]
    fn initial_camp_placement() {
        let config = test_config();
        let mut rng = StdRng::seed_from_u64(42);
        let system = BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        assert_eq!(system.active_camp_count(), 5);
        // 5 camps × [2,4] agents each.
        assert!(system.active_bandit_count() >= 10);
        assert!(system.active_bandit_count() <= 20);
    }

    #[test]
    fn camps_only_on_forest_or_hills() {
        let config = test_config();
        let mut rng = StdRng::seed_from_u64(42);

        // All-plains terrain should yield no camps.
        let plains_only = |_: Vec2| TerrainType::Plains;
        let system =
            BanditSystem::new(&config, &[], plains_only, 1600.0, 1000.0, &mut rng);
        assert_eq!(system.active_camp_count(), 0);
    }

    #[test]
    fn camps_avoid_cities() {
        let config = test_config();
        let cities = vec![
            CityInfo { position: Vec2::new(400.0, 300.0), has_walls: false },
            CityInfo { position: Vec2::new(800.0, 500.0), has_walls: false },
        ];
        let mut rng = StdRng::seed_from_u64(42);
        let system =
            BanditSystem::new(&config, &cities, forest_terrain, 1600.0, 1000.0, &mut rng);

        for camp in system.camps() {
            for city in &cities {
                assert!(
                    camp.position.distance(city.position) >= MIN_CITY_DISTANCE,
                    "camp at {:?} too close to city at {:?}",
                    camp.position,
                    city.position,
                );
            }
        }
    }

    #[test]
    fn patrol_stays_within_radius() {
        let config = test_config();
        let mut rng = StdRng::seed_from_u64(42);
        let mut system =
            BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        for _ in 0..1000 {
            system.tick_patrol(&mut rng);
        }

        for bandit in system.bandits() {
            if !bandit.active {
                continue;
            }
            let camp = system
                .camps()
                .iter()
                .find(|c| c.id == bandit.camp_id)
                .unwrap();
            assert!(
                bandit.position.distance(camp.position) <= camp.patrol_radius,
                "bandit at {:?} outside patrol_radius {} (camp at {:?})",
                bandit.position,
                camp.patrol_radius,
                camp.position,
            );
        }
    }

    #[test]
    fn robbery_steals_within_configured_range() {
        let config = test_config();
        let mut rng = StdRng::seed_from_u64(42);
        let mut system =
            BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        // Place a merchant right next to the first bandit.
        let bandit_pos = system.camps()[0].position;
        system.bandits[0].position = bandit_pos;

        let mut inv = HashMap::new();
        inv.insert(Commodity::Timber, 50.0);

        let merchants = vec![MerchantInfo {
            id: 1,
            position: bandit_pos + Vec2::new(5.0, 0.0),
            gold: 100.0,
            inventory: inv,
            profession: Profession::Trader,
            group_size: 1,
        }];

        let result = system.tick(
            &config,
            &merchants,
            &[],
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );

        if let Some(robbery) = result.robberies.first() {
            // 10-30% of 100 gold.
            assert!(robbery.gold_stolen >= 10.0 && robbery.gold_stolen <= 30.0);
            // 20-40% of 50 timber.
            if let Some(&timber) = robbery.goods_stolen.get(&Commodity::Timber) {
                assert!(timber >= 10.0 && timber <= 20.0);
            }
        }
    }

    #[test]
    fn caravan_of_four_is_immune() {
        let config = test_config();
        let mut rng = StdRng::seed_from_u64(42);
        let mut system =
            BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        let bandit_pos = system.camps()[0].position;

        let merchants = vec![MerchantInfo {
            id: 1,
            position: bandit_pos + Vec2::new(5.0, 0.0),
            gold: 100.0,
            inventory: HashMap::new(),
            profession: Profession::Trader,
            group_size: 4,
        }];

        for _ in 0..50 {
            system.bandits[0].position = bandit_pos;
            let result = system.tick(
                &config,
                &merchants,
                &[],
                Season::Summer,
                forest_terrain,
                1600.0,
                1000.0,
                &mut rng,
            );
            assert!(
                result.robberies.is_empty(),
                "caravan of 4+ should never be robbed",
            );
        }
    }

    #[test]
    fn walled_city_prevents_attack() {
        let config = test_config();
        let mut rng = StdRng::seed_from_u64(42);
        let mut system =
            BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        let bandit_pos = system.camps()[0].position;

        let cities = vec![CityInfo {
            position: bandit_pos + Vec2::new(50.0, 0.0),
            has_walls: true,
        }];

        let merchants = vec![MerchantInfo {
            id: 1,
            position: bandit_pos + Vec2::new(5.0, 0.0),
            gold: 100.0,
            inventory: HashMap::new(),
            profession: Profession::Trader,
            group_size: 1,
        }];

        for _ in 0..50 {
            system.bandits[0].position = bandit_pos;
            let result = system.tick(
                &config,
                &merchants,
                &cities,
                Season::Summer,
                forest_terrain,
                1600.0,
                1000.0,
                &mut rng,
            );
            assert!(
                result.robberies.is_empty(),
                "bandits should not attack near a walled city",
            );
        }
    }

    #[test]
    fn camp_starvation_destroys_camp() {
        let mut config = test_config();
        config.starvation_ticks = 10;
        config.num_camps = 1;

        let mut rng = StdRng::seed_from_u64(42);
        let mut system =
            BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        assert_eq!(system.active_camp_count(), 1);

        let mut destroyed = false;
        for _ in 0..20 {
            let result = system.tick(
                &config,
                &[],
                &[],
                Season::Spring,
                forest_terrain,
                1600.0,
                1000.0,
                &mut rng,
            );
            if !result.camps_destroyed.is_empty() {
                destroyed = true;
                break;
            }
        }
        assert!(destroyed, "camp should be destroyed after starvation");
    }

    #[test]
    fn respawn_maintains_target_count() {
        let mut config = test_config();
        config.num_camps = 3;
        config.starvation_ticks = 5;

        let mut rng = StdRng::seed_from_u64(42);
        let mut system =
            BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        assert_eq!(system.active_camp_count(), 3);

        // Run ticks to trigger starvation + respawn cycles.
        for _ in 0..30 {
            system.tick(
                &config,
                &[],
                &[],
                Season::Spring,
                forest_terrain,
                1600.0,
                1000.0,
                &mut rng,
            );
        }

        assert_eq!(system.active_camp_count(), 3);
    }

    #[test]
    fn seasonal_modifier_values() {
        let config = test_config();
        assert!((seasonal_modifier(Season::Spring, &config) - 1.0).abs() < 1e-6);
        assert!((seasonal_modifier(Season::Summer, &config) - 1.3).abs() < 1e-6);
        assert!((seasonal_modifier(Season::Autumn, &config) - 1.0).abs() < 1e-6);
        assert!((seasonal_modifier(Season::Winter, &config) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn danger_signal_deposited_on_robbery() {
        let config = test_config();
        let mut rng = StdRng::seed_from_u64(42);
        let mut system =
            BanditSystem::new(&config, &[], forest_terrain, 1600.0, 1000.0, &mut rng);

        let bandit_pos = system.camps()[0].position;
        system.bandits[0].position = bandit_pos;

        let merchants = vec![MerchantInfo {
            id: 1,
            position: bandit_pos + Vec2::new(5.0, 0.0),
            gold: 100.0,
            inventory: HashMap::new(),
            profession: Profession::Trader,
            group_size: 1,
        }];

        let result = system.tick(
            &config,
            &merchants,
            &[],
            Season::Summer,
            forest_terrain,
            1600.0,
            1000.0,
            &mut rng,
        );

        if !result.robberies.is_empty() {
            assert_eq!(result.danger_deposits.len(), result.robberies.len());
            for (_, strength) in &result.danger_deposits {
                assert!((*strength - DANGER_SIGNAL_STRENGTH).abs() < 1e-6);
            }
        }
    }
}
