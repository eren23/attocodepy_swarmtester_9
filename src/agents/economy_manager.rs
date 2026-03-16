use std::collections::HashMap;

use rand::Rng;

use crate::config::EconomyConfig;
use crate::types::{Profession, Vec2};
use crate::world::city::City;

use super::merchant::Merchant;

// ── Constants ────────────────────────────────────────────────────────────────

/// Food-crisis threshold: if fewer than this fraction of cities have food,
/// trigger emergency rebalancing.
const FOOD_CRISIS_THRESHOLD: f32 = 0.20;

/// Fraction of idle + low-performing traders reassigned during emergency.
const EMERGENCY_REASSIGN_FRACTION: f32 = 0.20;

/// Transfer fraction from worst profession to best during normal rebalance.
const REBALANCE_TRANSFER_FRACTION: f32 = 0.05;

/// Window (in ticks) over which average income per profession is computed.
const INCOME_WINDOW_TICKS: u32 = 1000;

// ── Income tracker ──────────────────────────────────────────────────────────

/// Tracks per-profession income over a sliding window.
#[derive(Debug, Clone)]
struct IncomeTracker {
    /// profession → list of (tick, income) records.
    records: HashMap<Profession, Vec<(u32, f32)>>,
}

impl IncomeTracker {
    fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    fn record(&mut self, profession: Profession, tick: u32, income: f32) {
        self.records.entry(profession).or_default().push((tick, income));
    }

    /// Prune records older than `current_tick - window`.
    fn prune(&mut self, current_tick: u32, window: u32) {
        let cutoff = current_tick.saturating_sub(window);
        for records in self.records.values_mut() {
            records.retain(|&(t, _)| t >= cutoff);
        }
    }

    /// Average income for a profession over the window.
    fn avg_income(&self, profession: Profession, current_tick: u32, window: u32) -> f32 {
        let cutoff = current_tick.saturating_sub(window);
        let records = match self.records.get(&profession) {
            Some(r) => r,
            None => return 0.0,
        };
        let (sum, count) = records
            .iter()
            .filter(|&&(t, _)| t >= cutoff)
            .fold((0.0f32, 0u32), |(s, c), &(_, inc)| (s + inc, c + 1));
        if count == 0 {
            0.0
        } else {
            sum / count as f32
        }
    }
}

// ── EconomyManager ──────────────────────────────────────────────────────────

pub struct EconomyManager {
    next_merchant_id: u32,
    income_tracker: IncomeTracker,
    /// Total gold in the economy at last measurement.
    pub total_gold_history: Vec<f32>,
}

impl EconomyManager {
    pub fn new(starting_id: u32) -> Self {
        Self {
            next_merchant_id: starting_id,
            income_tracker: IncomeTracker::new(),
            total_gold_history: Vec::new(),
        }
    }

    // ── Initial population ──────────────────────────────────────────────

    /// Spawn the initial merchant population distributed across cities
    /// according to the configured profession distribution.
    pub fn spawn_initial_population(
        &mut self,
        config: &EconomyConfig,
        cities: &[City],
        rng: &mut impl Rng,
    ) -> Vec<Merchant> {
        let count = config.merchant.initial_population;
        let mut merchants = Vec::with_capacity(count as usize);

        for _ in 0..count {
            let profession = self.pick_profession_by_distribution(config);
            let city = &cities[rng.gen_range(0..cities.len())];
            let merchant = self.spawn_one(config, city, profession, rng);
            merchants.push(merchant);
        }

        merchants
    }

    // ── Per-tick economy management ─────────────────────────────────────

    /// Run economy management for one tick: spawn new merchants, handle
    /// bankruptcies, rebalance professions, and track gold conservation.
    pub fn tick(
        &mut self,
        merchants: &mut Vec<Merchant>,
        cities: &[City],
        config: &EconomyConfig,
        current_tick: u32,
        rng: &mut impl Rng,
    ) {
        // 1. Record income for living merchants.
        self.record_incomes(merchants, current_tick);

        // 2. Remove bankrupt merchants.
        self.remove_bankrupt(merchants, config);

        // 3. Spawn new merchants if economy is healthy and below max.
        self.try_spawn(merchants, cities, config, rng);

        // 4. Periodic rebalancing.
        if current_tick > 0 && current_tick % config.professions.rebalance_interval == 0 {
            self.income_tracker.prune(current_tick, INCOME_WINDOW_TICKS);

            // Check for food emergency first.
            if self.is_food_crisis(cities) {
                self.emergency_rebalance(merchants, rng);
            } else {
                self.normal_rebalance(merchants, current_tick, rng);
            }
        }

        // 5. Track total gold.
        let total_gold: f32 = merchants.iter().filter(|m| m.alive).map(|m| m.gold).sum();
        self.total_gold_history.push(total_gold);
        // Keep only last 1000 measurements.
        if self.total_gold_history.len() > 1000 {
            self.total_gold_history.remove(0);
        }
    }

    // ── Spawning ────────────────────────────────────────────────────────

    /// Spawn new merchants at the configured spawn rate when below max_population
    /// and the economy is healthy (average gold > 50% of initial_gold).
    fn try_spawn(
        &mut self,
        merchants: &mut Vec<Merchant>,
        cities: &[City],
        config: &EconomyConfig,
        rng: &mut impl Rng,
    ) {
        let alive_count = merchants.iter().filter(|m| m.alive).count() as u32;
        if alive_count >= config.merchant.max_population {
            return;
        }

        // Economy health check: average gold must be above half of initial.
        let avg_gold = if alive_count > 0 {
            let total: f32 = merchants.iter().filter(|m| m.alive).map(|m| m.gold).sum();
            total / alive_count as f32
        } else {
            0.0
        };

        if avg_gold < config.merchant.initial_gold * 0.5 && alive_count > 0 {
            return; // Economy not healthy enough to spawn.
        }

        // Probabilistic spawn: spawn_rate chance per tick.
        if rng.gen::<f32>() < config.merchant.spawn_rate {
            let profession = self.pick_profession_by_need(merchants, config);
            let city = &cities[rng.gen_range(0..cities.len())];
            let merchant = self.spawn_one(config, city, profession, rng);
            merchants.push(merchant);
        }
    }

    /// Spawn a single merchant at a city.
    fn spawn_one(
        &mut self,
        config: &EconomyConfig,
        city: &City,
        profession: Profession,
        rng: &mut impl Rng,
    ) -> Merchant {
        let id = self.next_merchant_id;
        self.next_merchant_id += 1;

        // Offset position slightly from city center.
        let angle = rng.gen_range(0.0..std::f32::consts::TAU);
        let dist = rng.gen_range(0.0..city.radius * 0.5);
        let pos = city.position + Vec2::from_angle(angle) * dist;

        Merchant::new(id, pos, city.id, profession, &config.merchant, rng)
    }

    // ── Bankruptcy ──────────────────────────────────────────────────────

    /// Tick bankruptcy for all merchants and mark dead ones.
    fn remove_bankrupt(&self, merchants: &mut Vec<Merchant>, config: &EconomyConfig) {
        for merchant in merchants.iter_mut() {
            if merchant.alive {
                merchant.tick_bankruptcy(config.merchant.bankruptcy_grace_ticks);
            }
        }
    }

    // ── Income tracking ─────────────────────────────────────────────────

    /// Record each merchant's current gold as an income data point.
    fn record_incomes(&mut self, merchants: &[Merchant], tick: u32) {
        for m in merchants.iter().filter(|m| m.alive) {
            self.income_tracker.record(m.profession, tick, m.gold);
        }
    }

    // ── Profession selection ────────────────────────────────────────────

    /// Pick a profession based on the default distribution from config.
    fn pick_profession_by_distribution(&self, config: &EconomyConfig) -> Profession {
        // Use a deterministic mapping from distribution keys.
        // The config stores strings, so we map them.
        let dist = &config.professions.default_distribution;
        let trader = dist.get("trader").copied().unwrap_or(0.4);
        let miner = dist.get("miner").copied().unwrap_or(0.12);
        let farmer = dist.get("farmer").copied().unwrap_or(0.10);
        let craftsman = dist.get("craftsman").copied().unwrap_or(0.18);
        let soldier = dist.get("soldier").copied().unwrap_or(0.08);
        let shipwright = dist.get("shipwright").copied().unwrap_or(0.05);
        // idle gets the remainder

        // Build cumulative distribution.
        let r: f32 = rand::random();
        let mut cum = 0.0;
        for (prof, weight) in [
            (Profession::Trader, trader),
            (Profession::Miner, miner),
            (Profession::Farmer, farmer),
            (Profession::Craftsman, craftsman),
            (Profession::Soldier, soldier),
            (Profession::Shipwright, shipwright),
        ] {
            cum += weight;
            if r < cum {
                return prof;
            }
        }
        Profession::Idle
    }

    /// Pick a profession based on what's most needed (largest gap between
    /// desired distribution and actual count).
    fn pick_profession_by_need(
        &self,
        merchants: &[Merchant],
        config: &EconomyConfig,
    ) -> Profession {
        let alive: Vec<&Merchant> = merchants.iter().filter(|m| m.alive).collect();
        let total = alive.len().max(1) as f32;

        let mut counts: HashMap<Profession, u32> = HashMap::new();
        for m in &alive {
            *counts.entry(m.profession).or_insert(0) += 1;
        }

        let dist = &config.professions.default_distribution;
        let mut best_prof = Profession::Idle;
        let mut best_deficit = f32::NEG_INFINITY;

        for prof in Profession::ALL {
            let key = profession_key(prof);
            let desired_frac = dist.get(key).copied().unwrap_or(0.0);
            let actual_frac = *counts.get(&prof).unwrap_or(&0) as f32 / total;
            let deficit = desired_frac - actual_frac;
            if deficit > best_deficit {
                best_deficit = deficit;
                best_prof = prof;
            }
        }

        best_prof
    }

    // ── Rebalancing ─────────────────────────────────────────────────────

    /// Normal rebalance: compute average income per profession over the last
    /// `INCOME_WINDOW_TICKS`, transfer `REBALANCE_TRANSFER_FRACTION` of the
    /// worst-performing profession to the best, and assign idle merchants
    /// to the most-needed profession.
    fn normal_rebalance(
        &mut self,
        merchants: &mut [Merchant],
        current_tick: u32,
        rng: &mut impl Rng,
    ) {
        // Compute avg income per profession.
        let mut incomes: Vec<(Profession, f32)> = Profession::ALL
            .iter()
            .map(|&p| (p, self.income_tracker.avg_income(p, current_tick, INCOME_WINDOW_TICKS)))
            .collect();

        // Sort by income ascending.
        incomes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let worst = incomes.first().map(|x| x.0).unwrap_or(Profession::Idle);
        let best = incomes.last().map(|x| x.0).unwrap_or(Profession::Trader);

        if worst != best {
            // Count merchants in the worst profession.
            let worst_count = merchants
                .iter()
                .filter(|m| m.alive && m.profession == worst)
                .count();
            let transfer_count = ((worst_count as f32 * REBALANCE_TRANSFER_FRACTION).ceil() as usize).max(1);

            let mut transferred = 0;
            for m in merchants.iter_mut() {
                if transferred >= transfer_count {
                    break;
                }
                if m.alive && m.profession == worst {
                    m.profession = best;
                    transferred += 1;
                }
            }
        }

        // Assign idle merchants to the most-needed profession.
        let most_needed = self.find_most_needed_profession(merchants);
        for m in merchants.iter_mut() {
            if m.alive && m.profession == Profession::Idle {
                // Only reassign with some probability to avoid all switching at once.
                if rng.gen::<f32>() < 0.3 {
                    m.profession = most_needed;
                }
            }
        }
    }

    /// Emergency rebalance: if < 20% of cities have food, force 20% of
    /// idle + low-performing traders to become farmers.
    fn emergency_rebalance(&self, merchants: &mut [Merchant], rng: &mut impl Rng) {
        // Collect indices of idle and low-gold traders.
        let mut candidates: Vec<usize> = Vec::new();
        let avg_gold = {
            let alive: Vec<&Merchant> = merchants.iter().filter(|m| m.alive).collect();
            if alive.is_empty() {
                return;
            }
            let total: f32 = alive.iter().map(|m| m.gold).sum();
            total / alive.len() as f32
        };

        for (i, m) in merchants.iter().enumerate() {
            if !m.alive {
                continue;
            }
            if m.profession == Profession::Idle {
                candidates.push(i);
            } else if m.profession == Profession::Trader && m.gold < avg_gold * 0.5 {
                candidates.push(i);
            }
        }

        let reassign_count =
            ((candidates.len() as f32 * EMERGENCY_REASSIGN_FRACTION).ceil() as usize).max(1);

        // Shuffle candidates to randomize who gets reassigned.
        for i in (1..candidates.len()).rev() {
            let j = rng.gen_range(0..=i);
            candidates.swap(i, j);
        }

        for &idx in candidates.iter().take(reassign_count) {
            merchants[idx].profession = Profession::Farmer;
        }
    }

    /// Check if there's a food crisis: < 20% of cities have any food.
    fn is_food_crisis(&self, cities: &[City]) -> bool {
        if cities.is_empty() {
            return false;
        }
        let cities_with_food = cities
            .iter()
            .filter(|c| {
                c.warehouse.get(&crate::types::Commodity::Grain).copied().unwrap_or(0.0) > 0.0
                    || c.warehouse
                        .get(&crate::types::Commodity::Fish)
                        .copied()
                        .unwrap_or(0.0)
                        > 0.0
                    || c.warehouse
                        .get(&crate::types::Commodity::Provisions)
                        .copied()
                        .unwrap_or(0.0)
                        > 0.0
            })
            .count();
        (cities_with_food as f32 / cities.len() as f32) < FOOD_CRISIS_THRESHOLD
    }

    /// Find the profession with the largest deficit relative to desired distribution.
    fn find_most_needed_profession(&self, merchants: &[Merchant]) -> Profession {
        let alive: Vec<&Merchant> = merchants.iter().filter(|m| m.alive).collect();
        let total = alive.len().max(1) as f32;

        let mut counts: HashMap<Profession, u32> = HashMap::new();
        for m in &alive {
            *counts.entry(m.profession).or_insert(0) += 1;
        }

        // Use default target distribution.
        let targets = [
            (Profession::Trader, 0.40),
            (Profession::Miner, 0.12),
            (Profession::Farmer, 0.10),
            (Profession::Craftsman, 0.18),
            (Profession::Soldier, 0.08),
            (Profession::Shipwright, 0.05),
        ];

        let mut best_prof = Profession::Trader;
        let mut best_deficit = f32::NEG_INFINITY;

        for (prof, target) in targets {
            let actual = *counts.get(&prof).unwrap_or(&0) as f32 / total;
            let deficit = target - actual;
            if deficit > best_deficit {
                best_deficit = deficit;
                best_prof = prof;
            }
        }

        best_prof
    }

    // ── Accessors ───────────────────────────────────────────────────────

    pub fn next_id(&self) -> u32 {
        self.next_merchant_id
    }

    /// Count alive merchants by profession.
    pub fn profession_counts(merchants: &[Merchant]) -> HashMap<Profession, u32> {
        let mut counts = HashMap::new();
        for m in merchants.iter().filter(|m| m.alive) {
            *counts.entry(m.profession).or_insert(0) += 1;
        }
        counts
    }

    /// Total alive merchant count.
    pub fn alive_count(merchants: &[Merchant]) -> u32 {
        merchants.iter().filter(|m| m.alive).count() as u32
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn profession_key(prof: Profession) -> &'static str {
    match prof {
        Profession::Trader => "trader",
        Profession::Miner => "miner",
        Profession::Farmer => "farmer",
        Profession::Craftsman => "craftsman",
        Profession::Soldier => "soldier",
        Profession::Shipwright => "shipwright",
        Profession::Idle => "idle",
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EconomyConfig;
    use crate::world::terrain::Terrain;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn setup() -> (EconomyConfig, Vec<City>) {
        let cfg = EconomyConfig::load("economy_config.toml").expect("test config");
        let terrain = Terrain::new(&cfg.world);
        let mut rng = StdRng::seed_from_u64(42);
        let cities = City::generate(
            &cfg.world,
            &cfg.city,
            |pos| {
                let tx = (pos.x as u32).min(terrain.width().saturating_sub(1));
                let ty = (pos.y as u32).min(terrain.height().saturating_sub(1));
                terrain.terrain_at(tx, ty)
            },
            &mut rng,
        );
        (cfg, cities)
    }

    #[test]
    fn initial_population_count() {
        let (cfg, cities) = setup();
        let mut rng = StdRng::seed_from_u64(99);
        let mut mgr = EconomyManager::new(0);
        let merchants = mgr.spawn_initial_population(&cfg, &cities, &mut rng);
        assert_eq!(merchants.len(), cfg.merchant.initial_population as usize);
    }

    #[test]
    fn initial_population_has_all_professions() {
        let (cfg, cities) = setup();
        let mut rng = StdRng::seed_from_u64(99);
        let mut mgr = EconomyManager::new(0);
        let merchants = mgr.spawn_initial_population(&cfg, &cities, &mut rng);

        let counts = EconomyManager::profession_counts(&merchants);
        // With 200 merchants, we should have at least one of the major professions.
        assert!(counts.get(&Profession::Trader).copied().unwrap_or(0) > 0);
        assert!(counts.get(&Profession::Miner).copied().unwrap_or(0) > 0);
        assert!(counts.get(&Profession::Craftsman).copied().unwrap_or(0) > 0);
    }

    #[test]
    fn merchants_get_unique_ids() {
        let (cfg, cities) = setup();
        let mut rng = StdRng::seed_from_u64(99);
        let mut mgr = EconomyManager::new(0);
        let merchants = mgr.spawn_initial_population(&cfg, &cities, &mut rng);
        let mut ids: Vec<u32> = merchants.iter().map(|m| m.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), merchants.len());
    }

    #[test]
    fn bankrupt_merchants_removed() {
        let (cfg, cities) = setup();
        let mut rng = StdRng::seed_from_u64(99);
        let mut mgr = EconomyManager::new(0);
        let mut merchants = mgr.spawn_initial_population(&cfg, &cities, &mut rng);

        // Force one merchant into bankruptcy.
        merchants[0].gold = -100.0;
        for _ in 0..cfg.merchant.bankruptcy_grace_ticks + 1 {
            mgr.tick(&mut merchants, &cities, &cfg, 0, &mut rng);
        }
        assert!(!merchants[0].alive);
    }

    #[test]
    fn food_crisis_detection() {
        let (_cfg, cities) = setup();
        let mgr = EconomyManager::new(0);
        // No food in any city — should be a crisis.
        assert!(mgr.is_food_crisis(&cities));
    }

    #[test]
    fn spawn_respects_max_population() {
        let (cfg, cities) = setup();
        let mut rng = StdRng::seed_from_u64(99);
        let mut mgr = EconomyManager::new(0);
        let mut merchants = mgr.spawn_initial_population(&cfg, &cities, &mut rng);

        // Set all merchants to max_population worth of alive.
        assert!(EconomyManager::alive_count(&merchants) <= cfg.merchant.max_population);

        // Increase population to max.
        while EconomyManager::alive_count(&merchants) < cfg.merchant.max_population {
            let city = &cities[0];
            let m = mgr.spawn_one(&cfg, city, Profession::Trader, &mut rng);
            merchants.push(m);
        }

        for tick in 0..100 {
            mgr.tick(&mut merchants, &cities, &cfg, tick, &mut rng);
        }
        // No new merchants should have been spawned (though some may have died).
        let alive_after = EconomyManager::alive_count(&merchants);
        assert!(alive_after <= cfg.merchant.max_population);
    }

    #[test]
    fn gold_conservation_tracked() {
        let (cfg, cities) = setup();
        let mut rng = StdRng::seed_from_u64(99);
        let mut mgr = EconomyManager::new(0);
        let mut merchants = mgr.spawn_initial_population(&cfg, &cities, &mut rng);

        mgr.tick(&mut merchants, &cities, &cfg, 1, &mut rng);
        assert!(!mgr.total_gold_history.is_empty());
    }
}
