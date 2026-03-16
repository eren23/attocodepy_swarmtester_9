use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

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
    /// Connected-component ID per cell (0 = impassable). Passable cells in
    /// the same component can reach each other; different IDs cannot.
    components: Vec<u32>,
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

        // Third pass: flood-fill connected components for O(1) reachability.
        let components = Self::compute_components(&terrain_map, width, height);

        Self {
            width: config.width,
            height: config.height,
            sea_level: config.sea_level,
            heightmap,
            terrain_map,
            components,
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

    /// Flood-fill 4-connected components over passable cells.
    fn compute_components(map: &[TerrainType], w: u32, h: u32) -> Vec<u32> {
        let total = (w as usize) * (h as usize);
        let mut comp = vec![0u32; total];
        let mut next_id = 1u32;
        let mut queue = VecDeque::new();

        for start in 0..total {
            if comp[start] != 0 || !map[start].is_passable() {
                continue;
            }
            comp[start] = next_id;
            queue.push_back(start);
            while let Some(ci) = queue.pop_front() {
                let cx = (ci % w as usize) as i32;
                let cy = (ci / w as usize) as i32;
                for &(dx, dy) in &[(-1i32, 0), (1, 0), (0, -1i32), (0, 1)] {
                    let nx = cx + dx;
                    let ny = cy + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let ni = ny as usize * w as usize + nx as usize;
                    if comp[ni] == 0 && map[ni].is_passable() {
                        comp[ni] = next_id;
                        queue.push_back(ni);
                    }
                }
            }
            next_id += 1;
        }
        comp
    }

    /// O(1) reachability test: two passable cells are reachable iff they
    /// belong to the same connected component.
    pub fn same_component(&self, a: (u32, u32), b: (u32, u32)) -> bool {
        if !self.in_bounds(a.0, a.1) || !self.in_bounds(b.0, b.1) {
            return false;
        }
        let ca = self.components[self.idx(a.0, a.1)];
        let cb = self.components[self.idx(b.0, b.1)];
        ca != 0 && ca == cb
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
    /// Call `rebuild_components()` after finishing a batch of changes.
    pub fn set_terrain_at(&mut self, x: u32, y: u32, terrain: TerrainType) {
        let i = self.idx(x, y);
        self.terrain_map[i] = terrain;
    }

    /// Recompute connected components after terrain changes.
    pub fn rebuild_components(&mut self) {
        self.components = Self::compute_components(&self.terrain_map, self.width, self.height);
    }

    /// Compute the shortest walkable path from `start` to `goal` using A*
    /// with 8-directional movement. The cost of traversing a cell is
    /// `distance / speed_multiplier`, so faster terrain is preferred.
    /// Returns `None` if the goal is unreachable.
    pub fn find_path(&self, start: (u32, u32), goal: (u32, u32)) -> Option<Vec<(u32, u32)>> {
        if !self.in_bounds(start.0, start.1) || !self.in_bounds(goal.0, goal.1) {
            return None;
        }
        if !self.is_passable(start.0, start.1) || !self.is_passable(goal.0, goal.1) {
            return None;
        }
        if start == goal {
            return Some(vec![start]);
        }

        let total = (self.width as usize) * (self.height as usize);
        let mut g_score = vec![f32::INFINITY; total];
        let mut came_from: Vec<usize> = (0..total).collect();
        let mut closed = vec![false; total];

        let start_idx = self.idx(start.0, start.1);
        let goal_idx = self.idx(goal.0, goal.1);
        g_score[start_idx] = 0.0;

        let mut open = BinaryHeap::new();
        open.push(AStarNode {
            f_score: Self::heuristic(start, goal),
            g_score: 0.0,
            index: start_idx,
        });

        static DIRS: [(i32, i32); 8] = [
            (-1, -1), (0, -1), (1, -1),
            (-1,  0),          (1,  0),
            (-1,  1), (0,  1), (1,  1),
        ];

        while let Some(current) = open.pop() {
            if current.index == goal_idx {
                // Reconstruct path.
                let mut path = Vec::new();
                let mut ci = goal_idx;
                loop {
                    let cx = (ci % self.width as usize) as u32;
                    let cy = (ci / self.width as usize) as u32;
                    path.push((cx, cy));
                    if ci == start_idx {
                        break;
                    }
                    ci = came_from[ci];
                }
                path.reverse();
                return Some(path);
            }

            if closed[current.index] {
                continue;
            }
            closed[current.index] = true;

            let cx = (current.index % self.width as usize) as i32;
            let cy = (current.index / self.width as usize) as i32;

            for &(dx, dy) in &DIRS {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= self.width as i32 || ny >= self.height as i32 {
                    continue;
                }
                let nu = nx as u32;
                let nv = ny as u32;
                if !self.is_passable(nu, nv) {
                    continue;
                }

                let ni = self.idx(nu, nv);
                if closed[ni] {
                    continue;
                }

                let speed = self.terrain_at(nu, nv).speed_multiplier();
                // Distance: 1.0 for cardinal, sqrt(2) for diagonal.
                let dist = if dx == 0 || dy == 0 { 1.0_f32 } else { std::f32::consts::SQRT_2 };
                let move_cost = dist / speed;
                let tentative_g = current.g_score + move_cost;

                if tentative_g < g_score[ni] {
                    g_score[ni] = tentative_g;
                    came_from[ni] = current.index;
                    open.push(AStarNode {
                        f_score: tentative_g + Self::heuristic((nu, nv), goal),
                        g_score: tentative_g,
                        index: ni,
                    });
                }
            }
        }

        None
    }

    /// Returns `true` if a walkable path exists between `start` and `goal`.
    /// Uses precomputed connected components for O(1) lookup.
    pub fn is_reachable(&self, start: (u32, u32), goal: (u32, u32)) -> bool {
        self.same_component(start, goal)
    }

    /// Euclidean distance heuristic (admissible for cost = distance / speed
    /// since max speed is 1.0).
    fn heuristic(a: (u32, u32), b: (u32, u32)) -> f32 {
        let dx = a.0 as f32 - b.0 as f32;
        let dy = a.1 as f32 - b.1 as f32;
        (dx * dx + dy * dy).sqrt()
    }

    fn in_bounds(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.height
    }

    fn idx(&self, x: u32, y: u32) -> usize {
        debug_assert!(x < self.width && y < self.height, "terrain coord out of bounds");
        y as usize * self.width as usize + x as usize
    }
}

/// Min-heap node for A* open set (BinaryHeap is a max-heap, so we reverse ordering).
#[derive(PartialEq)]
struct AStarNode {
    f_score: f32,
    g_score: f32,
    index: usize,
}

impl Eq for AStarNode {}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse so BinaryHeap acts as a min-heap on f_score.
        other.f_score.partial_cmp(&self.f_score).unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
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

    #[test]
    fn path_to_self() {
        let t = Terrain::new(&test_config());
        let (px, py) = find_passable(&t);
        let path = t.find_path((px, py), (px, py)).unwrap();
        assert_eq!(path, vec![(px, py)]);
    }

    #[test]
    fn path_between_passable_neighbours() {
        let t = Terrain::new(&test_config());
        // Find two adjacent passable cells.
        let (ax, ay, bx, by) = find_two_adjacent_passable(&t);
        let path = t.find_path((ax, ay), (bx, by)).unwrap();
        assert_eq!(*path.first().unwrap(), (ax, ay));
        assert_eq!(*path.last().unwrap(), (bx, by));
        // All cells in the path must be passable.
        for &(x, y) in &path {
            assert!(t.is_passable(x, y), "path goes through impassable ({x},{y})");
        }
    }

    #[test]
    fn path_all_cells_passable() {
        let t = Terrain::new(&test_config());
        let (ax, ay, bx, by) = find_two_distant_passable(&t);
        if let Some(path) = t.find_path((ax, ay), (bx, by)) {
            for &(x, y) in &path {
                assert!(t.is_passable(x, y));
            }
            // Consecutive steps must be neighbours (8-connected).
            for w in path.windows(2) {
                let dx = (w[0].0 as i32 - w[1].0 as i32).abs();
                let dy = (w[0].1 as i32 - w[1].1 as i32).abs();
                assert!(dx <= 1 && dy <= 1 && (dx + dy) > 0,
                    "non-adjacent step {:?} -> {:?}", w[0], w[1]);
            }
        }
    }

    #[test]
    fn unreachable_returns_none() {
        // Build a tiny terrain where goal is surrounded by mountains.
        let mut t = Terrain::new(&WorldConfig {
            width: 5,
            height: 5,
            terrain_seed: 0,
            terrain_octaves: 1,
            sea_level: 0.0,
            num_cities: 0,
            num_resource_nodes: 0,
            season_length_ticks: 100,
        });
        // Make everything plains, then wall off (2,2) with mountains.
        for y in 0..5u32 {
            for x in 0..5u32 {
                t.set_terrain_at(x, y, TerrainType::Plains);
            }
        }
        for &(x, y) in &[(1,1),(2,1),(3,1),(1,2),(3,2),(1,3),(2,3),(3,3)] {
            t.set_terrain_at(x, y, TerrainType::Mountains);
        }
        t.rebuild_components();
        assert!(t.find_path((0, 0), (2, 2)).is_none());
        assert!(!t.is_reachable((0, 0), (2, 2)));
    }

    #[test]
    fn impassable_start_or_goal_returns_none() {
        let t = Terrain::new(&test_config());
        // Find a mountain cell.
        if let Some((mx, my)) = (0..t.width())
            .flat_map(|x| (0..t.height()).map(move |y| (x, y)))
            .find(|&(x, y)| t.terrain_at(x, y) == TerrainType::Mountains)
        {
            let (px, py) = find_passable(&t);
            assert!(t.find_path((mx, my), (px, py)).is_none());
            assert!(t.find_path((px, py), (mx, my)).is_none());
        }
    }

    #[test]
    fn out_of_bounds_returns_none() {
        let t = Terrain::new(&test_config());
        assert!(t.find_path((999, 999), (0, 0)).is_none());
    }

    #[test]
    fn prefers_faster_terrain() {
        // On a small grid: two routes, one through plains (speed 1.0), one through hills (0.4).
        // A* should pick the plains route.
        let mut t = Terrain::new(&WorldConfig {
            width: 5,
            height: 3,
            terrain_seed: 0,
            terrain_octaves: 1,
            sea_level: 0.0,
            num_cities: 0,
            num_resource_nodes: 0,
            season_length_ticks: 100,
        });
        // Row 0: Plains all across.
        // Row 1: Mountains in the middle to force detour.
        // Row 2: Hills all across.
        for x in 0..5u32 {
            t.set_terrain_at(x, 0, TerrainType::Plains);
            t.set_terrain_at(x, 1, TerrainType::Mountains);
            t.set_terrain_at(x, 2, TerrainType::Hills);
        }
        // Open start and end columns in row 1 so both routes connect.
        t.set_terrain_at(0, 1, TerrainType::Plains);
        t.set_terrain_at(4, 1, TerrainType::Plains);
        t.rebuild_components();

        let path = t.find_path((0, 1), (4, 1)).unwrap();
        // The path should go through row 0 (plains), not row 2 (hills).
        let goes_through_row0 = path.iter().any(|&(_, y)| y == 0);
        let goes_through_row2 = path.iter().any(|&(_, y)| y == 2);
        assert!(goes_through_row0, "A* should prefer faster plains route");
        assert!(!goes_through_row2, "A* should avoid slower hills route");
    }

    // --- test helpers ---

    fn find_passable(t: &Terrain) -> (u32, u32) {
        (0..t.width())
            .flat_map(|x| (0..t.height()).map(move |y| (x, y)))
            .find(|&(x, y)| t.is_passable(x, y))
            .expect("at least one passable cell")
    }

    fn find_two_adjacent_passable(t: &Terrain) -> (u32, u32, u32, u32) {
        for x in 0..t.width().saturating_sub(1) {
            for y in 0..t.height() {
                if t.is_passable(x, y) && t.is_passable(x + 1, y) {
                    return (x, y, x + 1, y);
                }
            }
        }
        panic!("no two adjacent passable cells found");
    }

    fn find_two_distant_passable(t: &Terrain) -> (u32, u32, u32, u32) {
        let a = find_passable(t);
        let b = (0..t.width())
            .rev()
            .flat_map(|x| (0..t.height()).rev().map(move |y| (x, y)))
            .find(|&(x, y)| t.is_passable(x, y) && (x != a.0 || y != a.1))
            .expect("at least two passable cells");
        (a.0, a.1, b.0, b.1)
    }
}
