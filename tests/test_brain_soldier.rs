mod common;

use swarm_economy::brain;
use swarm_economy::brain::interface::Brain;
use swarm_economy::types::*;

use common::*;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn soldier_brain() -> Box<dyn Brain> {
    brain::brain_for_profession(Profession::Soldier)
}

// ── Patrol route ────────────────────────────────────────────────────────────

#[test]
fn patrol_moves_with_reduced_speed_and_follows_roads() {
    let brain = soldier_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    merchant.state = AgentState::Patrolling;

    // Set up terrain rays with road values to follow
    let mut sensory = default_sensory_input();
    sensory.fatigue = 10.0;
    sensory.nearest_bandit = None;

    // Give the terrain rays some road values so the soldier follows roads
    for ray in sensory.terrain_rays.iter_mut() {
        ray.road_value = 0.5;
    }

    let action = brain.decide(&sensory, &mut merchant);

    assert!(
        action.speed_mult < 1.0,
        "patrolling soldier should move at reduced speed, got {}",
        action.speed_mult
    );
    assert_eq!(
        merchant.state,
        AgentState::Patrolling,
        "should remain in Patrolling state with no threats"
    );
}

// ── Escort transition ───────────────────────────────────────────────────────

#[test]
fn escort_transition_when_caravan_nearby_and_sociable() {
    let brain = soldier_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    merchant.state = AgentState::Patrolling;
    merchant.traits.sociability = 0.5; // > 0.3 threshold

    let mut sensory = default_sensory_input();
    sensory.fatigue = 10.0;
    sensory.nearest_bandit = None;

    // Add a neighbor with a caravan_id (non-soldier)
    sensory.neighbors.push(NeighborInfo {
        relative_pos: Vec2::new(5.0, 0.0),
        profession: Profession::Trader,
        inventory_fullness: 0.5,
        reputation: 50.0,
        caravan_id: Some(1),
    });

    let _action = brain.decide(&sensory, &mut merchant);

    assert_eq!(
        merchant.state,
        AgentState::Escorting,
        "soldier should transition to Escorting when caravan neighbor present and sociability > 0.3"
    );
}

#[test]
fn no_escort_transition_when_low_sociability() {
    let brain = soldier_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    merchant.state = AgentState::Patrolling;
    merchant.traits.sociability = 0.1; // < 0.3 threshold

    let mut sensory = default_sensory_input();
    sensory.fatigue = 10.0;
    sensory.nearest_bandit = None;

    sensory.neighbors.push(NeighborInfo {
        relative_pos: Vec2::new(5.0, 0.0),
        profession: Profession::Trader,
        inventory_fullness: 0.5,
        reputation: 50.0,
        caravan_id: Some(1),
    });

    let _action = brain.decide(&sensory, &mut merchant);

    assert_eq!(
        merchant.state,
        AgentState::Patrolling,
        "low-sociability soldier should NOT transition to Escorting"
    );
}

// ── Combat trigger ──────────────────────────────────────────────────────────

#[test]
fn combat_trigger_when_bandit_within_engage_range() {
    let brain = soldier_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    merchant.state = AgentState::Patrolling;
    merchant.traits.risk_tolerance = 0.5;

    // engage_range = 30 + 0.5 * 20 = 40
    let engage_range = 30.0 + merchant.traits.risk_tolerance * 20.0;

    let mut sensory = default_sensory_input();
    sensory.fatigue = 10.0;
    // Place bandit just inside engage range
    sensory.nearest_bandit = Some((Vec2::new(1.0, 0.0), engage_range - 1.0));

    let _action = brain.decide(&sensory, &mut merchant);

    assert_eq!(
        merchant.state,
        AgentState::Fighting,
        "soldier should enter Fighting when bandit within engage range ({})",
        engage_range
    );
}

#[test]
fn no_combat_trigger_when_bandit_outside_engage_range() {
    let brain = soldier_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    merchant.state = AgentState::Patrolling;
    merchant.traits.risk_tolerance = 0.0;

    // engage_range = 30 + 0.0 * 20 = 30
    let engage_range = 30.0 + merchant.traits.risk_tolerance * 20.0;

    let mut sensory = default_sensory_input();
    sensory.fatigue = 10.0;
    // Place bandit just outside engage range
    sensory.nearest_bandit = Some((Vec2::new(1.0, 0.0), engage_range + 5.0));

    let _action = brain.decide(&sensory, &mut merchant);

    assert_eq!(
        merchant.state,
        AgentState::Patrolling,
        "soldier should remain Patrolling when bandit outside engage range"
    );
}

// ── Fighting behavior ───────────────────────────────────────────────────────

#[test]
fn fighting_charges_toward_bandit_at_full_speed_and_deposits_danger() {
    let brain = soldier_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    merchant.state = AgentState::Fighting;

    let mut sensory = default_sensory_input();
    sensory.fatigue = 10.0;
    // Bandit at 25px away (within the 50px disengage threshold)
    sensory.nearest_bandit = Some((Vec2::new(1.0, 0.0), 25.0));

    let action = brain.decide(&sensory, &mut merchant);

    assert_eq!(
        action.speed_mult, 1.0,
        "fighting soldier should charge at full speed"
    );
    assert_eq!(
        action.deposit_signal,
        Some(ReputationChannel::Danger),
        "fighting soldier should deposit Danger signal"
    );
    assert!(
        action.signal_strength > 0.0,
        "danger signal strength should be positive"
    );
    assert_eq!(
        merchant.state,
        AgentState::Fighting,
        "should remain Fighting while bandit within 50px"
    );
}

// ── Combat disengage ────────────────────────────────────────────────────────

#[test]
fn combat_disengage_when_bandit_far_away() {
    let brain = soldier_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    merchant.state = AgentState::Fighting;

    let mut sensory = default_sensory_input();
    sensory.fatigue = 10.0;
    // Bandit at 55px away (> 50px disengage threshold)
    sensory.nearest_bandit = Some((Vec2::new(1.0, 0.0), 55.0));

    let _action = brain.decide(&sensory, &mut merchant);

    assert_eq!(
        merchant.state,
        AgentState::Patrolling,
        "soldier should disengage to Patrolling when bandit > 50px away"
    );
}

#[test]
fn combat_disengage_when_bandit_disappears() {
    let brain = soldier_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    merchant.state = AgentState::Fighting;

    let mut sensory = default_sensory_input();
    sensory.fatigue = 10.0;
    sensory.nearest_bandit = None; // No bandit visible

    let _action = brain.decide(&sensory, &mut merchant);

    assert_eq!(
        merchant.state,
        AgentState::Patrolling,
        "soldier should return to Patrolling when no bandit is visible"
    );
}

// ── Risk tolerance ──────────────────────────────────────────────────────────

#[test]
fn higher_risk_tolerance_gives_larger_engage_range() {
    let brain = soldier_brain();

    // Low risk tolerance: engage_range = 30 + 0.1 * 20 = 32
    let mut low_risk = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    low_risk.state = AgentState::Patrolling;
    low_risk.traits.risk_tolerance = 0.1;

    // High risk tolerance: engage_range = 30 + 0.9 * 20 = 48
    let mut high_risk = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    high_risk.state = AgentState::Patrolling;
    high_risk.traits.risk_tolerance = 0.9;

    // Place bandit at 40px — should be outside low-risk range but inside high-risk range
    let mut sensory = default_sensory_input();
    sensory.fatigue = 10.0;
    sensory.nearest_bandit = Some((Vec2::new(1.0, 0.0), 40.0));

    brain.decide(&sensory, &mut low_risk);
    assert_eq!(
        low_risk.state,
        AgentState::Patrolling,
        "low-risk soldier should NOT engage bandit at 40px (engage_range=32)"
    );

    brain.decide(&sensory, &mut high_risk);
    assert_eq!(
        high_risk.state,
        AgentState::Fighting,
        "high-risk soldier SHOULD engage bandit at 40px (engage_range=48)"
    );
}

// ── Rest when fatigued ──────────────────────────────────────────────────────

#[test]
fn rest_when_fatigue_above_70() {
    let brain = soldier_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    merchant.state = AgentState::Patrolling;

    let mut sensory = default_sensory_input();
    sensory.fatigue = 75.0;
    sensory.nearest_bandit = None;

    let _action = brain.decide(&sensory, &mut merchant);

    assert_eq!(
        merchant.state,
        AgentState::Resting,
        "soldier should rest when fatigue > 70"
    );
}

#[test]
fn no_rest_when_fatigue_below_70() {
    let brain = soldier_brain();
    let mut merchant = make_merchant_at(Vec2::new(30.0, 30.0), Profession::Soldier);
    merchant.state = AgentState::Patrolling;

    let mut sensory = default_sensory_input();
    sensory.fatigue = 50.0;
    sensory.nearest_bandit = None;

    let _action = brain.decide(&sensory, &mut merchant);

    assert_eq!(
        merchant.state,
        AgentState::Patrolling,
        "soldier should NOT rest when fatigue < 70"
    );
}
