use noise::{NoiseFn, Perlin, MultiFractal, Fbm};

use crate::config::WorldConfig;
use crate::types::{Season, TerrainType};

/// 2D heightmap-based terrain generated from Perlin noise.
pub struct Terrain {
    width: u32,
    height: u32,
    sea_level: f32,
    /// Row-major heightmap: index = y * width + x, values in [0, 1].
    heightmap: Vec<f32>,
    /// Cached terrain classification per cell.
    terrain_map: Vec<TerrainType>,
}

impl Terrain {
    /// Generate terrain from world configuration. Same seed always produces
    /// identical terrain.
    pub fn new(config: &WorldConfig) -> Self {
        let w = config.width as usize;
        let h = config.height as usize;
        let total = w * h;

        // Build 4-octave fBm Perlin noise from seed.
        let fbm: Fbm<Perlin> = Fbm::new(config.terrain_seed)
            .set_octaves(config.terrain_octaves as usize);

        // Sample noise and normalise to [0, 1].
        let scale = 0.005; // controls feature size
        let mut heightmap = Vec::with_capacity(total);
        let mut min_val = f64::MAX;
        let mut max_val = f64::MIN;

        let mut raw = Vec::with_capacity(total);
        for y in 0..h {
            for x in 0..w {
                let val = fbm.get([x as f64 * scale, y as f64 * scale]);
                raw.push(val);
                if val < min_val { min_val = val; }
                if val > max_val { max_val = val; }
            }
        }

        let range = if (max_val - min_val).abs() < f64::EPSILON {
            1.0
        } else {
            max_val - min_val
        };

        for &v in &raw {
            heightmap.push(((v - min_val) / range) as f32);
        }

        // First pass: classify without coast detection.
        let mut terrain_map: Vec<TerrainType> = heightmap
            .iter()
            .map(|&h| TerrainType::from_height(h, config.sea_level, false))
            .collect();

        // Second pass: identify coast cells (water adjacent to land).
        let width = config.width;
        let height = config.height;
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                if terrain_map[idx] != TerrainType::Water {
                    continue;
                }
                if Self::has_land_neighbour(&terrain_map, x, y, width, height) {
                    terrain_map[idx] = TerrainType::Coast;
                }
            }
        }

        Self {
            width: config.width,
            height: config.height,
            sea_level: config.sea_level,
            heightmap,
            terrain_map,
        }
    }

    /// Check whether any 4-connected neighbour is land (not Water/Coast).
    fn has_land_neighbour(
        map: &[TerrainType],
        x: usize,
        y: usize,
        w: u32,
        h: u32,
    ) -> bool {
        let w = w as usize;
        let h = h as usize;
        let neighbours: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        for (dx, dy) in neighbours {
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                let ni = ny as usize * w + nx as usize;
                let t = map[ni];
                if t != TerrainType::Water && t != TerrainType::Coast {
                    return true;
                }
            }
        }
        false
    }

    /// Terrain type at the given world coordinate.
    pub fn terrain_at(&self, x: u32, y: u32) -> TerrainType {
        self.terrain_map[self.idx(x, y)]
    }

    /// Raw height value in [0, 1] at the given world coordinate.
    pub fn height_at(&self, x: u32, y: u32) -> f32 {
        self.heightmap[self.idx(x, y)]
    }

    /// Base speed multiplier for a terrain type (no seasonal modifier).
    pub fn speed_multiplier(terrain: TerrainType) -> f32 {
        terrain.speed_multiplier()
    }

    /// Speed multiplier at a position, including seasonal modifier.
    /// Winter applies a ×0.7 global modifier.
    pub fn speed_at(&self, x: u32, y: u32, season: Season) -> f32 {
        let base = self.terrain_at(x, y).speed_multiplier();
        let seasonal = match season {
            Season::Winter => 0.7,
            _ => 1.0,
        };
        base * seasonal
    }

    /// Whether the cell at (x, y) can be traversed on foot.
    pub fn is_passable(&self, x: u32, y: u32) -> bool {
        self.terrain_at(x, y).is_passable()
    }

    /// Whether the cell at (x, y) is a Coast cell.
    pub fn is_coastal(&self, x: u32, y: u32) -> bool {
        self.terrain_at(x, y) == TerrainType::Coast
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn sea_level(&self) -> f32 {
        self.sea_level
    }

    /// Override the terrain type at a given cell (used by terrain painting).
    pub fn set_terrain_at(&mut self, x: u32, y: u32, terrain: TerrainType) {
        let i = self.idx(x, y);
        self.terrain_map[i] = terrain;
    }

    fn idx(&self, x: u32, y: u32) -> usize {
        debug_assert!(x < self.width && y < self.height, "terrain coord out of bounds");
        y as usize * self.width as usize + x as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> WorldConfig {
        WorldConfig {
            width: 64,
            height: 64,
            terrain_seed: 42,
            terrain_octaves: 4,
            sea_level: 0.25,
            num_cities: 0,
            num_resource_nodes: 0,
            season_length_ticks: 100,
        }
    }

    #[test]
    fn deterministic_generation() {
        let cfg = test_config();
        let t1 = Terrain::new(&cfg);
        let t2 = Terrain::new(&cfg);
        assert_eq!(t1.heightmap, t2.heightmap);
        for i in 0..t1.terrain_map.len() {
            assert_eq!(t1.terrain_map[i], t2.terrain_map[i]);
        }
    }

    #[test]
    fn height_range_normalised() {
        let t = Terrain::new(&test_config());
        let min = t.heightmap.iter().cloned().fold(f32::MAX, f32::min);
        let max = t.heightmap.iter().cloned().fold(f32::MIN, f32::max);
        assert!((min - 0.0).abs() < 1e-5);
        assert!((max - 1.0).abs() < 1e-5);
    }

    #[test]
    fn terrain_classification_covers_all() {
        let t = Terrain::new(&test_config());
        for &tt in &t.terrain_map {
            // Every cell must be one of the known variants.
            let _ = tt.speed_multiplier();
        }
    }

    #[test]
    fn impassable_terrain() {
        assert_eq!(Terrain::speed_multiplier(TerrainType::Mountains), 0.0);
        assert_eq!(Terrain::speed_multiplier(TerrainType::Water), 0.0);
        assert!(!TerrainType::Mountains.is_passable());
        assert!(!TerrainType::Water.is_passable());
    }

    #[test]
    fn winter_speed_modifier() {
        let t = Terrain::new(&test_config());
        // Find a passable cell.
        let (px, py) = (0..t.width())
            .flat_map(|x| (0..t.height()).map(move |y| (x, y)))
            .find(|&(x, y)| t.is_passable(x, y))
            .expect("at least one passable cell");

        let base = t.speed_at(px, py, Season::Summer);
        let winter = t.speed_at(px, py, Season::Winter);
        assert!((winter - base * 0.7).abs() < 1e-5);
    }

    #[test]
    fn coast_adjacent_to_land() {
        let t = Terrain::new(&test_config());
        let w = t.width() as usize;
        let h = t.height() as usize;
        for y in 0..h {
            for x in 0..w {
                if t.terrain_at(x as u32, y as u32) == TerrainType::Coast {
                    // Must have at least one land neighbour.
                    assert!(
                        Terrain::has_land_neighbour(
                            &t.terrain_map, x, y, t.width(), t.height()
                        ),
                        "Coast at ({x},{y}) has no land neighbour"
                    );
                }
            }
        }
    }
}
