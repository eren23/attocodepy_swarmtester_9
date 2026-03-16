use macroquad::prelude::*;
use std::collections::HashMap;

use crate::types::{CityId, Commodity, Profession, Season};
use crate::world::world::World;

use super::controls::InputState;
use super::renderer::profession_color;

// ── Inspector target ───────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum InspectorTarget {
    Merchant(u32),
    City(CityId),
}

// ── Helpers ────────────────────────────────────────────────────────────────

pub fn season_name(s: Season) -> &'static str {
    match s {
        Season::Spring => "Spring",
        Season::Summer => "Summer",
        Season::Autumn => "Autumn",
        Season::Winter => "Winter",
    }
}

fn season_color(s: Season) -> Color {
    match s {
        Season::Spring => Color::new(0.4, 0.9, 0.4, 1.0),
        Season::Summer => Color::new(0.9, 0.9, 0.2, 1.0),
        Season::Autumn => Color::new(0.9, 0.5, 0.2, 1.0),
        Season::Winter => Color::new(0.6, 0.7, 0.9, 1.0),
    }
}

fn commodity_short(c: Commodity) -> &'static str {
    match c {
        Commodity::Timber => "Tim",
        Commodity::Ore => "Ore",
        Commodity::Grain => "Grn",
        Commodity::Fish => "Fsh",
        Commodity::Clay => "Cly",
        Commodity::Herbs => "Hrb",
        Commodity::Tools => "Tls",
        Commodity::Medicine => "Med",
        Commodity::Bricks => "Brk",
        Commodity::Metalwork => "Mtl",
        Commodity::Provisions => "Prv",
        Commodity::Pottery => "Pot",
        Commodity::Weapons => "Wpn",
        Commodity::Furniture => "Fur",
        Commodity::Armor => "Arm",
        Commodity::Alchemy => "Alc",
        Commodity::Machinery => "Mch",
        Commodity::FeastGoods => "Fst",
        Commodity::EliteGear => "EGr",
        Commodity::Automaton => "Aut",
        Commodity::Elixir => "Elx",
        Commodity::LuxurySet => "Lux",
    }
}

// ── Top HUD bar ────────────────────────────────────────────────────────────

pub fn draw_top_hud(world: &World, input: &InputState) {
    let status = if input.paused { "PAUSED" } else { "RUNNING" };
    let alive = world.alive_merchant_count();
    let hud = format!(
        "Tick: {} | {} | {} | Pop: {} | Speed: {}x | {:.0} FPS",
        world.tick_count(),
        season_name(world.season()),
        status,
        alive,
        input.speed_mult,
        get_fps(),
    );
    draw_text(&hud, 10.0, 20.0, 18.0, WHITE);

    // Season color indicator.
    let sc = season_color(world.season());
    draw_rectangle(screen_width() - 60.0, 6.0, 50.0, 18.0, sc);
    draw_text(season_name(world.season()), screen_width() - 58.0, 20.0, 14.0, BLACK);
}

// ── Economy HUD (toggled S) ────────────────────────────────────────────────

pub fn draw_economy_hud(world: &World) {
    let panel_w = 320.0;
    let panel_h = 520.0;
    let px = screen_width() - panel_w - 10.0;
    let py = 40.0;
    draw_rectangle(px, py, panel_w, panel_h, Color::new(0.0, 0.0, 0.0, 0.85));

    let mut y = py + 18.0;
    let x = px + 10.0;
    let lh = 16.0; // line height

    // Population alive/bankrupt.
    let alive = world.merchants.iter().filter(|m| m.alive).count();
    let dead = world.merchants.len() - alive;
    draw_text(&format!("Merchants: {} alive / {} bankrupt", alive, dead), x, y, 14.0, WHITE);
    y += lh;

    // Total gold.
    let total_gold: f32 = world.merchants.iter().filter(|m| m.alive).map(|m| m.gold).sum();
    draw_text(&format!("Total Gold: {:.0}", total_gold), x, y, 14.0, YELLOW);
    y += lh;

    // Trade volume last 200 ticks.
    let trade_vol = compute_recent_trade_volume(world, 200);
    draw_text(&format!("Trade Vol (200t): {:.0}", trade_vol), x, y, 14.0, WHITE);
    y += lh;

    // Average wealth + Gini.
    let (avg_wealth, gini) = compute_wealth_stats(world);
    draw_text(&format!("Avg Wealth: {:.1}  Gini: {:.3}", avg_wealth, gini), x, y, 14.0, WHITE);
    y += lh;

    // Reputation signal mass.
    let rep_mass = compute_reputation_mass(world);
    draw_text(&format!("Rep Signal Mass: {:.1}", rep_mass), x, y, 14.0, WHITE);
    y += lh;

    // Active caravan count.
    let caravans = world.caravan_system.active_count();
    draw_text(&format!("Caravans: {}", caravans), x, y, 14.0, WHITE);
    y += lh;

    // Bandit camp count.
    let camps = world.bandit_system.active_camp_count();
    draw_text(&format!("Bandit Camps: {}", camps), x, y, 14.0, Color::new(1.0, 0.4, 0.4, 1.0));
    y += lh + 4.0;

    // Per-commodity avg price bars.
    draw_text("Commodity Avg Prices:", x, y, 13.0, GRAY);
    y += lh;
    let bar_max_w = panel_w - 80.0;
    for &commodity in &Commodity::RAW {
        let avg = avg_commodity_price(world, commodity);
        let bar_w = (avg / 50.0).min(1.0) * bar_max_w;
        draw_rectangle(x + 30.0, y - 10.0, bar_w, 10.0, Color::new(0.3, 0.6, 0.9, 0.7));
        draw_text(commodity_short(commodity), x, y, 12.0, WHITE);
        draw_text(&format!("{:.1}", avg), x + 35.0 + bar_w, y, 11.0, GRAY);
        y += 14.0;
    }
    y += 4.0;

    // Profession distribution bars.
    draw_text("Professions:", x, y, 13.0, GRAY);
    y += lh;
    let prof_counts = profession_counts(world);
    let total = prof_counts.values().sum::<u32>().max(1) as f32;
    for &prof in &Profession::ALL {
        let count = *prof_counts.get(&prof).unwrap_or(&0);
        let frac = count as f32 / total;
        let bar_w = frac * bar_max_w;
        let color = profession_color(prof);
        draw_rectangle(x + 40.0, y - 10.0, bar_w, 10.0, color);
        draw_text(&format!("{:?}", prof), x, y, 12.0, WHITE);
        draw_text(&format!("{}", count), x + 45.0 + bar_w, y, 11.0, GRAY);
        y += 14.0;
    }
    y += 4.0;

    // Top 3 trade routes (city pairs by volume).
    draw_text("Top Trade Routes:", x, y, 13.0, GRAY);
    y += lh;
    let routes = top_trade_routes(world, 3);
    for (c1, c2, vol) in routes {
        draw_text(&format!("C{} <-> C{}: {:.0}", c1, c2, vol), x + 10.0, y, 12.0, WHITE);
        y += 14.0;
    }
}

// ── Merchant Inspector (left-click merchant) ───────────────────────────────

pub fn draw_merchant_inspector(world: &World, merchant_id: u32) {
    let m = match world.merchants.iter().find(|m| m.id == merchant_id) {
        Some(m) => m,
        None => return,
    };

    let panel_w = 280.0;
    let panel_h = 480.0;
    let px = 10.0;
    let py = 40.0;
    draw_rectangle(px, py, panel_w, panel_h, Color::new(0.0, 0.0, 0.0, 0.9));

    let mut y = py + 18.0;
    let x = px + 10.0;
    let lh = 15.0;

    let status = if m.alive { "ALIVE" } else { "DEAD" };
    draw_text(&format!("Merchant #{} [{}]", m.id, status), x, y, 14.0, profession_color(m.profession));
    y += lh;

    draw_text(&format!("Profession: {:?}", m.profession), x, y, 13.0, WHITE);
    y += lh;
    draw_text(&format!("State: {:?}", m.state), x, y, 13.0, WHITE);
    y += lh;
    draw_text(&format!("Pos: ({:.0}, {:.0})  Heading: {:.1}°", m.pos.x, m.pos.y, m.heading.to_degrees()), x, y, 12.0, GRAY);
    y += lh;
    draw_text(&format!("Gold: {:.1}  Fatigue: {:.1}", m.gold, m.fatigue), x, y, 13.0, YELLOW);
    y += lh;
    draw_text(&format!("Speed: {:.2}  Rep: {:.1}", m.speed, m.reputation), x, y, 13.0, WHITE);
    y += lh;
    draw_text(&format!("Home City: C{}  Age: {}", m.home_city, m.age), x, y, 13.0, WHITE);
    y += lh;

    // Traits.
    draw_text(&format!(
        "Traits: risk={:.2} greed={:.2} soc={:.2} loy={:.2}",
        m.traits.risk_tolerance, m.traits.greed, m.traits.sociability, m.traits.loyalty
    ), x, y, 11.0, GRAY);
    y += lh;

    // Caravan info.
    if let Some(cid) = m.caravan_id {
        draw_text(&format!("Caravan: #{}", cid), x, y, 13.0, Color::new(0.5, 0.8, 1.0, 1.0));
    } else {
        draw_text("Caravan: none", x, y, 13.0, GRAY);
    }
    y += lh + 4.0;

    // Inventory.
    draw_text("Inventory:", x, y, 13.0, GRAY);
    y += lh;
    let total_carry: f32 = m.inventory.values().sum();
    draw_text(&format!("  {:.1} / {:.1}", total_carry, m.max_carry), x, y, 12.0, WHITE);
    y += lh;
    for (&commodity, &qty) in &m.inventory {
        if qty > 0.001 {
            draw_text(&format!("  {:?}: {:.2}", commodity, qty), x, y, 12.0, WHITE);
            y += 13.0;
        }
    }
    y += 4.0;

    // Price memory (last few entries).
    draw_text("Price Memory:", x, y, 13.0, GRAY);
    y += lh;
    let mut count = 0;
    for (&city_id, commodities) in m.price_memory.all_entries() {
        for (&commodity, entry) in commodities {
            if count >= 8 {
                break;
            }
            draw_text(
                &format!("  C{} {:?}: {:.1} @t{}", city_id, commodity, entry.price, entry.observed_tick),
                x, y, 11.0, GRAY,
            );
            y += 13.0;
            count += 1;
        }
    }
    y += 4.0;

    // Recent ledger.
    draw_text("Ledger (last 5):", x, y, 13.0, GRAY);
    y += lh;
    let ledger_iter = m.ledger.iter().rev().take(5);
    for tx in ledger_iter {
        let side = if tx.buyer_id == m.id { "BUY" } else { "SELL" };
        draw_text(
            &format!("  {} {:?} x{:.1} @{:.1} C{}", side, tx.commodity, tx.quantity, tx.price, tx.city_id),
            x, y, 11.0, WHITE,
        );
        y += 13.0;
    }
}

// ── City Inspector (left-click city) ───────────────────────────────────────

pub fn draw_city_inspector(world: &World, city_id: CityId) {
    let city = match world.cities.iter().find(|c| c.id == city_id) {
        Some(c) => c,
        None => return,
    };

    let panel_w = 280.0;
    let panel_h = 420.0;
    let px = 10.0;
    let py = 40.0;
    draw_rectangle(px, py, panel_w, panel_h, Color::new(0.0, 0.0, 0.0, 0.9));

    let mut y = py + 18.0;
    let x = px + 10.0;
    let lh = 15.0;

    draw_text(&format!("City #{}", city.id), x, y, 14.0, Color::new(0.9, 0.9, 0.3, 1.0));
    y += lh;
    draw_text(&format!("Pos: ({:.0}, {:.0})  Coastal: {}", city.position.x, city.position.y, city.is_coastal), x, y, 12.0, GRAY);
    y += lh;
    draw_text(&format!("Population: {:.0}", city.population), x, y, 13.0, WHITE);
    y += lh;
    draw_text(&format!("Treasury: {:.1}", city.treasury), x, y, 13.0, YELLOW);
    y += lh;
    draw_text(&format!("Tax Rate: {:.1}%", city.tax_rate * 100.0), x, y, 13.0, WHITE);
    y += lh;
    draw_text(&format!("Prosperity: {:.1}", city.prosperity), x, y, 13.0, WHITE);
    y += lh;
    draw_text(&format!("Specialization: {:?}", city.specialization), x, y, 13.0, WHITE);
    y += lh;
    draw_text(&format!("Trade Volume: {:.0}", city.trade_volume), x, y, 13.0, WHITE);
    y += lh;

    // Upgrades.
    let upgrades: Vec<String> = city.upgrades.iter().map(|u| format!("{:?}", u)).collect();
    let upgrades_str = if upgrades.is_empty() {
        "none".to_string()
    } else {
        upgrades.join(", ")
    };
    draw_text(&format!("Upgrades: {}", upgrades_str), x, y, 12.0, WHITE);
    y += lh + 4.0;

    // Warehouse contents.
    draw_text("Warehouse:", x, y, 13.0, GRAY);
    y += lh;
    let total_wh: f32 = city.warehouse.values().sum();
    draw_text(&format!("  Total: {:.0} / {:.0}", total_wh, world.config.city.warehouse_capacity), x, y, 12.0, WHITE);
    y += lh;
    for (&commodity, &qty) in &city.warehouse {
        if qty > 0.01 {
            draw_text(&format!("  {:?}: {:.1}", commodity, qty), x, y, 12.0, WHITE);
            y += 13.0;
        }
    }
    y += 4.0;

    // Order book summary.
    if let Some(book) = world.order_books.iter().find(|b| b.city_id() == city_id) {
        draw_text("Order Book:", x, y, 13.0, GRAY);
        y += lh;
        let mut shown = 0;
        for &commodity in &Commodity::ALL {
            let buys = book.order_count(commodity, crate::types::Side::Buy);
            let sells = book.order_count(commodity, crate::types::Side::Sell);
            if buys > 0 || sells > 0 {
                draw_text(
                    &format!("  {:?}: {}B / {}S", commodity, buys, sells),
                    x, y, 11.0, WHITE,
                );
                y += 13.0;
                shown += 1;
                if shown >= 10 {
                    break;
                }
            }
        }
    }
}

// ── Computation helpers ────────────────────────────────────────────────────

fn compute_recent_trade_volume(world: &World, window: usize) -> f32 {
    let start = world.metrics_history.len().saturating_sub(window);
    world.metrics_history[start..]
        .iter()
        .map(|m| m.total_trades as f32)
        .sum()
}

fn compute_wealth_stats(world: &World) -> (f32, f32) {
    let wealths: Vec<f32> = world
        .merchants
        .iter()
        .filter(|m| m.alive)
        .map(|m| m.gold)
        .collect();
    if wealths.is_empty() {
        return (0.0, 0.0);
    }
    let n = wealths.len() as f32;
    let mean = wealths.iter().sum::<f32>() / n;

    // Gini coefficient.
    let mut sum_diff = 0.0f64;
    for &a in &wealths {
        for &b in &wealths {
            sum_diff += (a as f64 - b as f64).abs();
        }
    }
    let gini = if mean.abs() < 0.01 {
        0.0
    } else {
        (sum_diff / (2.0 * n as f64 * n as f64 * mean as f64)) as f32
    };

    (mean, gini)
}

fn compute_reputation_mass(world: &World) -> f32 {
    use crate::types::ReputationChannel;
    let mut total = 0.0f32;
    for &ch in &ReputationChannel::ALL {
        total += world.reputation.raw_channel(ch).iter().sum::<f32>();
    }
    total
}

fn profession_counts(world: &World) -> HashMap<Profession, u32> {
    let mut counts = HashMap::new();
    for m in &world.merchants {
        if m.alive {
            *counts.entry(m.profession).or_insert(0) += 1;
        }
    }
    counts
}

fn avg_commodity_price(world: &World, commodity: Commodity) -> f32 {
    let mut sum = 0.0f32;
    let mut count = 0;
    for book in &world.order_books {
        if let Some(price) = book.avg_price(commodity, 100) {
            sum += price;
            count += 1;
        }
    }
    if count > 0 {
        sum / count as f32
    } else {
        0.0
    }
}

fn top_trade_routes(world: &World, n: usize) -> Vec<(CityId, CityId, f32)> {
    // Build trade volume between city pairs from recent merchant ledgers.
    let mut route_vol: HashMap<(CityId, CityId), f32> = HashMap::new();

    for m in &world.merchants {
        if !m.alive {
            continue;
        }
        // Look at last 20 ledger entries.
        for tx in m.ledger.iter().rev().take(20) {
            // Use home_city + tx city as a route approximation.
            let a = m.home_city.min(tx.city_id);
            let b = m.home_city.max(tx.city_id);
            if a != b {
                *route_vol.entry((a, b)).or_insert(0.0) += tx.price * tx.quantity;
            }
        }
    }

    let mut routes: Vec<_> = route_vol.into_iter().map(|((a, b), v)| (a, b, v)).collect();
    routes.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    routes.truncate(n);
    routes
}
