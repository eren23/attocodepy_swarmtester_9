use crate::config::{ChannelConfig, ReputationConfig};
use crate::types::{ReputationChannel, Vec2};

// ── Channel indices ───────────────────────────────────────────────────────

const PROFIT: usize = 0;
const DEMAND: usize = 1;
const DANGER: usize = 2;
const OPPORTUNITY: usize = 3;
const NUM_CHANNELS: usize = 4;

// ── Gaussian kernel ───────────────────────────────────────────────────────

struct GaussKernel {
    weights: Vec<f32>,
    radius: usize,
}

impl GaussKernel {
    fn new(sigma: f32) -> Self {
        if sigma < 1e-6 {
            return Self {
                weights: vec![1.0],
                radius: 0,
            };
        }
        let radius = (3.0 * sigma).ceil() as usize;
        let size = 2 * radius + 1;
        let mut weights = Vec::with_capacity(size);
        let inv_2s2 = 1.0 / (2.0 * sigma * sigma);
        let mut sum = 0.0f32;
        for i in 0..size {
            let d = i as f32 - radius as f32;
            let w = (-d * d * inv_2s2).exp();
            weights.push(w);
            sum += w;
        }
        let inv_sum = 1.0 / sum;
        for w in &mut weights {
            *w *= inv_sum;
        }
        Self { weights, radius }
    }
}

// ── ReputationGrid ────────────────────────────────────────────────────────

pub struct ReputationGrid {
    cols: usize,
    rows: usize,
    cell_size: f32,
    /// 4 flat arrays (row-major), one per channel.
    channels: [Vec<f32>; NUM_CHANNELS],
    decay: [f32; NUM_CHANNELS],
    kernels: [GaussKernel; NUM_CHANNELS],
    /// Reusable scratch buffer for separable blur.
    scratch: Vec<f32>,
}

impl ReputationGrid {
    pub fn new(config: &ReputationConfig, world_width: u32, world_height: u32) -> Self {
        let cs = config.cell_size as usize;
        let cols = (world_width as usize + cs - 1) / cs;
        let rows = (world_height as usize + cs - 1) / cs;
        let n = cols * rows;

        let ch: [&ChannelConfig; 4] = [
            &config.channels.profit,
            &config.channels.demand,
            &config.channels.danger,
            &config.channels.opportunity,
        ];

        Self {
            cols,
            rows,
            cell_size: config.cell_size as f32,
            channels: [
                vec![0.0; n],
                vec![0.0; n],
                vec![0.0; n],
                vec![0.0; n],
            ],
            decay: [ch[0].decay, ch[1].decay, ch[2].decay, ch[3].decay],
            kernels: [
                GaussKernel::new(ch[0].diffusion_sigma),
                GaussKernel::new(ch[1].diffusion_sigma),
                GaussKernel::new(ch[2].diffusion_sigma),
                GaussKernel::new(ch[3].diffusion_sigma),
            ],
            scratch: vec![0.0; n],
        }
    }

    // ── Grid helpers ──────────────────────────────────────────────────────

    #[inline]
    fn idx(&self, col: usize, row: usize) -> usize {
        row * self.cols + col
    }

    #[inline]
    fn world_to_grid(&self, pos: Vec2) -> (f32, f32) {
        (pos.x / self.cell_size, pos.y / self.cell_size)
    }

    #[inline]
    fn get_clamped(&self, ci: usize, col: isize, row: isize) -> f32 {
        let c = col.clamp(0, self.cols as isize - 1) as usize;
        let r = row.clamp(0, self.rows as isize - 1) as usize;
        self.channels[ci][r * self.cols + c]
    }

    #[inline]
    fn channel_index(ch: ReputationChannel) -> usize {
        match ch {
            ReputationChannel::Profit => PROFIT,
            ReputationChannel::Demand => DEMAND,
            ReputationChannel::Danger => DANGER,
            ReputationChannel::Opportunity => OPPORTUNITY,
        }
    }

    // ── Deposit ───────────────────────────────────────────────────────────

    /// Add `amount` to the given channel at a world position, clamped to 1.0.
    pub fn deposit(&mut self, channel: ReputationChannel, pos: Vec2, amount: f32) {
        if pos.x < 0.0 || pos.y < 0.0 {
            return;
        }
        let (gx, gy) = self.world_to_grid(pos);
        let col = gx as usize;
        let row = gy as usize;
        if col < self.cols && row < self.rows {
            let ci = Self::channel_index(channel);
            let i = self.idx(col, row);
            self.channels[ci][i] = (self.channels[ci][i] + amount).min(1.0);
        }
    }

    // ── Tick: diffusion + decay ───────────────────────────────────────────

    /// Run one tick: separable Gaussian diffusion then exponential decay.
    pub fn tick(&mut self) {
        for ci in 0..NUM_CHANNELS {
            self.diffuse(ci);
            self.apply_decay(ci);
        }
    }

    fn diffuse(&mut self, ci: usize) {
        if self.kernels[ci].radius == 0 {
            return;
        }
        self.blur_horizontal(ci);
        self.blur_vertical(ci);
    }

    fn blur_horizontal(&mut self, ci: usize) {
        let cols = self.cols;
        let rows = self.rows;
        let radius = self.kernels[ci].radius;
        let weights = &self.kernels[ci].weights;

        for r in 0..rows {
            let base = r * cols;
            for c in 0..cols {
                let mut sum = 0.0f32;
                for k in 0..weights.len() {
                    let sc = (c as isize + k as isize - radius as isize)
                        .clamp(0, cols as isize - 1) as usize;
                    sum += self.channels[ci][base + sc] * weights[k];
                }
                self.scratch[base + c] = sum;
            }
        }
    }

    fn blur_vertical(&mut self, ci: usize) {
        let cols = self.cols;
        let rows = self.rows;
        let radius = self.kernels[ci].radius;
        let weights = &self.kernels[ci].weights;

        for c in 0..cols {
            for r in 0..rows {
                let mut sum = 0.0f32;
                for k in 0..weights.len() {
                    let sr = (r as isize + k as isize - radius as isize)
                        .clamp(0, rows as isize - 1) as usize;
                    sum += self.scratch[sr * cols + c] * weights[k];
                }
                self.channels[ci][r * cols + c] = sum;
            }
        }
    }

    fn apply_decay(&mut self, ci: usize) {
        let d = self.decay[ci];
        for v in &mut self.channels[ci] {
            *v *= d;
        }
    }

    // ── Bilinear sampling ─────────────────────────────────────────────────

    /// Sample a channel at an arbitrary world position with bilinear interpolation.
    pub fn sample(&self, channel: ReputationChannel, pos: Vec2) -> f32 {
        self.sample_channel(Self::channel_index(channel), pos)
    }

    fn sample_channel(&self, ci: usize, pos: Vec2) -> f32 {
        let (gx, gy) = self.world_to_grid(pos);
        // Cell-centred: the value at cell (c,r) represents the center of that cell.
        let fx = gx - 0.5;
        let fy = gy - 0.5;
        let x0 = fx.floor() as isize;
        let y0 = fy.floor() as isize;
        let tx = fx - fx.floor();
        let ty = fy - fy.floor();

        let v00 = self.get_clamped(ci, x0, y0);
        let v10 = self.get_clamped(ci, x0 + 1, y0);
        let v01 = self.get_clamped(ci, x0, y0 + 1);
        let v11 = self.get_clamped(ci, x0 + 1, y0 + 1);

        let a = v00 + (v10 - v00) * tx;
        let b = v01 + (v11 - v01) * tx;
        a + (b - a) * ty
    }

    // ── Scanner cone sampling ─────────────────────────────────────────────

    /// Average signal in left/right cones around `heading`.
    ///
    /// Left cone: heading − half_angle … heading.
    /// Right cone: heading … heading + half_angle.
    /// Both sampled at distances up to `range` from `pos`.
    ///
    /// Returns `(left_avg, right_avg)`.
    pub fn scanner_sample(
        &self,
        channel: ReputationChannel,
        pos: Vec2,
        heading: f32,
        half_angle_rad: f32,
        range: f32,
    ) -> (f32, f32) {
        const ANGLE_STEPS: usize = 3;
        const DIST_STEPS: usize = 3;
        const N: f32 = (ANGLE_STEPS * DIST_STEPS) as f32;

        let ci = Self::channel_index(channel);
        let mut left_sum = 0.0f32;
        let mut right_sum = 0.0f32;

        for ai in 0..ANGLE_STEPS {
            // Spread samples evenly across the half-angle.
            let frac = (ai as f32 + 0.5) / ANGLE_STEPS as f32;
            let left_angle = heading - half_angle_rad * frac;
            let right_angle = heading + half_angle_rad * frac;

            for di in 0..DIST_STEPS {
                let dist = range * (di as f32 + 1.0) / DIST_STEPS as f32;

                left_sum += self.sample_channel(
                    ci,
                    Vec2::new(pos.x + dist * left_angle.cos(), pos.y + dist * left_angle.sin()),
                );
                right_sum += self.sample_channel(
                    ci,
                    Vec2::new(
                        pos.x + dist * right_angle.cos(),
                        pos.y + dist * right_angle.sin(),
                    ),
                );
            }
        }

        (left_sum / N, right_sum / N)
    }

    // ── Gradient ──────────────────────────────────────────────────────────

    /// Central-difference gradient of a channel at a world position.
    pub fn gradient(&self, channel: ReputationChannel, pos: Vec2) -> Vec2 {
        let ci = Self::channel_index(channel);
        let h = self.cell_size;
        let inv_2h = 1.0 / (2.0 * h);
        let dx = self.sample_channel(ci, Vec2::new(pos.x + h, pos.y))
            - self.sample_channel(ci, Vec2::new(pos.x - h, pos.y));
        let dy = self.sample_channel(ci, Vec2::new(pos.x, pos.y + h))
            - self.sample_channel(ci, Vec2::new(pos.x, pos.y - h));
        Vec2::new(dx * inv_2h, dy * inv_2h)
    }

    // ── Accessors ─────────────────────────────────────────────────────────

    pub fn raw_channel(&self, channel: ReputationChannel) -> &[f32] {
        &self.channels[Self::channel_index(channel)]
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
    use crate::config::{ChannelConfig, ReputationChannels, ReputationConfig};

    fn test_config() -> ReputationConfig {
        let ch = ChannelConfig {
            decay: 0.99,
            diffusion_sigma: 0.6,
            color: [255, 255, 255],
        };
        ReputationConfig {
            cell_size: 8,
            channels: ReputationChannels {
                profit: ch.clone(),
                demand: ch.clone(),
                danger: ch.clone(),
                opportunity: ch,
            },
        }
    }

    #[test]
    fn grid_dimensions() {
        let grid = ReputationGrid::new(&test_config(), 1600, 1000);
        assert_eq!(grid.cols(), 200);
        assert_eq!(grid.rows(), 125);
        assert_eq!(grid.cell_size(), 8.0);
    }

    #[test]
    fn deposit_adds_signal() {
        let mut grid = ReputationGrid::new(&test_config(), 160, 80);
        grid.deposit(ReputationChannel::Profit, Vec2::new(40.0, 40.0), 0.5);
        let val = grid.raw_channel(ReputationChannel::Profit)[5 * 20 + 5]; // col=5, row=5
        assert!((val - 0.5).abs() < 1e-6);
    }

    #[test]
    fn deposit_clamps_at_one() {
        let mut grid = ReputationGrid::new(&test_config(), 160, 80);
        let pos = Vec2::new(40.0, 40.0);
        grid.deposit(ReputationChannel::Profit, pos, 0.8);
        grid.deposit(ReputationChannel::Profit, pos, 0.5);
        let val = grid.raw_channel(ReputationChannel::Profit)[5 * 20 + 5];
        assert!((val - 1.0).abs() < 1e-6);
    }

    #[test]
    fn deposit_out_of_bounds_is_noop() {
        let mut grid = ReputationGrid::new(&test_config(), 160, 80);
        grid.deposit(ReputationChannel::Profit, Vec2::new(-10.0, 50.0), 1.0);
        grid.deposit(ReputationChannel::Profit, Vec2::new(200.0, 50.0), 1.0);
        let sum: f32 = grid.raw_channel(ReputationChannel::Profit).iter().sum();
        assert!(sum.abs() < 1e-6);
    }

    #[test]
    fn decay_reduces_signal() {
        let mut grid = ReputationGrid::new(&test_config(), 160, 80);
        grid.deposit(ReputationChannel::Profit, Vec2::new(40.0, 40.0), 1.0);
        let before = grid.raw_channel(ReputationChannel::Profit)[5 * 20 + 5];
        grid.tick();
        let after = grid.raw_channel(ReputationChannel::Profit)[5 * 20 + 5];
        assert!(after < before);
    }

    #[test]
    fn diffusion_spreads_signal() {
        let mut grid = ReputationGrid::new(&test_config(), 160, 80);
        grid.deposit(ReputationChannel::Profit, Vec2::new(80.0, 40.0), 1.0);
        // Neighbour should be zero before tick.
        let neighbour_before = grid.raw_channel(ReputationChannel::Profit)[5 * 20 + 11];
        assert!(neighbour_before.abs() < 1e-6);
        grid.tick();
        // After diffusion, neighbour should have some signal.
        let neighbour_after = grid.raw_channel(ReputationChannel::Profit)[5 * 20 + 11];
        assert!(neighbour_after > 0.0);
    }

    #[test]
    fn bilinear_sample_at_cell_center() {
        let mut grid = ReputationGrid::new(&test_config(), 160, 80);
        grid.deposit(ReputationChannel::Danger, Vec2::new(40.0, 40.0), 0.7);
        // Sample at the center of the cell (cell 5,5 -> world 44, 44 = (5+0.5)*8).
        let val = grid.sample(ReputationChannel::Danger, Vec2::new(44.0, 44.0));
        assert!((val - 0.7).abs() < 1e-4);
    }

    #[test]
    fn gradient_points_towards_source() {
        let mut grid = ReputationGrid::new(&test_config(), 320, 160);
        // Place strong signal to the right.
        grid.deposit(ReputationChannel::Profit, Vec2::new(200.0, 80.0), 1.0);
        // Sample gradient one cell-width left of the source so the +h sample
        // overlaps the signal via bilinear interpolation.
        let grad = grid.gradient(ReputationChannel::Profit, Vec2::new(196.0, 80.0));
        // x-component should be positive (pointing right, towards the source).
        assert!(grad.x > 0.0, "grad.x = {}", grad.x);
    }

    #[test]
    fn scanner_detects_asymmetry() {
        let mut grid = ReputationGrid::new(&test_config(), 320, 160);
        // Place signal to the right of a forward-facing agent.
        grid.deposit(ReputationChannel::Demand, Vec2::new(168.0, 80.0), 1.0);
        let heading = 0.0f32; // facing right
        let half_angle = 35.0f32.to_radians();
        // Agent at (160, 80) — signal is slightly right/ahead.
        let (left, right) = grid.scanner_sample(
            ReputationChannel::Demand,
            Vec2::new(120.0, 80.0),
            heading,
            half_angle,
            60.0,
        );
        // Right cone should pick up more signal.
        assert!(
            right >= left,
            "expected right >= left, got left={left}, right={right}"
        );
    }

    #[test]
    fn channels_are_independent() {
        let mut grid = ReputationGrid::new(&test_config(), 160, 80);
        grid.deposit(ReputationChannel::Profit, Vec2::new(40.0, 40.0), 1.0);
        let danger_sum: f32 = grid.raw_channel(ReputationChannel::Danger).iter().sum();
        assert!(danger_sum.abs() < 1e-6);
    }
}
