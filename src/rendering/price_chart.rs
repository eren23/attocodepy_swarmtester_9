use macroquad::prelude::*;

use crate::types::{Commodity, Season};
use crate::world::world::World;

// ── Commodity colors for chart lines ───────────────────────────────────────

fn chart_commodity_color(i: usize) -> Color {
    const PALETTE: [Color; 6] = [
        Color::new(0.9, 0.3, 0.3, 1.0), // Timber - red
        Color::new(0.5, 0.5, 0.7, 1.0), // Ore - steel
        Color::new(0.9, 0.8, 0.2, 1.0), // Grain - yellow
        Color::new(0.3, 0.6, 0.9, 1.0), // Fish - blue
        Color::new(0.8, 0.5, 0.3, 1.0), // Clay - brown
        Color::new(0.3, 0.8, 0.4, 1.0), // Herbs - green
    ];
    PALETTE[i % PALETTE.len()]
}

// ── Live multi-line commodity price chart (toggled P) ──────────────────────

pub fn draw_price_chart(world: &World) {
    let chart_w = 400.0;
    let chart_h = 200.0;
    let cx = screen_width() / 2.0 - chart_w / 2.0;
    let cy = screen_height() - chart_h - 20.0;

    // Background.
    draw_rectangle(cx, cy, chart_w, chart_h, Color::new(0.0, 0.0, 0.0, 0.85));
    draw_rectangle_lines(cx, cy, chart_w, chart_h, 1.0, GRAY);

    draw_text("Commodity Prices (last 1000 ticks)", cx + 10.0, cy + 14.0, 13.0, WHITE);

    let plot_x = cx + 5.0;
    let plot_y = cy + 20.0;
    let plot_w = chart_w - 10.0;
    let plot_h = chart_h - 30.0;

    // Collect price histories for raw commodities across all city order books.
    let commodities = Commodity::RAW;
    let window = 1000usize;

    // Find global min/max for y-axis.
    let mut global_min = f32::MAX;
    let mut global_max = f32::MIN;

    let mut series: Vec<Vec<(u32, f32)>> = Vec::new();
    for &commodity in &commodities {
        let mut points = Vec::new();
        // Aggregate price history from all order books.
        for book in &world.order_books {
            if let Some(history) = book.price_history(commodity) {
                let start = history.len().saturating_sub(window);
                for i in start..history.len() {
                    let rec = &history[i];
                    points.push((rec.tick, rec.price));
                    if rec.price < global_min {
                        global_min = rec.price;
                    }
                    if rec.price > global_max {
                        global_max = rec.price;
                    }
                }
            }
        }
        // Sort by tick and deduplicate by averaging same-tick prices.
        points.sort_by_key(|p| p.0);
        series.push(points);
    }

    if global_max <= global_min {
        draw_text("No price data yet", plot_x + 10.0, plot_y + plot_h / 2.0, 14.0, GRAY);
        return;
    }

    // Y-axis scale.
    let y_range = global_max - global_min;
    let y_pad = y_range * 0.1;
    let y_min = global_min - y_pad;
    let y_max = global_max + y_pad;

    // X-axis: tick range.
    let current_tick = world.tick_count();
    let x_min = current_tick.saturating_sub(window as u32);
    let x_max = current_tick;
    let x_range = (x_max - x_min).max(1) as f32;

    // Draw grid lines.
    for i in 0..5 {
        let frac = i as f32 / 4.0;
        let gy = plot_y + plot_h * (1.0 - frac);
        draw_line(plot_x, gy, plot_x + plot_w, gy, 0.5, Color::new(0.3, 0.3, 0.3, 0.5));
        let val = y_min + (y_max - y_min) * frac;
        draw_text(&format!("{:.0}", val), plot_x + plot_w + 2.0, gy + 4.0, 10.0, GRAY);
    }

    // Draw each commodity series.
    for (i, points) in series.iter().enumerate() {
        let color = chart_commodity_color(i);
        if points.len() < 2 {
            continue;
        }
        for j in 1..points.len() {
            let (t0, p0) = points[j - 1];
            let (t1, p1) = points[j];

            let sx0 = plot_x + ((t0 - x_min) as f32 / x_range) * plot_w;
            let sy0 = plot_y + plot_h - ((p0 - y_min) / (y_max - y_min)) * plot_h;
            let sx1 = plot_x + ((t1 - x_min) as f32 / x_range) * plot_w;
            let sy1 = plot_y + plot_h - ((p1 - y_min) / (y_max - y_min)) * plot_h;

            draw_line(sx0, sy0, sx1, sy1, 1.5, color);
        }
    }

    // Legend.
    let lx = cx + 10.0;
    let ly = cy + chart_h - 8.0;
    for (i, &commodity) in commodities.iter().enumerate().rev() {
        let color = chart_commodity_color(i);
        draw_rectangle(lx + i as f32 * 55.0, ly - 8.0, 8.0, 8.0, color);
        draw_text(&format!("{:?}", commodity), lx + i as f32 * 55.0 + 10.0, ly, 10.0, WHITE);
    }
    let _ = ly;
}

// ── Wealth histogram (toggled E) ──────────────────────────────────────────

pub fn draw_wealth_histogram(world: &World) {
    let hist_w = 300.0;
    let hist_h = 160.0;
    let hx = 10.0;
    let hy = screen_height() - hist_h - 20.0;

    draw_rectangle(hx, hy, hist_w, hist_h, Color::new(0.0, 0.0, 0.0, 0.85));
    draw_rectangle_lines(hx, hy, hist_w, hist_h, 1.0, GRAY);

    draw_text("Wealth Distribution", hx + 10.0, hy + 14.0, 13.0, WHITE);

    let wealths: Vec<f32> = world
        .merchants
        .iter()
        .filter(|m| m.alive)
        .map(|m| m.gold)
        .collect();

    if wealths.is_empty() {
        draw_text("No data", hx + 10.0, hy + 80.0, 14.0, GRAY);
        return;
    }

    let num_bins = 20usize;
    let min_w = wealths.iter().cloned().fold(f32::MAX, f32::min).min(0.0);
    let max_w = wealths.iter().cloned().fold(f32::MIN, f32::max).max(1.0);
    let range = max_w - min_w;
    let bin_size = range / num_bins as f32;

    let mut bins = vec![0u32; num_bins];
    for &w in &wealths {
        let idx = ((w - min_w) / bin_size).floor() as usize;
        let idx = idx.min(num_bins - 1);
        bins[idx] += 1;
    }

    let max_count = *bins.iter().max().unwrap_or(&1);
    let plot_x = hx + 5.0;
    let plot_y = hy + 22.0;
    let plot_w = hist_w - 10.0;
    let plot_h = hist_h - 32.0;
    let bar_w = plot_w / num_bins as f32;

    for (i, &count) in bins.iter().enumerate() {
        let bar_h = (count as f32 / max_count as f32) * plot_h;
        let bx = plot_x + i as f32 * bar_w;
        let by = plot_y + plot_h - bar_h;

        let t = i as f32 / num_bins as f32;
        let color = Color::new(0.2 + t * 0.6, 0.7 - t * 0.4, 0.3, 0.8);
        draw_rectangle(bx, by, bar_w - 1.0, bar_h, color);
    }

    // Axis labels.
    draw_text(&format!("{:.0}", min_w), plot_x, plot_y + plot_h + 12.0, 10.0, GRAY);
    draw_text(
        &format!("{:.0}", max_w),
        plot_x + plot_w - 30.0,
        plot_y + plot_h + 12.0,
        10.0,
        GRAY,
    );
}

// ── Season timeline at top ─────────────────────────────────────────────────

pub fn draw_season_timeline(world: &World) {
    let bar_y = 28.0;
    let bar_h = 4.0;
    let bar_w = screen_width() - 20.0;
    let bar_x = 10.0;

    // Background.
    draw_rectangle(bar_x, bar_y, bar_w, bar_h, Color::new(0.2, 0.2, 0.2, 0.5));

    // Compute progress within current season.
    let season_len = world.config.world.season_length_ticks as f32;
    let total_cycle = season_len * 4.0;
    let progress_in_cycle = (world.tick_count() as f32 % total_cycle) / total_cycle;

    // Draw season segments.
    let seasons = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];
    let colors = [
        Color::new(0.4, 0.9, 0.4, 0.6),
        Color::new(0.9, 0.9, 0.2, 0.6),
        Color::new(0.9, 0.5, 0.2, 0.6),
        Color::new(0.6, 0.7, 0.9, 0.6),
    ];

    for (i, _season) in seasons.iter().enumerate() {
        let seg_x = bar_x + (i as f32 / 4.0) * bar_w;
        let seg_w = bar_w / 4.0;
        draw_rectangle(seg_x, bar_y, seg_w, bar_h, colors[i]);
    }

    // Current position marker.
    let marker_x = bar_x + progress_in_cycle * bar_w;
    draw_rectangle(marker_x - 1.0, bar_y - 1.0, 3.0, bar_h + 2.0, WHITE);
}
