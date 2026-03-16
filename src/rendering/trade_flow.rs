use macroquad::prelude::*;

use crate::world::world::World;

use super::Camera;

/// Draw animated arrows between cities proportional to trade volume.
pub fn draw_trade_flows(world: &World, cam: &Camera) {
    let tick = world.tick_count();

    // Compute pairwise trade volumes from city trade_volume.
    // We approximate by drawing arrows between all city pairs, scaled by
    // the minimum trade volume of the pair.
    let cities = &world.cities;
    if cities.len() < 2 {
        return;
    }

    // Collect recent transaction data from merchant ledgers to build
    // city-pair volume.
    let mut pair_vol = std::collections::HashMap::<(u32, u32), f32>::new();
    for m in &world.merchants {
        if !m.alive {
            continue;
        }
        for tx in m.ledger.iter().rev().take(30) {
            // Route: home_city <-> tx.city_id
            let a = m.home_city.min(tx.city_id);
            let b = m.home_city.max(tx.city_id);
            if a != b {
                *pair_vol.entry((a, b)).or_insert(0.0) += tx.price * tx.quantity;
            }
        }
    }

    if pair_vol.is_empty() {
        return;
    }

    // Normalise volumes for line thickness.
    let max_vol = pair_vol.values().cloned().fold(0.0f32, f32::max);
    if max_vol < 0.01 {
        return;
    }

    for (&(ca, cb), &vol) in &pair_vol {
        let city_a = match cities.iter().find(|c| c.id == ca) {
            Some(c) => c,
            None => continue,
        };
        let city_b = match cities.iter().find(|c| c.id == cb) {
            Some(c) => c,
            None => continue,
        };

        let (ax, ay) = cam.world_to_screen(city_a.position.x, city_a.position.y);
        let (bx, by) = cam.world_to_screen(city_b.position.x, city_b.position.y);

        let thickness = 1.0 + (vol / max_vol) * 4.0;
        let alpha = 0.3 + (vol / max_vol) * 0.5;

        // Line.
        draw_line(ax, ay, bx, by, thickness, Color::new(0.3, 0.8, 1.0, alpha));

        // Animated dot traveling along the line.
        let cycle = 120.0; // frames per cycle
        let t = ((tick as f32) % cycle) / cycle;
        let dx = ax + (bx - ax) * t;
        let dy = ay + (by - ay) * t;
        draw_circle(dx, dy, thickness + 1.0, Color::new(0.5, 0.9, 1.0, alpha * 1.2));
    }
}
