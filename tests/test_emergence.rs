//! Headless accelerated integration tests for emergent behaviors.
//!
//! Each test spins up a full simulation with a deterministic seed,
//! runs for a specified number of ticks, and asserts that the
//! expected emergent behavior is observed using statistical measures.
//!
//! Tests run in spawned threads with large stacks to avoid overflow
//! from the simulation's deep call stacks.

mod common;

use std::collections::HashMap;
use std::thread;

use rand::rngs::StdRng;
use rand::SeedableRng;

use swarm_economy::config::EconomyConfig;
use swarm_economy::metrics::emergence;
use swarm_economy::metrics::inequality;
use swarm_economy::metrics::tracker::{MetricsTracker, TickSnapshot};
use swarm_economy::types::{Commodity, Profession, Vec2};
use swarm_economy::world::world::World;

const LARGE_STACK: usize = 64 * 1024 * 1024; // 64 MB

/// Wrap a test body in a large-stack thread to avoid stack overflow.
macro_rules! big_stack_test {
    ($name:ident, $body:block) => {
        #[test]
        fn $name() {
            let handle = thread::Builder::new()
                .name(stringify!($name).into())
                .stack_size(LARGE_STACK)
                .spawn(move || $body)
                .expect("spawn thread");
            handle.join().expect(concat!(stringify!($name), " panicked"));
        }
    };
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn world_with_seed(seed: u64) -> World {
    let config = EconomyConfig::load("economy_config.toml").expect("load config");
    let mut rng = StdRng::seed_from_u64(seed);
    World::new(config, &mut rng)
}

fn run_simulation(world: &mut World, rng: &mut StdRng, ticks: u32) -> MetricsTracker {
    let mut tracker = MetricsTracker::new(ticks as usize + 100);
    for _ in 0..ticks {
        world.tick(rng);
        let caravan_count = world.caravan_system.caravans().len() as u32;
        tracker.record(
            world.current_tick,
            world.season(),
            &world.merchants,
            &world.cities,
            &world.order_books,
            &world.roads,
            caravan_count,
            world.latest_metrics().map(|m| m.total_trades).unwrap_or(0),
            world.latest_metrics().map(|m| m.total_robberies).unwrap_or(0),
            0,
            0,
        );
    }
    tracker
}

fn city_hhi(warehouse: &HashMap<Commodity, f32>) -> f32 {
    let total: f32 = warehouse.values().sum();
    if total <= 0.0 {
        return 0.0;
    }
    warehouse.values().map(|&qty| { let s = qty / total; s * s }).sum()
}

fn pearson_r(x: &[f32], y: &[f32]) -> f32 {
    let n = x.len().min(y.len());
    if n < 2 { return 0.0; }
    let xm = x[..n].iter().sum::<f32>() / n as f32;
    let ym = y[..n].iter().sum::<f32>() / n as f32;
    let (mut cov, mut vx, mut vy) = (0.0_f64, 0.0_f64, 0.0_f64);
    for i in 0..n {
        let dx = x[i] as f64 - xm as f64;
        let dy = y[i] as f64 - ym as f64;
        cov += dx * dy; vx += dx * dx; vy += dy * dy;
    }
    let d = (vx * vy).sqrt();
    if d < 1e-12 { 0.0 } else { (cov / d) as f32 }
}

fn dbscan_count(points: &[Vec2], eps: f32, min_pts: usize) -> usize {
    let n = points.len();
    if n == 0 { return 0; }
    let eps2 = eps * eps;
    let mut labels = vec![-1i32; n];
    let mut cluster_id = 0i32;
    for i in 0..n {
        if labels[i] != -1 { continue; }
        let neighbors: Vec<usize> = (0..n).filter(|&j| (points[j] - points[i]).length_squared() <= eps2).collect();
        if neighbors.len() < min_pts { labels[i] = 0; continue; }
        cluster_id += 1;
        labels[i] = cluster_id;
        let mut queue = neighbors;
        let mut qi = 0;
        while qi < queue.len() {
            let j = queue[qi]; qi += 1;
            if labels[j] == 0 { labels[j] = cluster_id; }
            if labels[j] != -1 { continue; }
            labels[j] = cluster_id;
            let jn: Vec<usize> = (0..n).filter(|&k| (points[k] - points[j]).length_squared() <= eps2).collect();
            if jn.len() >= min_pts {
                for &k in &jn { if !queue.contains(&k) { queue.push(k); } }
            }
        }
    }
    cluster_id as usize
}

fn detrend(series: &[f32]) -> Vec<f32> {
    let n = series.len() as f64;
    if n < 2.0 { return series.to_vec(); }
    let xm = (n - 1.0) / 2.0;
    let ym: f64 = series.iter().map(|&v| v as f64).sum::<f64>() / n;
    let (mut num, mut den) = (0.0_f64, 0.0_f64);
    for (i, &y) in series.iter().enumerate() {
        let x = i as f64;
        num += (x - xm) * (y as f64 - ym);
        den += (x - xm).powi(2);
    }
    let b = if den.abs() > 1e-12 { num / den } else { 0.0 };
    let a = ym - b * xm;
    series.iter().enumerate().map(|(i, &y)| (y as f64 - a - b * i as f64) as f32).collect()
}

fn erfc(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let r = poly * (-x * x).exp();
    if x >= 0.0 { r } else { 2.0 - r }
}

fn ljung_box_p_value(series: &[f32], min_lag: usize, max_lag: usize) -> f32 {
    let n = series.len();
    if n < max_lag + 1 { return 1.0; }
    let mean: f64 = series.iter().map(|&v| v as f64).sum::<f64>() / n as f64;
    let var: f64 = series.iter().map(|&v| { let d = v as f64 - mean; d * d }).sum::<f64>() / n as f64;
    if var < 1e-12 { return 1.0; }
    let (mut q, mut nl) = (0.0_f64, 0usize);
    for lag in min_lag..=max_lag.min(n - 1) {
        let mut ac = 0.0_f64;
        for i in 0..(n - lag) { ac += (series[i] as f64 - mean) * (series[i + lag] as f64 - mean); }
        ac /= n as f64 * var;
        q += ac * ac / (n - lag) as f64;
        nl += 1;
    }
    q *= n as f64 * (n as f64 + 2.0);
    if nl == 0 { return 1.0; }
    let z = (q - nl as f64) / (2.0 * nl as f64).sqrt();
    (0.5 * erfc(z / std::f64::consts::SQRT_2)) as f32
}

fn price_variance(snapshots: &[TickSnapshot], commodity: Commodity) -> f32 {
    let prices: Vec<f32> = snapshots.iter().filter_map(|s| s.prices.get(&commodity).copied()).collect();
    if prices.len() < 2 { return 0.0; }
    let mean = prices.iter().sum::<f32>() / prices.len() as f32;
    prices.iter().map(|&p| (p - mean).powi(2)).sum::<f32>() / prices.len() as f32
}

fn road_entropy(world: &World) -> f32 {
    let cells = world.roads.raw_cells();
    let total: f64 = cells.iter().map(|&v| v as f64).sum();
    if total <= 0.0 { return 0.0; }
    let mut e = 0.0_f64;
    for &val in cells { if val > 0.0 { let p = val as f64 / total; e -= p * p.ln(); } }
    e as f32
}

fn measure_corridor_width(world: &World, a: Vec2, b: Vec2) -> f32 {
    let axis = b - a;
    let length = axis.length();
    if length < 1.0 { return 0.0; }
    let dir = axis * (1.0 / length);
    let perp = Vec2::new(-dir.y, dir.x);
    let num_samples = 20;
    let max_perp = 200.0;
    let step = 5.0;
    let mut max_width = 0.0f32;
    for i in 1..num_samples {
        let t = i as f32 / num_samples as f32;
        let center = a + axis * t;
        let threshold = 0.001;
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        let mut d = step;
        while d <= max_perp {
            if world.roads.road_value(center + perp * d) > threshold { hi = d; } else if hi > 0.0 { break; }
            d += step;
        }
        d = -step;
        while d >= -max_perp {
            if world.roads.road_value(center + perp * d) > threshold { lo = -d; } else if lo > 0.0 { break; }
            d -= step;
        }
        let w = lo + hi;
        if w > max_width { max_width = w; }
    }
    max_width
}

fn city_merchant_density(world: &World) -> Vec<f32> {
    let radius = 100.0;
    world.cities.iter().map(|city| {
        world.merchants.iter().filter(|m| m.alive && m.pos.distance(city.position) <= radius).count() as f32
    }).collect()
}

fn miner_fraction(world: &World) -> f32 {
    let alive: Vec<_> = world.merchants.iter().filter(|m| m.alive).collect();
    if alive.is_empty() { return 0.0; }
    alive.iter().filter(|m| m.profession == Profession::Miner).count() as f32 / alive.len() as f32
}

fn variance(values: &[f32]) -> f32 {
    if values.len() < 2 { return 0.0; }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    values.iter().map(|&v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32
}

// ── (1) Trade Route Formation ───────────────────────────────────────────────

big_stack_test!(emergence_trade_route_formation, {
    let mut config = EconomyConfig::load("economy_config.toml").expect("load config");
    config.world.num_cities = 2;
    config.merchant.initial_population = 200;
    config.merchant.max_population = 250;
    config.bandit.num_camps = 0;

    let mut rng = StdRng::seed_from_u64(42);
    let mut world = World::new(config, &mut rng);
    let initial_entropy = road_entropy(&world);

    for _ in 0..3000 { world.tick(&mut rng); }

    let final_entropy = road_entropy(&world);
    let city_a = world.cities[0].position;
    let city_b = world.cities[1].position;
    let corridor_width = measure_corridor_width(&world, city_a, city_b);

    assert!(corridor_width < 120.0,
        "Expected trade route corridor < 120px, got {corridor_width:.1}px");

    if initial_entropy > 0.0 {
        let decrease = (initial_entropy - final_entropy) / initial_entropy;
        assert!(decrease > 0.0, "Expected road entropy to decrease, got {decrease:.3}");
    }
});

// ── (2) Market Specialization ───────────────────────────────────────────────

big_stack_test!(emergence_market_specialization, {
    let mut world = world_with_seed(100);
    let mut rng = StdRng::seed_from_u64(100);
    for _ in 0..5000 { world.tick(&mut rng); }

    let hhi_values: Vec<f32> = world.cities.iter().map(|c| city_hhi(&c.warehouse)).collect();
    let cities_above = hhi_values.iter().filter(|&&h| h > 0.4).count();
    assert!(cities_above >= 3,
        "Expected >= 3 cities with HHI > 0.4, got {cities_above} (values: {hhi_values:?})");
});

// ── (3) Price Convergence ───────────────────────────────────────────────────

big_stack_test!(emergence_price_convergence, {
    let mut world = world_with_seed(200);
    let mut rng = StdRng::seed_from_u64(200);
    let tracker = run_simulation(&mut world, &mut rng, 5000);
    let snapshots = &tracker.snapshots;
    assert!(snapshots.len() >= 400, "Need >= 400 snapshots, got {}", snapshots.len());

    let early = &snapshots[..200.min(snapshots.len())];
    let late = &snapshots[snapshots.len().saturating_sub(200)..];
    let mut convergence_count = 0u32;
    for commodity in Commodity::ALL {
        let ev = price_variance(early, commodity);
        let lv = price_variance(late, commodity);
        if ev > 0.01 && (ev - lv) / ev > 0.3 { convergence_count += 1; }
    }
    assert!(convergence_count >= 4,
        "Expected >= 4 commodities with >= 30% variance decrease, got {convergence_count}");
});

// ── (4) Boom-Bust Cycles ────────────────────────────────────────────────────

big_stack_test!(emergence_boom_bust_cycles, {
    let mut world = world_with_seed(300);
    let mut rng = StdRng::seed_from_u64(300);
    let tracker = run_simulation(&mut world, &mut rng, 10000);
    let snapshots = &tracker.snapshots;
    let mut best_p = 1.0f32;
    for commodity in Commodity::ALL {
        let series: Vec<f32> = snapshots.iter().filter_map(|s| s.prices.get(&commodity).copied()).collect();
        if series.len() < 1000 { continue; }
        let dt = detrend(&series);
        let max_lag = (dt.len() / 2).min(3000);
        let min_lag = 500.min(max_lag);
        if min_lag >= max_lag { continue; }
        let p = ljung_box_p_value(&dt, min_lag, max_lag);
        if p < best_p { best_p = p; }
    }
    assert!(best_p < 0.05,
        "Expected Ljung-Box p < 0.05 for at least one commodity, best p = {best_p:.4}");
});

// ── (5) Seasonal Pricing ────────────────────────────────────────────────────

big_stack_test!(emergence_seasonal_pricing, {
    let mut world = world_with_seed(400);
    let mut rng = StdRng::seed_from_u64(400);
    let tracker = run_simulation(&mut world, &mut rng, 10000);
    let sl = world.config.world.season_length_ticks;
    let det = emergence::detect_seasonal_price_waves(&tracker.snapshots, sl);
    assert!(det.metric_value > 0.15,
        "Expected GRAIN seasonal amplitude > 15% of mean, got {:.3} ({:.1}%)",
        det.metric_value, det.metric_value * 100.0);
});

// ── (6) Wealth Inequality ───────────────────────────────────────────────────

big_stack_test!(emergence_wealth_inequality, {
    let mut world = world_with_seed(500);
    let mut rng = StdRng::seed_from_u64(500);
    for _ in 0..5000 { world.tick(&mut rng); }
    let golds: Vec<f32> = world.merchants.iter().filter(|m| m.alive).map(|m| m.gold.max(0.0)).collect();
    let gini = inequality::gini_coefficient(&golds);
    assert!(gini > 0.3, "Expected Gini > 0.3 after 5000 ticks, got {gini:.3}");
});

// ── (7) Guild Clustering ────────────────────────────────────────────────────

big_stack_test!(emergence_guild_clustering, {
    let mut world = world_with_seed(600);
    let mut rng = StdRng::seed_from_u64(600);
    for _ in 0..5000 { world.tick(&mut rng); }
    let alive: Vec<_> = world.merchants.iter().filter(|m| m.alive).collect();
    let (eps, min_pts) = (80.0, 3);
    let mut prof_clusters = 0u32;
    for prof in Profession::ALL {
        let pos: Vec<Vec2> = alive.iter().filter(|m| m.profession == prof).map(|m| m.pos).collect();
        if pos.len() < min_pts { continue; }
        let nc = dbscan_count(&pos, eps, min_pts);
        if nc > 0 && nc <= 4 { prof_clusters += 1; }
    }
    assert!(prof_clusters >= 2,
        "Expected >= 2 professions with DBSCAN <= 4 clusters, got {prof_clusters}");
});

// ── (8) Caravan-Danger Correlation ──────────────────────────────────────────

big_stack_test!(emergence_caravan_danger_correlation, {
    let mut world = world_with_seed(700);
    let mut rng = StdRng::seed_from_u64(700);
    let tracker = run_simulation(&mut world, &mut rng, 5000);
    let snapshots = &tracker.snapshots;
    let window = 100;
    if snapshots.len() < window * 2 { return; }
    let robbery_rate: Vec<f32> = snapshots.windows(window)
        .map(|w| w.iter().map(|s| s.robbery_count as f32).sum::<f32>() / window as f32).collect();
    let caravan_rate: Vec<f32> = snapshots.windows(window)
        .map(|w| w.iter().map(|s| s.caravan_count as f32).sum::<f32>() / window as f32).collect();
    let r = pearson_r(&robbery_rate, &caravan_rate);
    assert!(r > 0.3,
        "Expected Pearson r > 0.3 between caravan count and robbery rate, got {r:.3}");
});

// ── (9) Information Propagation ─────────────────────────────────────────────

big_stack_test!(emergence_information_propagation, {
    let mut world = world_with_seed(800);
    let mut rng = StdRng::seed_from_u64(800);
    for _ in 0..2000 { world.tick(&mut rng); }
    let alive: Vec<_> = world.merchants.iter().filter(|m| m.alive).collect();
    let total = alive.len();
    if total == 0 { return; }
    let informed = alive.iter().filter(|m| m.price_memory.all_entries().len() >= 2).count();
    let ratio = informed as f32 / total as f32;
    assert!(ratio > 0.5,
        "Expected > 50% merchants to know prices from >= 2 cities within 2000 ticks, got {:.1}% ({informed}/{total})",
        ratio * 100.0);
});

// ── (10) Economic Migration ─────────────────────────────────────────────────

big_stack_test!(emergence_economic_migration, {
    let mut world = world_with_seed(900);
    let mut rng = StdRng::seed_from_u64(900);
    let initial_density = city_merchant_density(&world);
    for _ in 0..1500 { world.tick(&mut rng); }
    let final_density = city_merchant_density(&world);
    let any_increased = (0..initial_density.len().min(final_density.len())).any(|i| {
        if initial_density[i] > 0.0 { (final_density[i] - initial_density[i]) / initial_density[i] >= 0.2 }
        else { final_density[i] > 0.0 }
    });
    assert!(any_increased,
        "Expected >= 20% density increase in at least one city within 1500 ticks. \
         Initial: {initial_density:?}, Final: {final_density:?}");
});

// ── (11) Profession Adaptation ──────────────────────────────────────────────

big_stack_test!(emergence_profession_adaptation, {
    let mut world = world_with_seed(1000);
    let mut rng = StdRng::seed_from_u64(1000);
    for _ in 0..2000 { world.tick(&mut rng); }
    let initial_miners = miner_fraction(&world);
    for node in &mut world.resource_nodes {
        if node.commodity == Commodity::Ore { node.depletion = 1.0; }
    }
    for _ in 0..3000 { world.tick(&mut rng); }
    let final_miners = miner_fraction(&world);
    if initial_miners > 0.01 {
        let drop = (initial_miners - final_miners) / initial_miners;
        assert!(drop >= 0.5,
            "Expected miner % to drop >= 50% after ORE removal, \
             initial={initial_miners:.3}, final={final_miners:.3}, drop={drop:.3}");
    }
});

// ── (12) City Growth ────────────────────────────────────────────────────────

big_stack_test!(emergence_city_growth, {
    let mut world = world_with_seed(1100);
    let mut rng = StdRng::seed_from_u64(1100);
    let initial_pops: Vec<f32> = world.cities.iter().map(|c| c.population).collect();
    for _ in 0..5000 { world.tick(&mut rng); }
    let final_pops: Vec<f32> = world.cities.iter().map(|c| c.population).collect();
    let initial_var = variance(&initial_pops);
    let final_var = variance(&final_pops);
    let var_increased = final_var > initial_var;
    let any_grew = initial_pops.iter().zip(final_pops.iter()).any(|(&i, &f)| f > i * 1.5);
    assert!(var_increased || any_grew,
        "Expected population variance to increase or at least one city > 1.5x initial. \
         Var: {initial_var:.1} -> {final_var:.1}, Pops: {initial_pops:?} -> {final_pops:?}");
});

// ── (13) Bandit Avoidance ───────────────────────────────────────────────────

big_stack_test!(emergence_bandit_avoidance, {
    let mut world = world_with_seed(1200);
    let mut rng = StdRng::seed_from_u64(1200);
    for _ in 0..2000 { world.tick(&mut rng); }
    let camp_positions: Vec<Vec2> = world.bandit_system.camps().iter()
        .filter(|c| c.alive).map(|c| c.position).collect();
    if camp_positions.is_empty() { return; }
    for _ in 0..1000 { world.tick(&mut rng); }
    let det = emergence::detect_bandit_avoidance(&world.roads, &camp_positions);
    assert!(det.metric_value > 0.0,
        "Expected some traffic reduction near bandit camps, got {:.3}", det.metric_value);
});

// ── (14) Tax Competition ────────────────────────────────────────────────────

big_stack_test!(emergence_tax_competition, {
    let mut world = world_with_seed(1300);
    let mut rng = StdRng::seed_from_u64(1300);
    for _ in 0..5000 { world.tick(&mut rng); }
    let taxes: Vec<f32> = world.cities.iter().map(|c| c.tax_rate).collect();
    let volumes: Vec<f32> = world.cities.iter().map(|c| c.trade_volume).collect();
    let r = pearson_r(&taxes, &volumes);
    assert!(r < -0.2,
        "Expected Pearson r < -0.2 between tax rate and trade volume, got {r:.3}");
});
