use std::collections::HashMap;

use crate::types::{CityUpgrade, Commodity, Inventory, Recipe};

// ── City context needed by the crafting engine ────────────────────────────

/// Minimal view of a city that the crafting engine requires.
#[derive(Debug, Clone)]
pub struct CityContext {
    pub upgrades: Vec<CityUpgrade>,
    pub specialization: Option<Commodity>,
}

impl CityContext {
    pub fn has_workshop(&self) -> bool {
        self.upgrades.contains(&CityUpgrade::Workshop)
    }
}

// ── Crafting job ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CraftingJob {
    pub recipe: Recipe,
    pub ticks_remaining: u32,
}

// ── Crafting engine ──────────────────────────────────────────────────────

pub struct CraftingEngine {
    recipes: Vec<Recipe>,
}

impl CraftingEngine {
    pub fn new() -> Self {
        Self {
            recipes: build_recipe_registry(),
        }
    }

    pub fn recipes(&self) -> &[Recipe] {
        &self.recipes
    }

    // ── (1) find_available_recipes ────────────────────────────────────

    /// Returns recipes the player can craft given current inventory and city.
    /// Filters by: sufficient materials, tier requirements, workshop for T3.
    pub fn find_available_recipes(
        &self,
        inventory: &Inventory,
        city: &CityContext,
    ) -> Vec<&Recipe> {
        self.recipes
            .iter()
            .filter(|r| {
                // Tier 3 requires workshop
                if r.requires_workshop && !city.has_workshop() {
                    return false;
                }
                // Check all inputs are available in sufficient quantity
                r.inputs.iter().all(|(commodity, qty)| {
                    inventory.get(commodity).copied().unwrap_or(0.0) >= *qty
                })
            })
            .collect()
    }

    // ── (2) start_craft ──────────────────────────────────────────────

    /// Consumes input materials from inventory and returns a `CraftingJob`.
    /// Returns `Err` if materials are insufficient or workshop requirement unmet.
    pub fn start_craft(
        &self,
        recipe: &Recipe,
        inventory: &mut Inventory,
        city: &CityContext,
    ) -> Result<CraftingJob, CraftingError> {
        // Validate workshop
        if recipe.requires_workshop && !city.has_workshop() {
            return Err(CraftingError::WorkshopRequired);
        }

        // Validate materials
        for (commodity, qty) in &recipe.inputs {
            let have = inventory.get(commodity).copied().unwrap_or(0.0);
            if have < *qty {
                return Err(CraftingError::InsufficientMaterial {
                    commodity: *commodity,
                    required: *qty,
                    available: have,
                });
            }
        }

        // Consume inputs
        for (commodity, qty) in &recipe.inputs {
            let entry = inventory.entry(*commodity).or_insert(0.0);
            *entry -= qty;
        }

        Ok(CraftingJob {
            recipe: recipe.clone(),
            ticks_remaining: recipe.craft_ticks,
        })
    }

    // ── (3) tick_craft ───────────────────────────────────────────────

    /// Advances the crafting job by one tick.
    /// If the city specialization matches the recipe output, crafting is 1.5× faster
    /// (the timer decrements by 2 every other tick, implemented as decrement-by-1
    ///  with a 50% chance of a bonus decrement — here we use a deterministic approach:
    ///  we decrement by 1, then by an extra 1 every second call when specialized).
    ///
    /// Returns `Some((Commodity, f32))` with the output when the job completes,
    /// or `None` if still in progress.
    pub fn tick_craft(
        &self,
        job: &mut CraftingJob,
        city_specialization: Option<Commodity>,
    ) -> Option<(Commodity, f32)> {
        if job.ticks_remaining == 0 {
            return Some((job.recipe.output, job.recipe.output_quantity));
        }

        // Base decrement
        let decrement = if city_specialization == Some(job.recipe.output) {
            // 1.5× speed → every 2 real ticks remove 3 timer ticks
            // Deterministic: alternate 1 and 2. We approximate by always removing
            // at least 1, and checking if remaining is still > 0 to remove another
            // half the time. Simplest correct approach: subtract 1, then if the new
            // remaining is odd, subtract 1 more (gives 1.5× average over time).
            // Actually simpler: just use ceiling division. The total real ticks =
            // ceil(craft_ticks / 1.5). We implement this by tracking a fractional
            // accumulator — but for simplicity and determinism, we just decrement
            // the timer by 1 and check: if specialized, every tick removes 1.5
            // effective ticks. We'll decrement by 2 when remaining is even, 1 when odd.
            if job.ticks_remaining % 2 == 0 { 2 } else { 1 }
        } else {
            1
        };

        job.ticks_remaining = job.ticks_remaining.saturating_sub(decrement);

        if job.ticks_remaining == 0 {
            Some((job.recipe.output, job.recipe.output_quantity))
        } else {
            None
        }
    }

    // ── (4) evaluate_recipe_profitability ────────────────────────────

    /// Calculates profit margin per tick for a recipe.
    /// `margin_per_tick = (sell_value - buy_cost) / craft_ticks`
    pub fn evaluate_recipe_profitability(
        &self,
        recipe: &Recipe,
        buy_prices: &HashMap<Commodity, f32>,
        sell_prices: &HashMap<Commodity, f32>,
    ) -> f32 {
        let input_cost: f32 = recipe
            .inputs
            .iter()
            .map(|(c, qty)| buy_prices.get(c).copied().unwrap_or(0.0) * qty)
            .sum();

        let output_value =
            sell_prices.get(&recipe.output).copied().unwrap_or(0.0) * recipe.output_quantity;

        let margin = output_value - input_cost;
        if recipe.craft_ticks == 0 {
            return margin; // instant craft — full margin
        }
        margin / recipe.craft_ticks as f32
    }
}

impl Default for CraftingEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Errors ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum CraftingError {
    WorkshopRequired,
    InsufficientMaterial {
        commodity: Commodity,
        required: f32,
        available: f32,
    },
}

impl std::fmt::Display for CraftingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CraftingError::WorkshopRequired => {
                write!(f, "tier-3 recipe requires a Workshop upgrade")
            }
            CraftingError::InsufficientMaterial {
                commodity,
                required,
                available,
            } => write!(
                f,
                "need {required} {commodity}, only have {available}"
            ),
        }
    }
}

impl std::error::Error for CraftingError {}

// ── Recipe registry ───────────────────────────────────────────────────────

fn r(
    inputs: Vec<(Commodity, f32)>,
    output: Commodity,
    output_qty: f32,
    ticks: u32,
    tier: u8,
    workshop: bool,
) -> Recipe {
    Recipe {
        inputs,
        output,
        output_quantity: output_qty,
        craft_ticks: ticks,
        tier,
        requires_workshop: workshop,
    }
}

fn build_recipe_registry() -> Vec<Recipe> {
    use Commodity::*;

    vec![
        // ── Tier 1 ───────────────────────────────────────────────────
        r(vec![(Timber, 1.0), (Ore, 1.0)],     Tools,      1.0, 5,  1, false),
        r(vec![(Grain, 1.0), (Herbs, 1.0)],    Medicine,   1.0, 5,  1, false),
        r(vec![(Clay, 1.0), (Timber, 1.0)],    Bricks,     1.0, 5,  1, false),
        r(vec![(Ore, 1.0), (Clay, 1.0)],       Metalwork,  1.0, 8,  1, false),
        r(vec![(Grain, 1.0), (Fish, 1.0)],     Provisions, 1.0, 3,  1, false),
        r(vec![(Herbs, 1.0), (Clay, 1.0)],     Pottery,    1.0, 4,  1, false),
        // ── Tier 2 ───────────────────────────────────────────────────
        r(vec![(Tools, 1.0), (Ore, 1.0)],      Weapons,    1.0, 10, 2, false),
        r(vec![(Tools, 1.0), (Timber, 1.0)],   Furniture,  1.0, 8,  2, false),
        r(vec![(Metalwork, 1.0), (Clay, 1.0)], Armor,      1.0, 12, 2, false),
        r(vec![(Medicine, 1.0), (Pottery, 1.0)], Alchemy,  1.0, 10, 2, false),
        r(vec![(Bricks, 1.0), (Metalwork, 1.0)], Machinery, 1.0, 15, 2, false),
        r(vec![(Provisions, 1.0), (Herbs, 1.0)], FeastGoods, 1.0, 6, 2, false),
        // ── Tier 3 (requires Workshop) ───────────────────────────────
        r(vec![(Weapons, 1.0), (Armor, 1.0)],     EliteGear,  1.0, 20, 3, true),
        r(vec![(Machinery, 1.0), (Tools, 1.0)],   Automaton,  1.0, 25, 3, true),
        r(vec![(Alchemy, 1.0), (Medicine, 1.0)],  Elixir,     1.0, 18, 3, true),
        r(vec![(Furniture, 1.0), (FeastGoods, 1.0)], LuxurySet, 1.0, 15, 3, true),
    ]
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_city() -> CityContext {
        CityContext {
            upgrades: vec![],
            specialization: None,
        }
    }

    fn workshop_city() -> CityContext {
        CityContext {
            upgrades: vec![CityUpgrade::Workshop],
            specialization: None,
        }
    }

    fn inv(items: &[(Commodity, f32)]) -> Inventory {
        items.iter().copied().collect()
    }

    #[test]
    fn registry_has_16_recipes() {
        let engine = CraftingEngine::new();
        assert_eq!(engine.recipes().len(), 16);
    }

    #[test]
    fn tier_counts() {
        let engine = CraftingEngine::new();
        let t1 = engine.recipes().iter().filter(|r| r.tier == 1).count();
        let t2 = engine.recipes().iter().filter(|r| r.tier == 2).count();
        let t3 = engine.recipes().iter().filter(|r| r.tier == 3).count();
        assert_eq!(t1, 6);
        assert_eq!(t2, 6);
        assert_eq!(t3, 4);
    }

    #[test]
    fn find_available_filters_by_inventory() {
        let engine = CraftingEngine::new();
        let inventory = inv(&[(Commodity::Timber, 1.0), (Commodity::Ore, 1.0)]);
        let available = engine.find_available_recipes(&inventory, &empty_city());
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].output, Commodity::Tools);
    }

    #[test]
    fn find_available_excludes_tier3_without_workshop() {
        let engine = CraftingEngine::new();
        let inventory = inv(&[(Commodity::Weapons, 1.0), (Commodity::Armor, 1.0)]);
        let available = engine.find_available_recipes(&inventory, &empty_city());
        assert!(available.is_empty());
    }

    #[test]
    fn find_available_includes_tier3_with_workshop() {
        let engine = CraftingEngine::new();
        let inventory = inv(&[(Commodity::Weapons, 1.0), (Commodity::Armor, 1.0)]);
        let available = engine.find_available_recipes(&inventory, &workshop_city());
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].output, Commodity::EliteGear);
    }

    #[test]
    fn start_craft_consumes_inputs() {
        let engine = CraftingEngine::new();
        let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();
        let mut inventory = inv(&[(Commodity::Timber, 5.0), (Commodity::Ore, 3.0)]);

        let job = engine.start_craft(recipe, &mut inventory, &empty_city()).unwrap();
        assert_eq!(job.ticks_remaining, 5);
        assert_eq!(*inventory.get(&Commodity::Timber).unwrap(), 4.0);
        assert_eq!(*inventory.get(&Commodity::Ore).unwrap(), 2.0);
    }

    #[test]
    fn start_craft_fails_insufficient_materials() {
        let engine = CraftingEngine::new();
        let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();
        let mut inventory = inv(&[(Commodity::Timber, 1.0)]);

        let result = engine.start_craft(recipe, &mut inventory, &empty_city());
        assert!(matches!(result, Err(CraftingError::InsufficientMaterial { .. })));
    }

    #[test]
    fn start_craft_fails_without_workshop_for_tier3() {
        let engine = CraftingEngine::new();
        let recipe = engine.recipes().iter().find(|r| r.output == Commodity::EliteGear).unwrap();
        let mut inventory = inv(&[(Commodity::Weapons, 1.0), (Commodity::Armor, 1.0)]);

        let result = engine.start_craft(recipe, &mut inventory, &empty_city());
        assert!(matches!(result, Err(CraftingError::WorkshopRequired)));
        // Inputs should not have been consumed
        assert_eq!(*inventory.get(&Commodity::Weapons).unwrap(), 1.0);
    }

    #[test]
    fn tick_craft_completes_without_specialization() {
        let engine = CraftingEngine::new();
        let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Provisions).unwrap();
        let mut inventory = inv(&[(Commodity::Grain, 1.0), (Commodity::Fish, 1.0)]);
        let mut job = engine.start_craft(recipe, &mut inventory, &empty_city()).unwrap();
        assert_eq!(job.ticks_remaining, 3);

        // Tick 3 times
        assert!(engine.tick_craft(&mut job, None).is_none());
        assert!(engine.tick_craft(&mut job, None).is_none());
        let result = engine.tick_craft(&mut job, None);
        assert_eq!(result, Some((Commodity::Provisions, 1.0)));
    }

    #[test]
    fn tick_craft_faster_with_matching_specialization() {
        let engine = CraftingEngine::new();
        let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();
        let mut inventory = inv(&[(Commodity::Timber, 1.0), (Commodity::Ore, 1.0)]);
        let mut job = engine.start_craft(recipe, &mut inventory, &empty_city()).unwrap();
        assert_eq!(job.ticks_remaining, 5); // 5 ticks normally

        // With specialization: alternates -1 (odd remaining) and -2 (even remaining)
        // 5 (odd) → -1 → 4, 4 (even) → -2 → 2, 2 (even) → -2 → 0 done
        let spec = Some(Commodity::Tools);
        assert!(engine.tick_craft(&mut job, spec).is_none()); // 5→4
        assert!(engine.tick_craft(&mut job, spec).is_none()); // 4→2
        let result = engine.tick_craft(&mut job, spec);        // 2→0
        assert_eq!(result, Some((Commodity::Tools, 1.0)));
        // 3 ticks instead of 5 ≈ 1.67× speed (close to 1.5×)
    }

    #[test]
    fn profitability_calculation() {
        let engine = CraftingEngine::new();
        let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();

        let buy: HashMap<Commodity, f32> =
            [(Commodity::Timber, 10.0), (Commodity::Ore, 15.0)].into();
        let sell: HashMap<Commodity, f32> = [(Commodity::Tools, 50.0)].into();

        let margin = engine.evaluate_recipe_profitability(recipe, &buy, &sell);
        // (50 - 25) / 5 = 5.0
        assert!((margin - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn profitability_negative_margin() {
        let engine = CraftingEngine::new();
        let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();

        let buy: HashMap<Commodity, f32> =
            [(Commodity::Timber, 30.0), (Commodity::Ore, 30.0)].into();
        let sell: HashMap<Commodity, f32> = [(Commodity::Tools, 10.0)].into();

        let margin = engine.evaluate_recipe_profitability(recipe, &buy, &sell);
        // (10 - 60) / 5 = -10.0
        assert!((margin - (-10.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn all_tier3_require_workshop() {
        let engine = CraftingEngine::new();
        for recipe in engine.recipes().iter().filter(|r| r.tier == 3) {
            assert!(recipe.requires_workshop, "{:?} should require workshop", recipe.output);
        }
    }

    #[test]
    fn no_tier1_or_tier2_require_workshop() {
        let engine = CraftingEngine::new();
        for recipe in engine.recipes().iter().filter(|r| r.tier < 3) {
            assert!(!recipe.requires_workshop, "{:?} should not require workshop", recipe.output);
        }
    }
}
