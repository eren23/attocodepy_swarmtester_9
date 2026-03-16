use macroquad::prelude::*;

use ::rand::Rng;
use crate::types::{Commodity, ReputationChannel, TerrainType};
use crate::world::world::World;

use super::hud::InspectorTarget;
use super::Camera;

// ── Input state ────────────────────────────────────────────────────────────

pub struct InputState {
    pub paused: bool,
    pub speed_mult: f32,
    pub show_help: bool,
    pub show_economy_hud: bool,
    pub show_trade_arrows: bool,
    pub show_price_chart: bool,
    pub show_wealth_histogram: bool,
    pub show_road_overlay: bool,
    pub show_heatmap: bool,
    pub reputation_channel: Option<ReputationChannel>,
    pub inspector: Option<InspectorTarget>,
    /// Accumulated drag delta for middle-click pan.
    drag_start: Option<(f32, f32)>,
    /// Right-click drag painting.
    #[allow(dead_code)]
    painting: bool,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            paused: false,
            speed_mult: 1.0,
            show_help: false,
            show_economy_hud: false,
            show_trade_arrows: false,
            show_price_chart: false,
            show_wealth_histogram: false,
            show_road_overlay: false,
            show_heatmap: false,
            reputation_channel: None,
            inspector: None,
            drag_start: None,
            painting: false,
        }
    }

    pub fn ticks_per_frame(&self) -> u32 {
        // speed_mult: 0.5, 1, 2, 4, 8, 10
        // At 0.5x we tick every other frame via caller logic.
        (self.speed_mult as u32).max(1)
    }

    pub fn should_tick_this_frame(&self, frame: u64) -> bool {
        if self.speed_mult < 1.0 {
            // 0.5x: tick every 2nd frame.
            frame % 2 == 0
        } else {
            true
        }
    }
}

// ── Handle input ───────────────────────────────────────────────────────────

/// Process all input for one frame. Returns true if the user requested a
/// single-step tick (RIGHT arrow while paused).
pub fn handle_input(
    input: &mut InputState,
    cam: &mut Camera,
    world: &mut World,
    rng: &mut impl Rng,
) -> bool {
    let mut single_step = false;

    // ── Keyboard ───────────────────────────────────────────────────────
    if is_key_pressed(KeyCode::Space) {
        input.paused = !input.paused;
    }
    if is_key_pressed(KeyCode::Right) && input.paused {
        single_step = true;
    }

    // Speed: +/- between 0.5x and 10x.
    if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
        input.speed_mult = next_speed_up(input.speed_mult);
    }
    if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
        input.speed_mult = next_speed_down(input.speed_mult);
    }

    // Toggles.
    if is_key_pressed(KeyCode::S) {
        input.show_economy_hud = !input.show_economy_hud;
    }
    if is_key_pressed(KeyCode::A) {
        input.show_trade_arrows = !input.show_trade_arrows;
    }
    if is_key_pressed(KeyCode::P) {
        input.show_price_chart = !input.show_price_chart;
    }
    if is_key_pressed(KeyCode::E) {
        input.show_wealth_histogram = !input.show_wealth_histogram;
    }
    if is_key_pressed(KeyCode::O) {
        input.show_road_overlay = !input.show_road_overlay;
    }
    if is_key_pressed(KeyCode::H) {
        input.show_heatmap = !input.show_heatmap;
    }

    // Reputation overlay: 1-4 select channel, 0 off.
    if is_key_pressed(KeyCode::Key1) {
        input.reputation_channel = Some(ReputationChannel::Profit);
    }
    if is_key_pressed(KeyCode::Key2) {
        input.reputation_channel = Some(ReputationChannel::Demand);
    }
    if is_key_pressed(KeyCode::Key3) {
        input.reputation_channel = Some(ReputationChannel::Danger);
    }
    if is_key_pressed(KeyCode::Key4) {
        input.reputation_channel = Some(ReputationChannel::Opportunity);
    }
    if is_key_pressed(KeyCode::Key0) {
        input.reputation_channel = None;
    }

    // Help.
    if is_key_pressed(KeyCode::Slash) {
        // ? is Shift+/ on US layout, but also catch /
        input.show_help = !input.show_help;
    }

    // Close inspector.
    if is_key_pressed(KeyCode::Escape) {
        input.inspector = None;
    }

    // ── Events: C F G B W K ───────────────────────────────────────────
    if is_key_pressed(KeyCode::C) {
        trigger_market_crash(world);
    }
    if is_key_pressed(KeyCode::F) {
        trigger_famine(world);
    }
    if is_key_pressed(KeyCode::G) {
        trigger_gold_injection(world);
    }
    if is_key_pressed(KeyCode::B) {
        trigger_bandit_surge(world, rng);
    }
    if is_key_pressed(KeyCode::W) {
        world.season = crate::types::Season::Winter;
    }
    if is_key_pressed(KeyCode::K) {
        kill_random_merchants(world, rng);
    }

    // ── Mouse: zoom ───────────────────────────────────────────────────
    let (_, wheel_y) = mouse_wheel();
    if wheel_y.abs() > 0.1 {
        let factor = if wheel_y > 0.0 { 1.1 } else { 1.0 / 1.1 };
        let (mx, my) = mouse_position();
        cam.apply_zoom(factor, mx, my);
    }

    // ── Mouse: middle-drag pan ────────────────────────────────────────
    if is_mouse_button_pressed(MouseButton::Middle) {
        let (mx, my) = mouse_position();
        input.drag_start = Some((mx, my));
    }
    if is_mouse_button_down(MouseButton::Middle) {
        if let Some((sx, sy)) = input.drag_start {
            let (mx, my) = mouse_position();
            cam.pan(mx - sx, my - sy);
            input.drag_start = Some((mx, my));
        }
    }
    if is_mouse_button_released(MouseButton::Middle) && !is_mouse_button_down(MouseButton::Right) {
        // Middle-click (no drag) = place resource node.
        if let Some((sx, sy)) = input.drag_start {
            let (mx, my) = mouse_position();
            let dist = ((mx - sx).powi(2) + (my - sy).powi(2)).sqrt();
            if dist < 3.0 {
                let (wx, wy) = cam.screen_to_world(mx, my);
                place_resource_node(world, wx, wy, rng);
            }
        }
        input.drag_start = None;
    }

    // ── Mouse: left-click inspect ─────────────────────────────────────
    if is_mouse_button_pressed(MouseButton::Left) {
        let (mx, my) = mouse_position();
        input.inspector = find_inspector_target(world, cam, mx, my);
    }

    // ── Mouse: right-click drag paint ─────────────────────────────────
    if is_mouse_button_down(MouseButton::Right) {
        let (mx, my) = mouse_position();
        let (wx, wy) = cam.screen_to_world(mx, my);
        if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
            // Shift+right = clear to plains.
            paint_terrain(world, wx, wy, TerrainType::Plains);
        } else {
            paint_terrain(world, wx, wy, TerrainType::Mountains);
        }
    }

    single_step
}

// ── Speed steps ────────────────────────────────────────────────────────────

const SPEED_STEPS: [f32; 7] = [0.5, 1.0, 2.0, 4.0, 6.0, 8.0, 10.0];

fn next_speed_up(current: f32) -> f32 {
    for &s in &SPEED_STEPS {
        if s > current + 0.01 {
            return s;
        }
    }
    *SPEED_STEPS.last().unwrap()
}

fn next_speed_down(current: f32) -> f32 {
    for &s in SPEED_STEPS.iter().rev() {
        if s < current - 0.01 {
            return s;
        }
    }
    SPEED_STEPS[0]
}

// ── Inspector target finding ───────────────────────────────────────────────

fn find_inspector_target(
    world: &World,
    cam: &Camera,
    screen_x: f32,
    screen_y: f32,
) -> Option<InspectorTarget> {
    let (wx, wy) = cam.screen_to_world(screen_x, screen_y);
    let wpos = crate::types::Vec2::new(wx, wy);

    // Check merchants first (smaller targets, higher priority).
    let mut closest_merchant: Option<(u32, f32)> = None;
    for m in &world.merchants {
        if !m.alive {
            continue;
        }
        let d = m.pos.distance(wpos);
        if d < 10.0 / cam.zoom.max(0.1) {
            if closest_merchant.map_or(true, |(_, cd)| d < cd) {
                closest_merchant = Some((m.id, d));
            }
        }
    }
    if let Some((id, _)) = closest_merchant {
        return Some(InspectorTarget::Merchant(id));
    }

    // Check cities.
    for city in &world.cities {
        let d = city.position.distance(wpos);
        if d < city.radius {
            return Some(InspectorTarget::City(city.id));
        }
    }

    None
}

// ── World modification events ──────────────────────────────────────────────

fn trigger_market_crash(world: &mut World) {
    // Wipe all merchant gold by 50%.
    for m in &mut world.merchants {
        if m.alive {
            m.gold *= 0.5;
        }
    }
}

fn trigger_famine(world: &mut World) {
    // Clear all food from city warehouses.
    for city in &mut world.cities {
        city.warehouse.remove(&Commodity::Grain);
        city.warehouse.remove(&Commodity::Fish);
        city.warehouse.remove(&Commodity::Provisions);
    }
}

fn trigger_gold_injection(world: &mut World) {
    for m in &mut world.merchants {
        if m.alive {
            m.gold += 50.0;
        }
    }
}

fn trigger_bandit_surge(world: &mut World, _rng: &mut impl Rng) {
    // Double all active bandits' effective presence by activating inactive ones.
    for bandit in world.bandit_system.bandits_mut() {
        bandit.active = true;
    }
}

fn kill_random_merchants(world: &mut World, rng: &mut impl Rng) {
    // Kill ~20% of alive merchants.
    for m in &mut world.merchants {
        if m.alive && rng.gen::<f32>() < 0.2 {
            m.alive = false;
        }
    }
}

fn paint_terrain(world: &mut World, wx: f32, wy: f32, terrain_type: TerrainType) {
    let x = wx as u32;
    let y = wy as u32;
    if x < world.terrain.width() && y < world.terrain.height() {
        world.terrain.set_terrain_at(x, y, terrain_type);
        world.terrain.rebuild_components();
    }
}

fn place_resource_node(world: &mut World, wx: f32, wy: f32, rng: &mut impl Rng) {
    use crate::world::resource_node::ResourceNode;
    let commodity = Commodity::RAW[rng.gen_range(0..Commodity::RAW.len())];
    let id = world.resource_nodes.len() as u32;
    world.resource_nodes.push(ResourceNode::new_at(
        id,
        crate::types::Vec2::new(wx, wy),
        commodity,
    ));
}
