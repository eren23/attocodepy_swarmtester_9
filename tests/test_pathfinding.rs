mod common;

use swarm_economy::agents::actions::MerchantAction;
use swarm_economy::types::*;
use swarm_economy::world::terrain::Terrain;

use common::*;

/// Create a fully-flat Plains terrain with no mountains or water.
/// `make_all_land_terrain()` still has mountains from heightmap noise,
/// so we manually flatten every cell.
fn make_flat_terrain() -> Terrain {
    let mut terrain = make_all_land_terrain();
    for y in 0..terrain.height() {
        for x in 0..terrain.width() {
            terrain.set_terrain_at(x, y, TerrainType::Plains);
        }
    }
    terrain.rebuild_components();
    terrain
}

// ── Pathfinding around obstacles ──────────────────────────────────────────

#[test]
fn test_pathfinding_around_mountain_range() {
    let mut terrain = make_flat_terrain();

    // Vertical mountain wall at x=32, y=5..55
    for y in 5..55 {
        terrain.set_terrain_at(32, y, TerrainType::Mountains);
    }
    terrain.rebuild_components();

    let start = (10, 30); // left side
    let goal = (50, 30);  // right side

    let path = terrain
        .find_path(start, goal)
        .expect("path should exist around mountain range");

    // Path must not contain any impassable cell.
    for &(px, py) in &path {
        assert!(
            terrain.is_passable(px, py),
            "path contains impassable cell ({px}, {py}), terrain={:?}",
            terrain.terrain_at(px, py),
        );
    }

    // Path should start at `start` and end at `goal`.
    assert_eq!(
        *path.first().unwrap(),
        start,
        "path should start at the start position"
    );
    assert_eq!(
        *path.last().unwrap(),
        goal,
        "path should end at the goal position"
    );

    // Path must be longer than the straight-line manhattan distance since it
    // needs to go around the wall.
    let direct_dist = ((goal.0 as i32 - start.0 as i32).abs()
        + (goal.1 as i32 - start.1 as i32).abs()) as usize;
    assert!(
        path.len() > direct_dist,
        "path length {} should exceed direct manhattan distance {direct_dist}",
        path.len(),
    );
}

// ── Unreachable destination ──────────────────────────────────────────────

#[test]
fn test_unreachable_destination_returns_none() {
    let mut terrain = make_flat_terrain();

    // Surround cell (32, 32) with mountains on all 8 neighbors, making it an
    // isolated island that nothing outside can reach.
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            terrain.set_terrain_at(
                (32 + dx) as u32,
                (32 + dy) as u32,
                TerrainType::Mountains,
            );
        }
    }
    terrain.rebuild_components();

    let start = (10, 10); // outside the ring — passable
    let goal = (32, 32);  // inside the ring — passable but isolated

    assert!(
        terrain.is_passable(start.0, start.1),
        "start should be passable"
    );
    assert!(
        terrain.is_passable(goal.0, goal.1),
        "goal should be passable (but isolated)"
    );

    // Reachability check
    assert!(
        !terrain.is_reachable(start, goal),
        "isolated cell should not be reachable"
    );

    // find_path should return None
    assert!(
        terrain.find_path(start, goal).is_none(),
        "find_path to isolated cell should return None"
    );
}

// ── Merchant does not stick to gray tiles ─────────────────────────────────

#[test]
fn test_merchant_does_not_stick_to_gray_tiles() {
    let mut terrain = make_flat_terrain();
    let roads = make_road_grid();

    // Mountain wall at x=32, y=10..54 — leaves passages at top and bottom.
    for y in 10..54 {
        terrain.set_terrain_at(32, y, TerrainType::Mountains);
    }
    terrain.rebuild_components();

    // Place two cities on either side of the wall.
    let city_a_pos = Vec2::new(16.0, 32.0);
    let city_b_pos = Vec2::new(48.0, 32.0);
    let city_a = make_city(0, city_a_pos);
    let city_b = make_city(1, city_b_pos);

    // Spawn merchant at city A heading toward city B.
    let mut merchant = make_merchant_at(city_a_pos, Profession::Trader);
    merchant.fatigue = 0.0;
    merchant.home_city = city_a.id;

    // Compute a path from A to B.
    let start_grid = (city_a_pos.x as u32, city_a_pos.y as u32);
    let goal_grid = (city_b_pos.x as u32, city_b_pos.y as u32);
    let path = terrain
        .find_path(start_grid, goal_grid)
        .expect("path should exist between city A and city B");
    merchant.set_waypoints(city_b.id, path);

    let world_w = terrain.width() as f32;
    let world_h = terrain.height() as f32;

    let total_ticks = 1500;
    let window = 50;
    let mut positions: Vec<Vec2> = Vec::with_capacity(total_ticks);
    let mut reached = false;

    for tick in 0..total_ticks {
        // Follow waypoints: compute direction toward the next waypoint and
        // create a movement action.
        let action = if let Some(dir) = merchant.advance_waypoints() {
            let target_heading = dir.y.atan2(dir.x).rem_euclid(std::f32::consts::TAU);
            let mut turn = target_heading - merchant.heading;
            // Normalize turn to [-PI, PI]
            if turn > std::f32::consts::PI {
                turn -= std::f32::consts::TAU;
            }
            if turn < -std::f32::consts::PI {
                turn += std::f32::consts::TAU;
            }
            // Clamp to max turn rate (pi/6)
            turn = turn.clamp(-std::f32::consts::FRAC_PI_6, std::f32::consts::FRAC_PI_6);
            MerchantAction::movement(turn, 1.0)
        } else {
            MerchantAction::movement(0.0, 0.0) // waypoints exhausted
        };

        merchant.apply_action(&action, &terrain, &roads, Season::Spring, world_w, world_h);

        // (a) Merchant must never stand on an impassable tile.
        let mx = (merchant.pos.x as u32).min(terrain.width() - 1);
        let my = (merchant.pos.y as u32).min(terrain.height() - 1);
        assert!(
            terrain.is_passable(mx, my),
            "tick {tick}: merchant is on impassable tile ({mx}, {my}), terrain={:?}",
            terrain.terrain_at(mx, my),
        );

        positions.push(merchant.pos);

        // Check if merchant reached city B (within city radius).
        if merchant.pos.distance(city_b_pos) < 15.0 {
            reached = true;
            break;
        }

        // Recompute path if merchant is stuck.
        merchant.update_stuck_detection();
        if merchant.is_stuck() {
            let cur = (merchant.pos.x as u32, merchant.pos.y as u32);
            if let Some(new_path) = terrain.find_path(cur, goal_grid) {
                merchant.set_waypoints(city_b.id, new_path);
            }
        }

        // Recover a bit of fatigue periodically so merchant doesn't collapse.
        if tick % 100 == 0 {
            merchant.recover_fatigue_at_city();
        }
    }

    // (b) Merchant should be making progress: position must change over any
    // 50-tick window. We check after we have enough data.
    if !reached {
        for i in window..positions.len() {
            let old = positions[i - window];
            let cur = positions[i];
            let dist = old.distance(cur);
            assert!(
                dist > 0.5,
                "merchant made no progress in {window}-tick window ending at tick {i}: \
                 distance moved = {dist:.3}"
            );
        }
    }

    // The merchant should have either reached city B or made substantial
    // progress toward it.
    let final_dist = merchant.pos.distance(city_b_pos);
    let initial_dist = city_a_pos.distance(city_b_pos);
    assert!(
        reached || final_dist < initial_dist * 0.5,
        "merchant should have reached city B or covered >50% of the distance: \
         initial_dist={initial_dist:.1}, final_dist={final_dist:.1}"
    );
}

// ── Path prefers faster terrain ──────────────────────────────────────────

#[test]
fn test_path_prefers_faster_terrain() {
    let mut terrain = make_flat_terrain();

    // Fill a band of hills along y=10 from x=6..14 (the direct corridor).
    for x in 6..15 {
        terrain.set_terrain_at(x, 10, TerrainType::Hills);
    }
    // Also block the immediate neighbors of the corridor to force the
    // pathfinder to actually detour rather than slip around by one cell.
    for x in 6..15 {
        terrain.set_terrain_at(x, 9, TerrainType::Hills);
        terrain.set_terrain_at(x, 11, TerrainType::Hills);
    }
    terrain.rebuild_components();

    let start = (5, 10);
    let goal = (15, 10);

    let path = terrain
        .find_path(start, goal)
        .expect("path should exist");

    // Count how many cells in the path are hills vs plains.
    let hills_count = path
        .iter()
        .filter(|&&(x, y)| terrain.terrain_at(x, y) == TerrainType::Hills)
        .count();
    let plains_count = path
        .iter()
        .filter(|&&(x, y)| terrain.terrain_at(x, y) == TerrainType::Plains)
        .count();

    // The optimal path should go around the hill band via plains. The
    // majority of path cells should be plains, not hills.
    assert!(
        plains_count > hills_count,
        "path should prefer plains over hills: plains={plains_count}, hills={hills_count}"
    );

    // Compute path cost. The optimal path through plains (detour) should
    // have a lower total cost than the direct hills corridor.
    let path_cost: f32 = path
        .windows(2)
        .map(|w| {
            let (x1, y1) = w[0];
            let (x2, y2) = w[1];
            let dx = (x2 as f32 - x1 as f32).abs();
            let dy = (y2 as f32 - y1 as f32).abs();
            let dist = if dx > 0.0 && dy > 0.0 {
                std::f32::consts::SQRT_2
            } else {
                1.0
            };
            let speed = Terrain::speed_multiplier(terrain.terrain_at(x2, y2));
            if speed > 0.0 {
                dist / speed
            } else {
                f32::MAX
            }
        })
        .sum();

    // Compare against the cost of the direct hills route (9 cells at cost 1/0.4 each).
    let direct_hills_cost: f32 = 9.0 / 0.4; // = 22.5
    assert!(
        path_cost < direct_hills_cost + 1.0,
        "A* path cost ({path_cost:.2}) should be competitive with or better than \
         direct hills cost ({direct_hills_cost:.2})"
    );
}
