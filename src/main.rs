use ::rand::SeedableRng;
use clap::Parser;
use swarm_economy::config::EconomyConfig;
use swarm_economy::metrics::reporter;
use swarm_economy::metrics::tracker::MetricsTracker;
use swarm_economy::types::Season;
use swarm_economy::world::world::World;

// ── CLI Arguments ───────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "swarm-economy", about = "Emergent Market Economy Simulation")]
struct Args {
    /// Run in headless mode (no GUI, output JSON).
    #[arg(long)]
    headless: bool,

    /// Number of simulation ticks to run (headless mode).
    #[arg(long, default_value_t = 10000)]
    ticks: u32,

    /// Number of initial merchants.
    #[arg(long)]
    merchants: Option<u32>,

    /// Random seed.
    #[arg(long)]
    seed: Option<u64>,

    /// Disable bandits.
    #[arg(long)]
    no_bandits: bool,

    /// Lock season to summer (no season cycling).
    #[arg(long)]
    eternal_summer: bool,
}

// ── Headless Mode ───────────────────────────────────────────────────────────

fn run_headless(args: &Args) {
    let mut config = EconomyConfig::load("economy_config.toml").expect("failed to load config");

    // Apply CLI overrides.
    if let Some(merchants) = args.merchants {
        config.merchant.initial_population = merchants;
        config.merchant.max_population = config.merchant.max_population.max(merchants * 2);
    }

    let seed = args.seed.unwrap_or(config.world.terrain_seed as u64);

    if args.no_bandits {
        config.bandit.num_camps = 0;
    }

    if args.eternal_summer {
        // Set very long season to effectively lock to summer.
        config.world.season_length_ticks = u32::MAX / 2;
    }

    let initial_merchants = config.merchant.initial_population;
    let season_length = config.world.season_length_ticks;
    let eternal_summer = args.eternal_summer;

    // World::new allocates large terrain/reputation grids; run on a thread with
    // 16 MiB stack to avoid stack overflow.
    let (mut world, mut rng) = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut rng = ::rand::rngs::StdRng::seed_from_u64(seed);
            let mut world = World::new(config, &mut rng);
            if eternal_summer {
                world.season = Season::Summer;
            }
            (world, rng)
        })
        .expect("failed to spawn world init thread")
        .join()
        .expect("world init thread panicked");

    let mut tracker = MetricsTracker::new(args.ticks as usize);

    let total_ticks = args.ticks;

    // Progress reporting to stderr.
    let report_interval = (total_ticks / 10).max(1);

    for tick in 0..total_ticks {
        world.tick(&mut rng);

        // Count per-tick events from world state.
        let caravan_count = world.caravan_system.caravans().len() as u32;
        let robberies_this_tick = world
            .metrics_history
            .last()
            .map(|m| m.total_robberies)
            .unwrap_or(0);

        tracker.record(
            tick,
            world.season(),
            &world.merchants,
            &world.cities,
            &world.order_books,
            &world.roads,
            caravan_count,
            world.metrics_history.last().map(|m| m.total_trades).unwrap_or(0),
            robberies_this_tick,
            0, // bankruptcies tracked via cumulative counter in world
            0, // caravans formed — tracked in tracker cumulatively
        );

        if tick > 0 && tick % report_interval == 0 {
            let pct = (tick as f32 / total_ticks as f32 * 100.0) as u32;
            eprintln!("[{pct}%] tick {tick}/{total_ticks} — alive: {}", world.alive_merchant_count());
        }
    }

    // Collect bandit positions for emergence detection.
    let bandit_positions: Vec<swarm_economy::types::Vec2> = world
        .bandit_system
        .bandits()
        .iter()
        .filter(|b| b.active)
        .map(|b| b.position)
        .collect();

    let report = reporter::generate_report(
        &tracker,
        &world.merchants,
        &world.cities,
        &world.roads,
        &bandit_positions,
        season_length,
        total_ticks,
        initial_merchants,
        seed,
    );

    let json = reporter::report_to_json(&report);
    println!("{json}");
}

// ── GUI Mode ────────────────────────────────────────────────────────────────

#[cfg(not(feature = "headless_only"))]
mod gui {
    use macroquad::prelude::*;
    use super::*;
    use swarm_economy::rendering::{self, Camera};
    use swarm_economy::rendering::controls::InputState;

    pub async fn run_gui(args: &Args) {
        let mut config = EconomyConfig::load("economy_config.toml").expect("failed to load config");

        if let Some(merchants) = args.merchants {
            config.merchant.initial_population = merchants;
            config.merchant.max_population = config.merchant.max_population.max(merchants * 2);
        }

        let seed = args.seed.unwrap_or(config.world.terrain_seed as u64);

        if args.no_bandits {
            config.bandit.num_camps = 0;
        }

        let world_w = config.world.width as f32;
        let world_h = config.world.height as f32;
        let eternal_summer = args.eternal_summer;

        // World::new allocates large terrain/reputation grids; run on a thread with
        // 16 MiB stack to avoid stack overflow on the main thread.
        let (mut world, mut rng) = std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let mut rng = ::rand::rngs::StdRng::seed_from_u64(seed);
                let mut world = World::new(config, &mut rng);
                if eternal_summer {
                    world.season = Season::Summer;
                }
                (world, rng)
            })
            .expect("failed to spawn world init thread")
            .join()
            .expect("world init thread panicked");

        let mut input = InputState::new();
        let mut cam = Camera::fit_world(world_w, world_h);
        let mut frame_count: u64 = 0;

        loop {
            // Re-fit camera on Home key.
            if is_key_pressed(KeyCode::Home) {
                cam = Camera::fit_world(world_w, world_h);
            }

            // Process input.
            let single_step =
                rendering::handle_input(&mut input, &mut cam, &mut world, &mut rng);

            // Simulation.
            if single_step {
                world.tick(&mut rng);
            } else if !input.paused && input.should_tick_this_frame(frame_count) {
                for _ in 0..input.ticks_per_frame() {
                    world.tick(&mut rng);
                }
            }

            // Render.
            rendering::render_frame(&world, &cam, &input);

            frame_count += 1;
            next_frame().await;
        }
    }
}

// ── Entry Point ─────────────────────────────────────────────────────────────

#[cfg(feature = "headless_only")]
fn main() {
    let args = Args::parse();
    run_headless(&args);
}

#[cfg(not(feature = "headless_only"))]
#[macroquad::main("Swarm Economy")]
async fn main() {
    let args = Args::parse();

    if args.headless {
        run_headless(&args);
    } else {
        gui::run_gui(&args).await;
    }
}
