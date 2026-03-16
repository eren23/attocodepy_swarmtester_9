mod common;

use swarm_economy::types::{Season, TerrainType};
use swarm_economy::world::terrain::Terrain;

use common::*;

// ── Perlin output in [0, 1] ────────────────────────────────────────────────

#[test]
fn perlin_heights_in_0_1_range() {
    let terrain = make_terrain();
    for y in 0..terrain.height() {
        for x in 0..terrain.width() {
            let h = terrain.height_at(x, y);
            assert!(
                (0.0..=1.0).contains(&h),
                "height_at({x}, {y}) = {h} is outside [0, 1]"
            );
        }
    }
}

#[test]
fn perlin_heights_span_full_range() {
    let terrain = make_terrain();
    let mut min_h = f32::MAX;
    let mut max_h = f32::MIN;
    for y in 0..terrain.height() {
        for x in 0..terrain.width() {
            let h = terrain.height_at(x, y);
            if h < min_h { min_h = h; }
            if h > max_h { max_h = h; }
        }
    }
    // After normalisation, min should be ~0 and max ~1.
    assert!(
        (min_h - 0.0).abs() < 1e-4,
        "minimum height should be ~0, got {min_h}"
    );
    assert!(
        (max_h - 1.0).abs() < 1e-4,
        "maximum height should be ~1, got {max_h}"
    );
}

// ── Deterministic seed ─────────────────────────────────────────────────────

#[test]
fn same_seed_produces_identical_terrain() {
    let config = mini_world_config();
    let t1 = Terrain::new(&config);
    let t2 = Terrain::new(&config);

    for y in 0..config.height {
        for x in 0..config.width {
            assert_eq!(
                t1.height_at(x, y),
                t2.height_at(x, y),
                "height mismatch at ({x}, {y})"
            );
            assert_eq!(
                t1.terrain_at(x, y),
                t2.terrain_at(x, y),
                "terrain type mismatch at ({x}, {y})"
            );
        }
    }
}

#[test]
fn different_seed_produces_different_terrain() {
    let config1 = mini_world_config(); // seed 42
    let mut config2 = mini_world_config();
    config2.terrain_seed = 99;

    let t1 = Terrain::new(&config1);
    let t2 = Terrain::new(&config2);

    // At least some cells should differ.
    let mut any_different = false;
    for y in 0..config1.height {
        for x in 0..config1.width {
            if (t1.height_at(x, y) - t2.height_at(x, y)).abs() > 1e-4 {
                any_different = true;
                break;
            }
        }
        if any_different { break; }
    }
    assert!(
        any_different,
        "different seeds should produce different terrain"
    );
}

// ── Type thresholds (from_height) ──────────────────────────────────────────

#[test]
fn from_height_water_below_sea_level() {
    let sea_level = 0.25;
    assert_eq!(
        TerrainType::from_height(0.0, sea_level, false),
        TerrainType::Water,
        "height 0.0 < sea_level should be Water"
    );
    assert_eq!(
        TerrainType::from_height(0.24, sea_level, false),
        TerrainType::Water,
        "height 0.24 < sea_level should be Water"
    );
}

#[test]
fn from_height_coast_below_sea_level_adjacent_to_land() {
    let sea_level = 0.25;
    assert_eq!(
        TerrainType::from_height(0.1, sea_level, true),
        TerrainType::Coast,
        "height below sea_level with land neighbour should be Coast"
    );
    assert_eq!(
        TerrainType::from_height(0.24, sea_level, true),
        TerrainType::Coast,
        "height 0.24 < sea_level adjacent to land should be Coast"
    );
}

#[test]
fn from_height_plains() {
    let sea_level = 0.25;
    assert_eq!(
        TerrainType::from_height(0.25, sea_level, false),
        TerrainType::Plains,
        "height at sea_level should be Plains"
    );
    assert_eq!(
        TerrainType::from_height(0.29, sea_level, false),
        TerrainType::Plains,
        "height 0.29 < 0.3 should be Plains"
    );
}

#[test]
fn from_height_forest() {
    let sea_level = 0.25;
    assert_eq!(
        TerrainType::from_height(0.3, sea_level, false),
        TerrainType::Forest,
        "height 0.3 should be Forest"
    );
    assert_eq!(
        TerrainType::from_height(0.49, sea_level, false),
        TerrainType::Forest,
        "height 0.49 < 0.5 should be Forest"
    );
}

#[test]
fn from_height_hills() {
    let sea_level = 0.25;
    assert_eq!(
        TerrainType::from_height(0.5, sea_level, false),
        TerrainType::Hills,
        "height 0.5 should be Hills"
    );
    assert_eq!(
        TerrainType::from_height(0.69, sea_level, false),
        TerrainType::Hills,
        "height 0.69 < 0.7 should be Hills"
    );
}

#[test]
fn from_height_mountains() {
    let sea_level = 0.25;
    assert_eq!(
        TerrainType::from_height(0.7, sea_level, false),
        TerrainType::Mountains,
        "height 0.7 should be Mountains"
    );
    assert_eq!(
        TerrainType::from_height(1.0, sea_level, false),
        TerrainType::Mountains,
        "height 1.0 should be Mountains"
    );
}

#[test]
fn from_height_boundary_at_sea_level() {
    // Exactly at sea_level boundary: height >= sea_level goes to Plains (if < 0.3).
    let sea_level = 0.25;
    assert_eq!(
        TerrainType::from_height(sea_level, sea_level, false),
        TerrainType::Plains,
        "height exactly at sea_level should be Plains, not Water"
    );
    assert_eq!(
        TerrainType::from_height(sea_level - 0.001, sea_level, false),
        TerrainType::Water,
        "height just below sea_level should be Water"
    );
}

// ── Impassability ──────────────────────────────────────────────────────────

#[test]
fn mountains_are_impassable() {
    assert!(!TerrainType::Mountains.is_passable());
    assert!(
        (TerrainType::Mountains.speed_multiplier() - 0.0).abs() < 1e-6,
        "Mountains speed_multiplier should be 0.0"
    );
}

#[test]
fn water_is_impassable() {
    assert!(!TerrainType::Water.is_passable());
    assert!(
        (TerrainType::Water.speed_multiplier() - 0.0).abs() < 1e-6,
        "Water speed_multiplier should be 0.0"
    );
}

#[test]
fn passable_terrain_types_are_passable() {
    for tt in &[
        TerrainType::Plains,
        TerrainType::Forest,
        TerrainType::Hills,
        TerrainType::Coast,
    ] {
        assert!(
            tt.is_passable(),
            "{tt:?} should be passable"
        );
        assert!(
            tt.speed_multiplier() > 0.0,
            "{tt:?} speed_multiplier should be > 0"
        );
    }
}

#[test]
fn terrain_is_passable_matches_type() {
    let terrain = make_terrain();
    for y in 0..terrain.height() {
        for x in 0..terrain.width() {
            let tt = terrain.terrain_at(x, y);
            assert_eq!(
                terrain.is_passable(x, y),
                tt.is_passable(),
                "is_passable mismatch at ({x}, {y}) for {tt:?}"
            );
        }
    }
}

// ── Speed multipliers ──────────────────────────────────────────────────────

#[test]
fn plains_speed_multiplier() {
    assert!(
        (TerrainType::Plains.speed_multiplier() - 1.0).abs() < 1e-6,
        "Plains speed should be 1.0"
    );
}

#[test]
fn forest_speed_multiplier() {
    assert!(
        (TerrainType::Forest.speed_multiplier() - 0.6).abs() < 1e-6,
        "Forest speed should be 0.6"
    );
}

#[test]
fn hills_speed_multiplier() {
    assert!(
        (TerrainType::Hills.speed_multiplier() - 0.4).abs() < 1e-6,
        "Hills speed should be 0.4"
    );
}

#[test]
fn coast_speed_multiplier() {
    assert!(
        (TerrainType::Coast.speed_multiplier() - 0.8).abs() < 1e-6,
        "Coast speed should be 0.8"
    );
}

#[test]
fn mountains_speed_multiplier() {
    assert!(
        (TerrainType::Mountains.speed_multiplier() - 0.0).abs() < 1e-6,
        "Mountains speed should be 0.0"
    );
}

#[test]
fn water_speed_multiplier() {
    assert!(
        (TerrainType::Water.speed_multiplier() - 0.0).abs() < 1e-6,
        "Water speed should be 0.0"
    );
}

// ── Coast identification ───────────────────────────────────────────────────

#[test]
fn coast_cell_is_water_adjacent_to_land() {
    let terrain = make_terrain();
    let w = terrain.width();
    let h = terrain.height();

    let mut found_coast = false;
    for y in 0..h {
        for x in 0..w {
            if terrain.terrain_at(x, y) == TerrainType::Coast {
                found_coast = true;
                // Coast must have at least one passable (land) neighbour.
                let neighbors: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
                let has_land = neighbors.iter().any(|&(dx, dy)| {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && ny >= 0 && (nx as u32) < w && (ny as u32) < h {
                        let t = terrain.terrain_at(nx as u32, ny as u32);
                        t != TerrainType::Water && t != TerrainType::Coast
                    } else {
                        false
                    }
                });
                assert!(
                    has_land,
                    "Coast at ({x}, {y}) must have at least one land neighbour"
                );
            }
        }
    }
    assert!(found_coast, "terrain with sea_level=0.25 should have some coast cells");
}

#[test]
fn is_coastal_returns_true_for_coast() {
    let terrain = make_terrain();
    for y in 0..terrain.height() {
        for x in 0..terrain.width() {
            let expected = terrain.terrain_at(x, y) == TerrainType::Coast;
            assert_eq!(
                terrain.is_coastal(x, y),
                expected,
                "is_coastal mismatch at ({x}, {y})"
            );
        }
    }
}

#[test]
fn no_coast_when_sea_level_zero() {
    let terrain = make_all_land_terrain();
    let mut found_coast = false;
    let mut found_water = false;
    for y in 0..terrain.height() {
        for x in 0..terrain.width() {
            if terrain.terrain_at(x, y) == TerrainType::Coast {
                found_coast = true;
            }
            if terrain.terrain_at(x, y) == TerrainType::Water {
                found_water = true;
            }
        }
    }
    assert!(
        !found_water,
        "sea_level=0 should produce no Water cells"
    );
    assert!(
        !found_coast,
        "no water means no coast cells either"
    );
}

#[test]
fn deep_water_cells_are_not_coast() {
    // A Water cell that is NOT adjacent to land should remain Water (not Coast).
    let terrain = make_terrain();
    let w = terrain.width();
    let h = terrain.height();

    for y in 0..h {
        for x in 0..w {
            if terrain.terrain_at(x, y) == TerrainType::Water {
                // Verify no land neighbour.
                let neighbors: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
                let has_land = neighbors.iter().any(|&(dx, dy)| {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && ny >= 0 && (nx as u32) < w && (ny as u32) < h {
                        let t = terrain.terrain_at(nx as u32, ny as u32);
                        t != TerrainType::Water && t != TerrainType::Coast
                    } else {
                        false
                    }
                });
                assert!(
                    !has_land,
                    "Water cell at ({x}, {y}) should NOT have a land neighbour — it should be Coast instead"
                );
            }
        }
    }
}

// ── Seasonal modifiers ─────────────────────────────────────────────────────

#[test]
fn winter_speed_is_base_times_0_7() {
    let terrain = make_terrain();
    // Find a passable cell.
    let (px, py) = find_passable_cell(&terrain);

    let base = terrain.speed_at(px, py, Season::Summer);
    let winter = terrain.speed_at(px, py, Season::Winter);

    assert!(
        base > 0.0,
        "passable cell should have positive speed"
    );
    assert!(
        (winter - base * 0.7).abs() < 1e-5,
        "winter speed should be base * 0.7: expected {}, got {winter}",
        base * 0.7
    );
}

#[test]
fn spring_summer_autumn_have_no_modifier() {
    let terrain = make_terrain();
    let (px, py) = find_passable_cell(&terrain);

    let spring = terrain.speed_at(px, py, Season::Spring);
    let summer = terrain.speed_at(px, py, Season::Summer);
    let autumn = terrain.speed_at(px, py, Season::Autumn);

    let base = terrain.terrain_at(px, py).speed_multiplier();

    assert!(
        (spring - base).abs() < 1e-6,
        "spring speed should equal base"
    );
    assert!(
        (summer - base).abs() < 1e-6,
        "summer speed should equal base"
    );
    assert!(
        (autumn - base).abs() < 1e-6,
        "autumn speed should equal base"
    );
}

#[test]
fn winter_modifier_applies_to_all_passable_types() {
    let terrain = make_terrain();
    let w = terrain.width();
    let h = terrain.height();

    let mut checked = std::collections::HashSet::new();

    for y in 0..h {
        for x in 0..w {
            let tt = terrain.terrain_at(x, y);
            if tt.is_passable() && !checked.contains(&tt) {
                checked.insert(tt);
                let base = tt.speed_multiplier();
                let winter_speed = terrain.speed_at(x, y, Season::Winter);
                assert!(
                    (winter_speed - base * 0.7).abs() < 1e-5,
                    "winter speed for {tt:?} should be {:.3} * 0.7 = {:.3}, got {winter_speed:.3}",
                    base,
                    base * 0.7
                );
            }
        }
    }
}

#[test]
fn impassable_terrain_speed_zero_in_all_seasons() {
    let seasons = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];
    let terrain = make_terrain();
    let w = terrain.width();
    let h = terrain.height();

    for y in 0..h {
        for x in 0..w {
            if !terrain.is_passable(x, y) {
                for &season in &seasons {
                    let s = terrain.speed_at(x, y, season);
                    assert!(
                        s.abs() < 1e-6,
                        "impassable cell ({x}, {y}) should have speed 0 in {season:?}, got {s}"
                    );
                }
            }
        }
    }
}

// ── Terrain dimensions ─────────────────────────────────────────────────────

#[test]
fn terrain_dimensions_match_config() {
    let config = mini_world_config();
    let terrain = Terrain::new(&config);
    assert_eq!(terrain.width(), config.width);
    assert_eq!(terrain.height(), config.height);
}

#[test]
fn sea_level_accessor() {
    let config = mini_world_config();
    let terrain = Terrain::new(&config);
    assert!(
        (terrain.sea_level() - config.sea_level).abs() < 1e-6,
        "sea_level should match config"
    );
}

// ── set_terrain_at override ────────────────────────────────────────────────

#[test]
fn set_terrain_at_overrides_type() {
    let mut terrain = make_all_land_terrain();
    let (px, py) = find_passable_cell(&terrain);

    // Should start passable.
    assert!(terrain.is_passable(px, py));

    // Paint as Mountains.
    terrain.set_terrain_at(px, py, TerrainType::Mountains);
    assert_eq!(terrain.terrain_at(px, py), TerrainType::Mountains);
    assert!(!terrain.is_passable(px, py));

    // Paint back as Plains.
    terrain.set_terrain_at(px, py, TerrainType::Plains);
    assert!(terrain.is_passable(px, py));
}

// ── Helper ─────────────────────────────────────────────────────────────────

/// Find the first passable cell in the terrain (as grid coords).
fn find_passable_cell(terrain: &Terrain) -> (u32, u32) {
    for y in 0..terrain.height() {
        for x in 0..terrain.width() {
            if terrain.is_passable(x, y) {
                return (x, y);
            }
        }
    }
    panic!("no passable cell found");
}
