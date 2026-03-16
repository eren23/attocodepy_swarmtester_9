use macroquad::prelude::*;

use crate::types::{Commodity, Profession, TerrainType};
use crate::world::world::World;

use super::controls::InputState;
use super::Camera;

// ── Color helpers ──────────────────────────────────────────────────────────

pub fn profession_color(prof: Profession) -> Color {
    match prof {
        Profession::Trader => Color::new(0.0, 0.8, 0.4, 1.0),
        Profession::Miner => Color::new(0.8, 0.5, 0.2, 1.0),
        Profession::Farmer => Color::new(0.4, 0.8, 0.1, 1.0),
        Profession::Craftsman => Color::new(0.9, 0.7, 0.1, 1.0),
        Profession::Soldier => Color::new(0.9, 0.2, 0.2, 1.0),
        Profession::Shipwright => Color::new(0.2, 0.6, 0.9, 1.0),
        Profession::Idle => Color::new(0.5, 0.5, 0.5, 1.0),
    }
}

fn terrain_color(t: TerrainType) -> Color {
    match t {
        TerrainType::Plains => Color::new(0.28, 0.36, 0.22, 1.0),
        TerrainType::Forest => Color::new(0.13, 0.26, 0.12, 1.0),
        TerrainType::Hills => Color::new(0.42, 0.38, 0.28, 1.0),
        TerrainType::Mountains => Color::new(0.50, 0.50, 0.52, 1.0),
        TerrainType::Water => Color::new(0.10, 0.18, 0.35, 1.0),
        TerrainType::Coast => Color::new(0.22, 0.35, 0.45, 1.0),
    }
}

fn commodity_color(c: Commodity) -> Color {
    match c {
        Commodity::Timber => Color::new(0.55, 0.35, 0.15, 1.0),
        Commodity::Ore => Color::new(0.50, 0.50, 0.55, 1.0),
        Commodity::Grain => Color::new(0.85, 0.75, 0.20, 1.0),
        Commodity::Fish => Color::new(0.30, 0.55, 0.80, 1.0),
        Commodity::Clay => Color::new(0.70, 0.45, 0.30, 1.0),
        Commodity::Herbs => Color::new(0.30, 0.70, 0.30, 1.0),
        _ => Color::new(0.60, 0.60, 0.60, 1.0),
    }
}

fn prosperity_color(prosperity: f32) -> Color {
    // 0 = red, 50 = yellow, 100 = green
    let t = (prosperity / 100.0).clamp(0.0, 1.0);
    if t < 0.5 {
        let f = t * 2.0;
        Color::new(1.0, f, 0.0, 1.0)
    } else {
        let f = (t - 0.5) * 2.0;
        Color::new(1.0 - f, 1.0, 0.0, 1.0)
    }
}

// ── Main draw ──────────────────────────────────────────────────────────────

/// Draw terrain, cities, resource nodes, merchants, bandits, roads.
pub fn draw_world(world: &World, cam: &Camera, input: &InputState) {
    draw_terrain(world, cam);
    draw_roads(world, cam);
    draw_resource_nodes(world, cam);
    draw_cities(world, cam);
    draw_bandits(world, cam);
    draw_merchants(world, cam, input);
}

// ── Terrain ────────────────────────────────────────────────────────────────

fn draw_terrain(world: &World, cam: &Camera) {
    let tw = world.terrain.width();
    let th = world.terrain.height();

    // Determine visible cell range and step size based on zoom.
    // At low zoom, skip cells to avoid drawing millions of rects.
    let step = (1.0 / cam.zoom).max(1.0).ceil() as u32;
    let cell_px = cam.zoom * step as f32;

    // Only draw if cells would be at least 1px.
    if cell_px < 0.5 {
        return;
    }

    // Visible world bounds.
    let (vx0, vy0) = cam.screen_to_world(0.0, super::HUD_TOP_HEIGHT);
    let (vx1, vy1) = cam.screen_to_world(screen_width(), screen_height());
    let x_start = (vx0.max(0.0) as u32 / step) * step;
    let y_start = (vy0.max(0.0) as u32 / step) * step;
    let x_end = ((vx1 as u32).min(tw)).min(tw);
    let y_end = ((vy1 as u32).min(th)).min(th);

    let mut y = y_start;
    while y < y_end {
        let mut x = x_start;
        while x < x_end {
            let t = world.terrain.terrain_at(x.min(tw - 1), y.min(th - 1));
            let (sx, sy) = cam.world_to_screen(x as f32, y as f32);
            draw_rectangle(sx, sy, cell_px, cell_px, terrain_color(t));
            x += step;
        }
        y += step;
    }
}

// ── Roads overlay ──────────────────────────────────────────────────────────

pub fn draw_road_overlay(world: &World, cam: &Camera) {
    let cols = world.roads.cols();
    let rows = world.roads.rows();
    let cs = world.roads.cell_size();
    let cells = world.roads.raw_cells();

    for r in 0..rows {
        for c in 0..cols {
            let val = cells[r * cols + c];
            if val < 0.001 {
                continue;
            }
            let wx = c as f32 * cs;
            let wy = r as f32 * cs;
            let (sx, sy) = cam.world_to_screen(wx, wy);
            let sz = cs * cam.zoom;
            let alpha = val.min(1.0) * 0.6;
            draw_rectangle(sx, sy, sz, sz, Color::new(0.9, 0.8, 0.5, alpha));
        }
    }
}

// ── Roads (darkened cells proportional to road_value) ──────────────────────

fn draw_roads(world: &World, cam: &Camera) {
    let cols = world.roads.cols();
    let rows = world.roads.rows();
    let cs = world.roads.cell_size();
    let cells = world.roads.raw_cells();

    // Visible bounds check.
    let (vx0, vy0) = cam.screen_to_world(0.0, super::HUD_TOP_HEIGHT);
    let (vx1, vy1) = cam.screen_to_world(screen_width(), screen_height());
    let c_start = ((vx0 / cs).max(0.0) as usize).min(cols);
    let c_end = ((vx1 / cs).ceil() as usize).min(cols);
    let r_start = ((vy0 / cs).max(0.0) as usize).min(rows);
    let r_end = ((vy1 / cs).ceil() as usize).min(rows);

    for r in r_start..r_end {
        for c in c_start..c_end {
            let val = cells[r * cols + c];
            if val < 0.005 {
                continue;
            }
            let wx = c as f32 * cs;
            let wy = r as f32 * cs;
            let (sx, sy) = cam.world_to_screen(wx, wy);
            let sz = cs * cam.zoom;
            // Darken terrain proportional to road value — subtle brown tint.
            let alpha = val.min(1.0) * 0.3;
            draw_rectangle(sx, sy, sz, sz, Color::new(0.35, 0.25, 0.15, alpha));
        }
    }
}

// ── Cities ─────────────────────────────────────────────────────────────────

fn draw_cities(world: &World, cam: &Camera) {
    for city in &world.cities {
        let (cx, cy) = cam.world_to_screen(city.position.x, city.position.y);
        let r = city.radius * cam.zoom;

        // Radius ring colored by prosperity.
        let pcolor = prosperity_color(city.prosperity);
        draw_circle_lines(cx, cy, r, 1.5, Color::new(pcolor.r, pcolor.g, pcolor.b, 0.5));

        // Population-scaled center dot.
        let pop_norm = (city.population / 500.0).clamp(0.1, 1.0);
        let dot_r = 2.0 + pop_norm * 6.0;
        draw_circle(cx, cy, dot_r, pcolor);

        // City name (id) when zoomed in enough.
        if cam.zoom > 0.5 {
            let label = format!("C{}", city.id);
            draw_text(&label, cx + dot_r + 2.0, cy - 2.0, 14.0, WHITE);
        }
    }
}

// ── Resource Nodes ─────────────────────────────────────────────────────────

fn draw_resource_nodes(world: &World, cam: &Camera) {
    for node in &world.resource_nodes {
        let (nx, ny) = cam.world_to_screen(node.position.x, node.position.y);
        let alpha = (1.0 - node.depletion).max(0.15);
        let color = commodity_color(node.commodity);
        let c = Color::new(color.r, color.g, color.b, alpha);

        // Small diamond shape for resource nodes.
        let s = 3.0;
        draw_line(nx, ny - s, nx + s, ny, 1.5, c);
        draw_line(nx + s, ny, nx, ny + s, 1.5, c);
        draw_line(nx, ny + s, nx - s, ny, 1.5, c);
        draw_line(nx - s, ny, nx, ny - s, 1.5, c);
    }
}

// ── Merchants ──────────────────────────────────────────────────────────────

fn draw_merchants(world: &World, cam: &Camera, _input: &InputState) {
    for m in &world.merchants {
        if !m.alive {
            continue;
        }
        let (mx, my) = cam.world_to_screen(m.pos.x, m.pos.y);
        let color = profession_color(m.profession);

        // Small triangle pointing in heading direction.
        let size = 3.0;
        let cos_h = m.heading.cos();
        let sin_h = m.heading.sin();

        // Triangle: tip at heading, two rear corners.
        let tip_x = mx + cos_h * size * 1.5;
        let tip_y = my + sin_h * size * 1.5;
        let left_x = mx + (-sin_h * size - cos_h * size * 0.5);
        let left_y = my + (cos_h * size - sin_h * size * 0.5);
        let right_x = mx + (sin_h * size - cos_h * size * 0.5);
        let right_y = my + (-cos_h * size - sin_h * size * 0.5);

        draw_triangle(
            macroquad::math::Vec2::new(tip_x, tip_y),
            macroquad::math::Vec2::new(left_x, left_y),
            macroquad::math::Vec2::new(right_x, right_y),
            color,
        );
    }
}

// ── Bandits ────────────────────────────────────────────────────────────────

fn draw_bandits(world: &World, cam: &Camera) {
    let bandit_color = Color::new(1.0, 0.1, 0.1, 0.7);
    for bandit in world.bandit_system.bandits() {
        if !bandit.active {
            continue;
        }
        let (bx, by) = cam.world_to_screen(bandit.position.x, bandit.position.y);
        draw_circle(bx, by, 2.0, bandit_color);
    }

    // Draw camp indicators when zoomed in.
    if cam.zoom > 0.3 {
        for camp in world.bandit_system.camps() {
            if !camp.alive {
                continue;
            }
            let (cx, cy) = cam.world_to_screen(camp.position.x, camp.position.y);
            let r = camp.patrol_radius * cam.zoom;
            draw_circle_lines(cx, cy, r, 1.0, Color::new(1.0, 0.2, 0.2, 0.15));
        }
    }
}
