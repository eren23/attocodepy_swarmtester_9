mod common;

use swarm_economy::market::crafting::{CityContext, CraftingEngine, CraftingError, CraftingJob};
use swarm_economy::types::{CityUpgrade, Commodity, Inventory};

// ── Helpers ─────────────────────────────────────────────────────────────────

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

/// Tick a job to completion without specialization, counting the number of ticks.
fn tick_to_completion(engine: &CraftingEngine, job: &mut CraftingJob, spec: Option<Commodity>) -> u32 {
    let mut ticks = 0;
    loop {
        ticks += 1;
        if let Some(_) = engine.tick_craft(job, spec) {
            return ticks;
        }
        if ticks > 1000 {
            panic!("crafting job did not complete within 1000 ticks");
        }
    }
}

// ── Tier IO Tests ───────────────────────────────────────────────────────────

#[test]
fn tier1_has_6_recipes() {
    let engine = CraftingEngine::new();
    let t1: Vec<_> = engine.recipes().iter().filter(|r| r.tier == 1).collect();
    assert_eq!(t1.len(), 6);
}

#[test]
fn tier2_has_6_recipes() {
    let engine = CraftingEngine::new();
    let t2: Vec<_> = engine.recipes().iter().filter(|r| r.tier == 2).collect();
    assert_eq!(t2.len(), 6);
}

#[test]
fn tier3_has_4_recipes() {
    let engine = CraftingEngine::new();
    let t3: Vec<_> = engine.recipes().iter().filter(|r| r.tier == 3).collect();
    assert_eq!(t3.len(), 4);
}

#[test]
fn tier1_tools_is_timber_plus_ore() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();
    assert_eq!(recipe.tier, 1);
    assert!(recipe.inputs.contains(&(Commodity::Timber, 1.0)));
    assert!(recipe.inputs.contains(&(Commodity::Ore, 1.0)));
    assert_eq!(recipe.inputs.len(), 2);
}

#[test]
fn tier1_medicine_is_grain_plus_herbs() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Medicine).unwrap();
    assert_eq!(recipe.tier, 1);
    assert!(recipe.inputs.contains(&(Commodity::Grain, 1.0)));
    assert!(recipe.inputs.contains(&(Commodity::Herbs, 1.0)));
}

#[test]
fn tier1_bricks_is_clay_plus_timber() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Bricks).unwrap();
    assert_eq!(recipe.tier, 1);
    assert!(recipe.inputs.contains(&(Commodity::Clay, 1.0)));
    assert!(recipe.inputs.contains(&(Commodity::Timber, 1.0)));
}

#[test]
fn tier1_metalwork_is_ore_plus_clay() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Metalwork).unwrap();
    assert_eq!(recipe.tier, 1);
    assert!(recipe.inputs.contains(&(Commodity::Ore, 1.0)));
    assert!(recipe.inputs.contains(&(Commodity::Clay, 1.0)));
}

#[test]
fn tier1_provisions_is_grain_plus_fish() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Provisions).unwrap();
    assert_eq!(recipe.tier, 1);
    assert!(recipe.inputs.contains(&(Commodity::Grain, 1.0)));
    assert!(recipe.inputs.contains(&(Commodity::Fish, 1.0)));
}

#[test]
fn tier1_pottery_is_herbs_plus_clay() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Pottery).unwrap();
    assert_eq!(recipe.tier, 1);
    assert!(recipe.inputs.contains(&(Commodity::Herbs, 1.0)));
    assert!(recipe.inputs.contains(&(Commodity::Clay, 1.0)));
}

// ── Insufficient Materials Rejection ────────────────────────────────────────

#[test]
fn start_craft_returns_insufficient_material_when_missing_commodity() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();

    // Only have Timber, missing Ore entirely.
    let mut inventory = inv(&[(Commodity::Timber, 1.0)]);
    let result = engine.start_craft(recipe, &mut inventory, &empty_city());

    match result {
        Err(CraftingError::InsufficientMaterial { commodity, required, available }) => {
            assert_eq!(commodity, Commodity::Ore);
            assert!((required - 1.0).abs() < f32::EPSILON);
            assert!((available - 0.0).abs() < f32::EPSILON);
        }
        other => panic!("expected InsufficientMaterial, got {:?}", other),
    }

    // Inventory should be untouched.
    assert_eq!(*inventory.get(&Commodity::Timber).unwrap(), 1.0);
}

#[test]
fn start_craft_returns_insufficient_material_when_quantity_too_low() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();

    let mut inventory = inv(&[(Commodity::Timber, 0.5), (Commodity::Ore, 1.0)]);
    let result = engine.start_craft(recipe, &mut inventory, &empty_city());

    match result {
        Err(CraftingError::InsufficientMaterial { commodity, required, available }) => {
            assert_eq!(commodity, Commodity::Timber);
            assert!((required - 1.0).abs() < f32::EPSILON);
            assert!((available - 0.5).abs() < f32::EPSILON);
        }
        other => panic!("expected InsufficientMaterial, got {:?}", other),
    }
}

// ── Duration Enforcement ────────────────────────────────────────────────────

#[test]
fn provisions_takes_3_ticks() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Provisions).unwrap();
    assert_eq!(recipe.craft_ticks, 3);

    let mut inventory = inv(&[(Commodity::Grain, 1.0), (Commodity::Fish, 1.0)]);
    let mut job = engine.start_craft(recipe, &mut inventory, &empty_city()).unwrap();
    assert_eq!(job.ticks_remaining, 3);

    // Tick 1: still in progress.
    assert!(engine.tick_craft(&mut job, None).is_none());
    assert_eq!(job.ticks_remaining, 2);

    // Tick 2: still in progress.
    assert!(engine.tick_craft(&mut job, None).is_none());
    assert_eq!(job.ticks_remaining, 1);

    // Tick 3: completes.
    let result = engine.tick_craft(&mut job, None);
    assert_eq!(result, Some((Commodity::Provisions, 1.0)));
}

#[test]
fn tools_takes_5_ticks() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();
    assert_eq!(recipe.craft_ticks, 5);

    let mut inventory = inv(&[(Commodity::Timber, 1.0), (Commodity::Ore, 1.0)]);
    let mut job = engine.start_craft(recipe, &mut inventory, &empty_city()).unwrap();

    let ticks = tick_to_completion(&engine, &mut job, None);
    assert_eq!(ticks, 5);
}

// ── City Requirement (Workshop for Tier 3) ──────────────────────────────────

#[test]
fn tier3_requires_workshop_returns_error_without_it() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::EliteGear).unwrap();

    let mut inventory = inv(&[(Commodity::Weapons, 1.0), (Commodity::Armor, 1.0)]);
    let result = engine.start_craft(recipe, &mut inventory, &empty_city());
    assert!(matches!(result, Err(CraftingError::WorkshopRequired)));

    // Inputs should not be consumed.
    assert_eq!(*inventory.get(&Commodity::Weapons).unwrap(), 1.0);
    assert_eq!(*inventory.get(&Commodity::Armor).unwrap(), 1.0);
}

#[test]
fn tier3_succeeds_with_workshop() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::EliteGear).unwrap();

    let mut inventory = inv(&[(Commodity::Weapons, 1.0), (Commodity::Armor, 1.0)]);
    let job = engine.start_craft(recipe, &mut inventory, &workshop_city());
    assert!(job.is_ok());
    assert_eq!(job.unwrap().ticks_remaining, 20);
}

#[test]
fn find_available_excludes_tier3_without_workshop() {
    let engine = CraftingEngine::new();
    let inventory = inv(&[(Commodity::Weapons, 1.0), (Commodity::Armor, 1.0)]);
    let available = engine.find_available_recipes(&inventory, &empty_city());
    assert!(available.is_empty(), "Tier3 recipes should not appear without workshop");
}

#[test]
fn find_available_includes_tier3_with_workshop() {
    let engine = CraftingEngine::new();
    let inventory = inv(&[(Commodity::Weapons, 1.0), (Commodity::Armor, 1.0)]);
    let available = engine.find_available_recipes(&inventory, &workshop_city());
    assert_eq!(available.len(), 1);
    assert_eq!(available[0].output, Commodity::EliteGear);
}

// ── Specialization Bonus ────────────────────────────────────────────────────

#[test]
fn specialization_makes_crafting_faster() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();

    // Without specialization: exactly 5 ticks.
    let mut inv1 = inv(&[(Commodity::Timber, 1.0), (Commodity::Ore, 1.0)]);
    let mut job_normal = engine.start_craft(recipe, &mut inv1, &empty_city()).unwrap();
    let normal_ticks = tick_to_completion(&engine, &mut job_normal, None);
    assert_eq!(normal_ticks, 5);

    // With specialization: should be fewer ticks (~1.5x faster).
    let mut inv2 = inv(&[(Commodity::Timber, 1.0), (Commodity::Ore, 1.0)]);
    let mut job_spec = engine.start_craft(recipe, &mut inv2, &empty_city()).unwrap();
    let spec_ticks = tick_to_completion(&engine, &mut job_spec, Some(Commodity::Tools));
    assert!(
        spec_ticks < normal_ticks,
        "specialization should reduce ticks: {} (spec) vs {} (normal)",
        spec_ticks,
        normal_ticks,
    );

    // The specialization pattern: 5(odd)->-1->4, 4(even)->-2->2, 2(even)->-2->0 = 3 ticks.
    assert_eq!(spec_ticks, 3);
}

#[test]
fn specialization_alternates_decrement() {
    let engine = CraftingEngine::new();
    // Metalwork has 8 ticks (even start).
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Metalwork).unwrap();
    assert_eq!(recipe.craft_ticks, 8);

    let mut inventory = inv(&[(Commodity::Ore, 1.0), (Commodity::Clay, 1.0)]);
    let mut job = engine.start_craft(recipe, &mut inventory, &empty_city()).unwrap();

    // 8(even)->-2->6, 6(even)->-2->4, 4(even)->-2->2, 2(even)->-2->0 = 4 ticks.
    let spec_ticks = tick_to_completion(&engine, &mut job, Some(Commodity::Metalwork));
    assert_eq!(spec_ticks, 4, "8 craft_ticks with specialization (all even) should take 4 real ticks");
}

#[test]
fn non_matching_specialization_does_not_speed_up() {
    let engine = CraftingEngine::new();
    let recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();

    let mut inventory = inv(&[(Commodity::Timber, 1.0), (Commodity::Ore, 1.0)]);
    let mut job = engine.start_craft(recipe, &mut inventory, &empty_city()).unwrap();

    // Specialization in Provisions should not speed up Tools crafting.
    let ticks = tick_to_completion(&engine, &mut job, Some(Commodity::Provisions));
    assert_eq!(ticks, 5, "non-matching specialization should take normal duration");
}

// ── All Tier 3 Require Workshop ─────────────────────────────────────────────

#[test]
fn all_tier3_recipes_have_requires_workshop() {
    let engine = CraftingEngine::new();
    let tier3: Vec<_> = engine.recipes().iter().filter(|r| r.tier == 3).collect();
    assert!(!tier3.is_empty(), "should have tier 3 recipes");
    for recipe in &tier3 {
        assert!(
            recipe.requires_workshop,
            "{:?} is tier 3 but requires_workshop is false",
            recipe.output,
        );
    }
}

#[test]
fn no_tier1_or_tier2_require_workshop() {
    let engine = CraftingEngine::new();
    for recipe in engine.recipes().iter().filter(|r| r.tier < 3) {
        assert!(
            !recipe.requires_workshop,
            "{:?} (tier {}) should not require workshop",
            recipe.output,
            recipe.tier,
        );
    }
}

// ── Concurrent Crafting ─────────────────────────────────────────────────────

#[test]
fn can_have_multiple_concurrent_crafting_jobs() {
    let engine = CraftingEngine::new();

    let mut inventory = inv(&[
        (Commodity::Timber, 5.0),
        (Commodity::Ore, 5.0),
        (Commodity::Grain, 5.0),
        (Commodity::Fish, 5.0),
        (Commodity::Clay, 5.0),
        (Commodity::Herbs, 5.0),
    ]);

    let tools_recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();
    let provisions_recipe = engine.recipes().iter().find(|r| r.output == Commodity::Provisions).unwrap();
    let pottery_recipe = engine.recipes().iter().find(|r| r.output == Commodity::Pottery).unwrap();

    let mut job1 = engine.start_craft(tools_recipe, &mut inventory, &empty_city()).unwrap();
    let mut job2 = engine.start_craft(provisions_recipe, &mut inventory, &empty_city()).unwrap();
    let mut job3 = engine.start_craft(pottery_recipe, &mut inventory, &empty_city()).unwrap();

    // All three jobs should exist concurrently.
    assert_eq!(job1.ticks_remaining, 5);
    assert_eq!(job2.ticks_remaining, 3);
    assert_eq!(job3.ticks_remaining, 4);

    // Tick all three for 3 ticks. Provisions should complete first.
    for _ in 0..2 {
        assert!(engine.tick_craft(&mut job1, None).is_none());
        assert!(engine.tick_craft(&mut job2, None).is_none());
        assert!(engine.tick_craft(&mut job3, None).is_none());
    }

    // 3rd tick: provisions completes, others still going.
    assert!(engine.tick_craft(&mut job1, None).is_none());
    let result2 = engine.tick_craft(&mut job2, None);
    assert_eq!(result2, Some((Commodity::Provisions, 1.0)));
    assert!(engine.tick_craft(&mut job3, None).is_none());

    // 4th tick: pottery completes.
    assert!(engine.tick_craft(&mut job1, None).is_none());
    let result3 = engine.tick_craft(&mut job3, None);
    assert_eq!(result3, Some((Commodity::Pottery, 1.0)));

    // 5th tick: tools completes.
    let result1 = engine.tick_craft(&mut job1, None);
    assert_eq!(result1, Some((Commodity::Tools, 1.0)));
}

#[test]
fn concurrent_jobs_consume_separate_materials() {
    let engine = CraftingEngine::new();

    let mut inventory = inv(&[
        (Commodity::Timber, 2.0),
        (Commodity::Ore, 2.0),
    ]);

    let tools_recipe = engine.recipes().iter().find(|r| r.output == Commodity::Tools).unwrap();

    // Start first job - consumes 1 Timber + 1 Ore.
    let _job1 = engine.start_craft(tools_recipe, &mut inventory, &empty_city()).unwrap();
    assert_eq!(*inventory.get(&Commodity::Timber).unwrap(), 1.0);
    assert_eq!(*inventory.get(&Commodity::Ore).unwrap(), 1.0);

    // Start second job - consumes another 1 Timber + 1 Ore.
    let _job2 = engine.start_craft(tools_recipe, &mut inventory, &empty_city()).unwrap();
    assert_eq!(*inventory.get(&Commodity::Timber).unwrap(), 0.0);
    assert_eq!(*inventory.get(&Commodity::Ore).unwrap(), 0.0);

    // Third attempt should fail - no materials left.
    let result = engine.start_craft(tools_recipe, &mut inventory, &empty_city());
    assert!(matches!(result, Err(CraftingError::InsufficientMaterial { .. })));
}

// ── Engine Creation ─────────────────────────────────────────────────────────

#[test]
fn engine_creates_with_16_recipes() {
    let engine = CraftingEngine::new();
    assert_eq!(engine.recipes().len(), 16);
}
