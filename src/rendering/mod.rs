pub mod controls;
pub mod hud;
pub mod price_chart;
pub mod renderer;
pub mod reputation_overlay;
pub mod trade_flow;

use macroquad::prelude::*;

use crate::world::world::World;
use controls::InputState;
use hud::InspectorTarget;

// Re-export the top-level draw call so main.rs stays thin.
pub use controls::handle_input;
pub use renderer::draw_world;

// ── Rendering constants ────────────────────────────────────────────────────

pub const BG_COLOR: Color = Color::new(0.078, 0.086, 0.110, 1.0);
pub const MARGIN: f32 = 10.0;
pub const HUD_TOP_HEIGHT: f32 = 34.0;

/// Camera state for smooth scrolling / zoom.
pub struct Camera {
    /// Offset in world-space (top-left corner of viewport).
    pub offset: Vec2,
    /// Pixels-per-world-unit zoom level.
    pub zoom: f32,
}

impl Camera {
    pub fn fit_world(world_w: f32, world_h: f32) -> Self {
        let vw = screen_width() - 2.0 * MARGIN;
        let vh = screen_height() - HUD_TOP_HEIGHT - MARGIN;
        let zoom = (vw / world_w).min(vh / world_h);
        Self {
            offset: Vec2::new(0.0, 0.0),
            zoom,
        }
    }

    /// Convert world coordinate to screen coordinate.
    #[inline]
    pub fn world_to_screen(&self, wx: f32, wy: f32) -> (f32, f32) {
        (
            MARGIN + (wx - self.offset.x) * self.zoom,
            HUD_TOP_HEIGHT + (wy - self.offset.y) * self.zoom,
        )
    }

    /// Convert screen coordinate to world coordinate.
    #[inline]
    pub fn screen_to_world(&self, sx: f32, sy: f32) -> (f32, f32) {
        (
            (sx - MARGIN) / self.zoom + self.offset.x,
            (sy - HUD_TOP_HEIGHT) / self.zoom + self.offset.y,
        )
    }

    /// Smooth-zoom toward mouse position.
    pub fn apply_zoom(&mut self, factor: f32, mouse_x: f32, mouse_y: f32) {
        let (wx, wy) = self.screen_to_world(mouse_x, mouse_y);
        self.zoom *= factor;
        self.zoom = self.zoom.clamp(0.1, 10.0);
        // Adjust offset so the world point under the mouse stays put.
        self.offset.x = wx - (mouse_x - MARGIN) / self.zoom;
        self.offset.y = wy - (mouse_y - HUD_TOP_HEIGHT) / self.zoom;
    }

    /// Pan by screen-space delta.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.offset.x -= dx / self.zoom;
        self.offset.y -= dy / self.zoom;
    }
}

/// Full rendering pass: world, overlays, HUD, inspectors.
pub fn render_frame(world: &World, cam: &Camera, input: &InputState) {
    clear_background(BG_COLOR);

    let world_w = world.config.world.width as f32;
    let world_h = world.config.world.height as f32;

    // World bounds.
    let (x0, y0) = cam.world_to_screen(0.0, 0.0);
    let (x1, y1) = cam.world_to_screen(world_w, world_h);
    draw_rectangle_lines(x0, y0, x1 - x0, y1 - y0, 1.0, GRAY);

    // Main world rendering.
    draw_world(world, cam, input);

    // Overlays.
    if input.show_road_overlay {
        renderer::draw_road_overlay(world, cam);
    }
    if let Some(channel) = input.reputation_channel {
        reputation_overlay::draw_reputation_overlay(world, cam, channel);
    }
    if input.show_trade_arrows {
        trade_flow::draw_trade_flows(world, cam);
    }

    // HUD.
    hud::draw_top_hud(world, input);
    if input.show_economy_hud {
        hud::draw_economy_hud(world);
    }

    // Inspectors.
    match &input.inspector {
        Some(InspectorTarget::Merchant(id)) => {
            hud::draw_merchant_inspector(world, *id);
        }
        Some(InspectorTarget::City(id)) => {
            hud::draw_city_inspector(world, *id);
        }
        None => {}
    }

    // Charts.
    if input.show_price_chart {
        price_chart::draw_price_chart(world);
    }
    if input.show_wealth_histogram {
        price_chart::draw_wealth_histogram(world);
    }

    // Season timeline at top.
    price_chart::draw_season_timeline(world);

    // Help overlay.
    if input.show_help {
        draw_help_overlay();
    }
}

fn draw_help_overlay() {
    let lines = [
        "Controls:",
        "  SPACE       Pause / Resume",
        "  RIGHT       Step one tick (paused)",
        "  +/-         Speed (0.5x - 10x)",
        "  Scroll      Zoom in/out",
        "  Middle-drag  Pan camera",
        "  Left-click  Inspect merchant/city",
        "  Right-drag  Paint mountains",
        "  Shift+Right Clear terrain",
        "  Mid-click   Place resource node",
        "  1-4         Reputation overlay",
        "  0           Overlay off",
        "  S           Economy HUD",
        "  A           Trade flow arrows",
        "  P           Price chart",
        "  E           Wealth histogram",
        "  O           Road overlay",
        "  H           Heatmap toggle",
        "  C           Market crash",
        "  F           Famine event",
        "  G           Gold injection",
        "  B           Bandit surge",
        "  W           Force winter",
        "  K           Kill merchants",
        "  ESC         Close inspector",
        "  ?           This help",
    ];
    let w = 320.0;
    let h = lines.len() as f32 * 18.0 + 20.0;
    let x = screen_width() / 2.0 - w / 2.0;
    let y = screen_height() / 2.0 - h / 2.0;
    draw_rectangle(x, y, w, h, Color::new(0.0, 0.0, 0.0, 0.85));
    for (i, line) in lines.iter().enumerate() {
        draw_text(line, x + 10.0, y + 18.0 + i as f32 * 18.0, 16.0, WHITE);
    }
}
