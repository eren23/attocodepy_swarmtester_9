use crate::config::RoadConfig;
use crate::types::Vec2;

// ── RoadGrid ──────────────────────────────────────────────────────────────

pub struct RoadGrid {
    cols: usize,
    rows: usize,
    cell_size: f32,
    cells: Vec<f32>,
    increment: f32,
    decay: f32,
    max_speed_bonus: f32,
}

impl RoadGrid {
    pub fn new(config: &RoadConfig, world_width: u32, world_height: u32) -> Self {
        let cs = config.cell_size as usize;
        let cols = (world_width as usize + cs - 1) / cs;
        let rows = (world_height as usize + cs - 1) / cs;

        Self {
            cols,
            rows,
            cell_size: config.cell_size as f32,
            cells: vec![0.0; cols * rows],
            increment: config.increment,
            decay: config.decay,
            max_speed_bonus: config.max_speed_bonus,
        }
    }

    // ── Grid helpers ──────────────────────────────────────────────────────

    #[inline]
    fn world_to_cell(&self, pos: Vec2) -> Option<usize> {
        if pos.x < 0.0 || pos.y < 0.0 {
            return None;
        }
        let col = (pos.x / self.cell_size) as usize;
        let row = (pos.y / self.cell_size) as usize;
        if col < self.cols && row < self.rows {
            Some(row * self.cols + col)
        } else {
            None
        }
    }

    // ── Traversal ─────────────────────────────────────────────────────────

    /// Increment the road value when a merchant crosses the cell at `pos`.
    pub fn traverse(&mut self, pos: Vec2) {
        if let Some(i) = self.world_to_cell(pos) {
            self.cells[i] = (self.cells[i] + self.increment).min(1.0);
        }
    }

    // ── Tick: decay ───────────────────────────────────────────────────────

    /// Multiply all cells by the decay factor.
    pub fn tick(&mut self) {
        let d = self.decay;
        for v in &mut self.cells {
            *v *= d;
        }
    }

    // ── Queries ───────────────────────────────────────────────────────────

    /// Road value at world position (nearest cell). Returns 0.0 for out-of-bounds.
    pub fn road_value(&self, pos: Vec2) -> f32 {
        match self.world_to_cell(pos) {
            Some(i) => self.cells[i],
            None => 0.0,
        }
    }

    /// Speed multiplier at world position: `1.0 + max_speed_bonus * road_value`.
    pub fn speed_multiplier(&self, pos: Vec2) -> f32 {
        1.0 + self.max_speed_bonus * self.road_value(pos)
    }

    // ── Accessors ─────────────────────────────────────────────────────────

    pub fn raw_cells(&self) -> &[f32] {
        &self.cells
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RoadConfig;

    fn test_config() -> RoadConfig {
        RoadConfig {
            cell_size: 8,
            increment: 0.002,
            decay: 0.9998,
            max_speed_bonus: 0.6,
        }
    }

    #[test]
    fn grid_dimensions() {
        let grid = RoadGrid::new(&test_config(), 1600, 1000);
        assert_eq!(grid.cols(), 200);
        assert_eq!(grid.rows(), 125);
    }

    #[test]
    fn traverse_increments() {
        let mut grid = RoadGrid::new(&test_config(), 160, 80);
        let pos = Vec2::new(40.0, 40.0);
        grid.traverse(pos);
        let val = grid.road_value(pos);
        assert!((val - 0.002).abs() < 1e-6);
    }

    #[test]
    fn traverse_accumulates() {
        let mut grid = RoadGrid::new(&test_config(), 160, 80);
        let pos = Vec2::new(40.0, 40.0);
        for _ in 0..100 {
            grid.traverse(pos);
        }
        let val = grid.road_value(pos);
        assert!((val - 0.2).abs() < 1e-5);
    }

    #[test]
    fn traverse_clamps_at_one() {
        let mut grid = RoadGrid::new(&test_config(), 160, 80);
        let pos = Vec2::new(40.0, 40.0);
        for _ in 0..1000 {
            grid.traverse(pos);
        }
        let val = grid.road_value(pos);
        assert!((val - 1.0).abs() < 1e-6);
    }

    #[test]
    fn decay_reduces_values() {
        let mut grid = RoadGrid::new(&test_config(), 160, 80);
        let pos = Vec2::new(40.0, 40.0);
        grid.traverse(pos);
        let before = grid.road_value(pos);
        grid.tick();
        let after = grid.road_value(pos);
        assert!(after < before);
    }

    #[test]
    fn speed_multiplier_range() {
        let mut grid = RoadGrid::new(&test_config(), 160, 80);
        let pos = Vec2::new(40.0, 40.0);
        // No road: multiplier should be 1.0.
        assert!((grid.speed_multiplier(pos) - 1.0).abs() < 1e-6);
        // Max road: multiplier should be 1.6.
        for _ in 0..1000 {
            grid.traverse(pos);
        }
        assert!((grid.speed_multiplier(pos) - 1.6).abs() < 1e-5);
    }

    #[test]
    fn out_of_bounds_returns_zero() {
        let grid = RoadGrid::new(&test_config(), 160, 80);
        assert!((grid.road_value(Vec2::new(-5.0, 10.0))).abs() < 1e-6);
        assert!((grid.road_value(Vec2::new(200.0, 10.0))).abs() < 1e-6);
        assert!((grid.speed_multiplier(Vec2::new(-5.0, 10.0)) - 1.0).abs() < 1e-6);
    }
}
