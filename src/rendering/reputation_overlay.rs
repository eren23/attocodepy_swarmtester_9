use macroquad::prelude::*;

use crate::types::ReputationChannel;
use crate::world::world::World;

use super::Camera;

/// Draw a semi-transparent heatmap overlay for a single reputation channel.
/// Color per channel comes from config ([r,g,b]).
pub fn draw_reputation_overlay(world: &World, cam: &Camera, channel: ReputationChannel) {
    let cols = world.reputation.cols();
    let rows = world.reputation.rows();
    let cs = world.reputation.cell_size();
    let data = world.reputation.raw_channel(channel);

    // Channel color from config.
    let [cr, cg, cb] = channel_color(&world.config.reputation, channel);
    let base_r = cr as f32 / 255.0;
    let base_g = cg as f32 / 255.0;
    let base_b = cb as f32 / 255.0;

    // Visible bounds.
    let (vx0, vy0) = cam.screen_to_world(0.0, super::HUD_TOP_HEIGHT);
    let (vx1, vy1) = cam.screen_to_world(screen_width(), screen_height());
    let c_start = ((vx0 / cs).max(0.0) as usize).min(cols);
    let c_end = ((vx1 / cs).ceil() as usize).min(cols);
    let r_start = ((vy0 / cs).max(0.0) as usize).min(rows);
    let r_end = ((vy1 / cs).ceil() as usize).min(rows);

    let px_size = cs * cam.zoom;

    for r in r_start..r_end {
        for c in c_start..c_end {
            let val = data[r * cols + c];
            if val < 0.001 {
                continue;
            }
            let alpha = val.min(1.0) * 0.5;
            let wx = c as f32 * cs;
            let wy = r as f32 * cs;
            let (sx, sy) = cam.world_to_screen(wx, wy);
            draw_rectangle(sx, sy, px_size, px_size, Color::new(base_r, base_g, base_b, alpha));
        }
    }

    // Legend in top-right corner.
    let label = match channel {
        ReputationChannel::Profit => "PROFIT",
        ReputationChannel::Demand => "DEMAND",
        ReputationChannel::Danger => "DANGER",
        ReputationChannel::Opportunity => "OPPORTUNITY",
    };
    let lx = screen_width() - 140.0;
    let ly = 38.0;
    draw_rectangle(lx - 5.0, ly - 12.0, 135.0, 18.0, Color::new(0.0, 0.0, 0.0, 0.7));
    draw_text(
        &format!("Overlay: {}", label),
        lx,
        ly,
        14.0,
        Color::new(base_r, base_g, base_b, 1.0),
    );
}

fn channel_color(
    config: &crate::config::ReputationConfig,
    channel: ReputationChannel,
) -> [u8; 3] {
    match channel {
        ReputationChannel::Profit => config.channels.profit.color,
        ReputationChannel::Demand => config.channels.demand.color,
        ReputationChannel::Danger => config.channels.danger.color,
        ReputationChannel::Opportunity => config.channels.opportunity.color,
    }
}
