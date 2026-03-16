# Swarm Goal

Emergent Market Economy Simulation — Rust + macroquad

Build a real-time economy simulation where hundreds of autonomous merchant
agents follow local rules that produce complex emergent behavior: trade
route formation, market specialization, price cycles, supply chain networks,
caravan formation, guild clustering, city growth and decline, and economic
migration. Implemented in Rust for performance, rendered with macroquad at
60 FPS. A full test suite validates every layer from individual agent
accounting up to emergent macroeconomic metrics.

---

## 0) World

A continuous 2D world (default 1600×1000 pixels) representing a geographic
trade map with:

### Cities (8–12)
Circular settlement nodes (radius ~30px) placed via Poisson disk sampling.
Each city has:
- A **local market** with buy/sell order books per commodity.
- A **population** (50–500) that generates passive demand and grows/shrinks
  based on trade activity and food supply.
- A **tax rate** (0–15%) that affects trade profitability. Tax rate adjusts
  dynamically: high trade volume → city raises tax; low volume → city
  lowers tax to attract merchants.
- A **warehouse** with limited storage capacity (500 units total across
  all commodities). Overstocked goods decay at 0.1%/tick.
- A **prosperity score** (0–100) derived from population, trade volume,
  warehouse fullness, and diversity of goods available. Affects NPC demand
  strength and merchant spawn rate at that city.
- A **specialization affinity**: each city has a 1.5× crafting speed bonus
  for one random recipe category, creating natural comparative advantage.
- **City growth**: population increases by 0.01/tick when prosperity > 60
  and food goods (GRAIN, FISH, PROVISIONS) are stocked. Population
  decreases by 0.02/tick when food supply is zero for > 200 ticks.
- **Unlockable upgrades** (purchased with accumulated tax revenue):
  - **Market hall** (cost 500g): order book capacity doubles.
  - **Walls** (cost 800g): bandits cannot raid this city.
  - **Harbor** (cost 1000g, coastal cities only): enables ship trade routes.
  - **Workshop** (cost 600g): unlocks Tier 3 crafting recipes at this city.

### Resource Nodes (20–30)
Extraction sites scattered on the map. Each produces one of 6 raw
commodities: `TIMBER`, `ORE`, `GRAIN`, `FISH`, `CLAY`, `HERBS`.

- **Yield rate**: 1–5 units per extraction action.
- **Depletion**: cumulative extraction reduces yield. At 80% depletion,
  yield drops to 50%. At 100%, node is exhausted (produces nothing).
- **Regeneration**: depleted nodes recover at 0.05%/tick when not being
  actively harvested. Exhausted nodes recover to 20% capacity after 2000
  ticks of no extraction.
- **Seasonal modifiers**: GRAIN and HERBS yield ×2 in summer, ×0.3 in
  winter. FISH yield ×1.5 in spring, ×0.5 in autumn. TIMBER and ORE
  are season-independent. CLAY yield ×0 when frozen (winter, if node is
  in northern 30% of map).

### Terrain
A continuous heightmap (Perlin noise, 4 octaves) defining:
- `PLAINS` (1.0× speed) — height 0.0–0.3
- `FOREST` (0.6× speed) — height 0.3–0.5
- `HILLS` (0.4× speed) — height 0.5–0.7
- `MOUNTAINS` (impassable) — height 0.7–1.0
- `WATER` (impassable on foot) — below sea level (0.25)
- `COAST` (0.8× speed on foot; navigable by ships) — water cells adjacent
  to land

### Roads (Emergent)
No roads exist initially. A terrain overlay grid (cell size ~8px) where
repeated merchant traversal gradually improves a cell's speed multiplier:
- Each traversal: `+0.002` to cell road value (clamped at 1.0).
- Decay: `×0.9998` per tick without traffic.
- Speed bonus: `1.0 + 0.6 × road_value`.
- Visual: cells darken proportionally to road value, producing visible
  emergent road networks.

### Seasons (cycling every 2500 ticks)
`SPRING → SUMMER → AUTUMN → WINTER → SPRING ...`

Each season affects:
- Resource yields (see above).
- Travel speed: winter ×0.7 global modifier on all terrain.
- City food consumption: winter ×1.5 (increased demand → price spikes).
- Bandit activity: summer ×1.3, winter ×0.5.
- Ship travel: disabled in winter (harbors freeze).

### Reputation Grid
A discrete grid overlay (cell size ~8px) with 4 signal channels:
- `PROFIT` — "I made money trading near here"
- `DEMAND` — "buyers want goods in this direction"
- `DANGER` — "bandits, bankruptcy, or loss occurred here"
- `OPPORTUNITY` — "underserved market or rich resource nearby"

Each channel: independent Gaussian diffusion (σ per config) and exponential
decay per tick.

### Bandits (NPC Threats)
5–10 bandit camps spawn in FOREST/HILLS terrain, away from cities. Each
camp produces 2–4 bandit agents that:
- Patrol a radius of 100–200px around their camp.
- Attack merchants within 15px: steal 20–40% of carried goods + 10–30%
  of gold.
- Merchants who are robbed deposit strong `DANGER` signals.
- Bandits avoid cities with WALLS upgrade.
- Bandit camps are destroyed if no successful robbery in 3000 ticks
  (starvation). New camps spawn to maintain target count.
- Soldier-profession merchants within 25px of a robbery will fight:
  50% chance to defeat bandit (bandit removed) vs 50% chance soldier
  loses 30% gold and all inventory.

---

## 1) Merchant Agent

Each merchant is an autonomous agent with:

### State
```rust
struct Merchant {
    id: u32,
    pos: Vec2,                    // continuous position
    heading: f32,                 // radians
    speed: f32,                   // pixels/tick (base ~1.5)
    gold: f32,                    // current liquid wealth
    inventory: HashMap<Commodity, f32>,  // commodity → quantity carried
    max_carry: f32,               // total inventory capacity (weight units)
    profession: Profession,       // TRADER | CRAFTSMAN | MINER | FARMER | SOLDIER | SHIPWRIGHT | IDLE
    reputation: f32,              // 0–100, affects trade prices
    fatigue: f32,                 // 0–100
    alive: bool,
    age: u32,                     // ticks alive
    home_city: CityId,
    state: AgentState,            // current behavior state (FSM)
    price_memory: PriceMemory,    // per-city, per-commodity last-known prices
    ledger: VecDeque<Transaction>,// recent transaction history (last 50)
    caravan_id: Option<u32>,      // if part of a caravan group
    traits: MerchantTraits,       // personality modifiers
}

struct MerchantTraits {
    risk_tolerance: f32,    // 0–1: low = avoids DANGER zones, high = ignores
    greed: f32,             // 0–1: high = chases bigger margins, low = takes safe trades
    sociability: f32,       // 0–1: high = forms caravans easily, shares gossip more
    loyalty: f32,           // 0–1: high = stays with home city, low = migrates freely
}
```

### Professions (7)
- **TRADER**: buys low, sells high between cities. Core economy driver.
- **MINER**: extracts ORE and CLAY from resource nodes.
- **FARMER**: extracts GRAIN, HERBS, FISH from resource nodes.
- **CRAFTSMAN**: converts raw materials → refined goods at cities.
- **SOLDIER**: escorts caravans, fights bandits, patrols trade routes.
  Income from caravan protection fees (paid by nearby merchants per tick).
- **SHIPWRIGHT**: operates between coastal cities with harbors. Moves along
  COAST cells at 2.0× speed (simulating boat travel). Can carry 3× normal
  inventory. Cannot traverse inland.
- **IDLE**: newly spawned or between professions. Wanders toward nearest city.

### Sensory Input (per tick)
```rust
struct SensoryInput {
    // Scanner cones: left/right, ±35° from heading, range 60px
    left_scanner: [f32; 4],   // avg reputation per channel in left cone
    right_scanner: [f32; 4],  // avg reputation per channel in right cone

    // Terrain raycasts: 5 rays, 120° arc, range 40px
    terrain_rays: [TerrainRay; 5],  // (distance, terrain_type, road_value)

    // Nearby agents within 30px
    neighbors: Vec<NeighborInfo>,  // (relative_pos, profession, inventory_fullness, reputation, caravan_id)

    // Navigation
    nearest_city: (Vec2, f32),     // (direction, distance)
    home_city: (Vec2, f32),        // (direction, distance)
    nearest_resource: Option<(Vec2, f32, Commodity)>,  // closest relevant resource node

    // Market intelligence
    profit_gradient: Vec2,         // local PROFIT reputation gradient
    danger_gradient: Vec2,         // local DANGER reputation gradient

    // Self state
    gold: f32,
    fatigue: f32,
    inventory_fill_ratio: f32,
    inventory_breakdown: HashMap<Commodity, f32>,
    current_terrain: TerrainType,
    current_season: Season,
    reputation: f32,

    // Bandit proximity
    nearest_bandit: Option<(Vec2, f32)>,  // (direction, distance) if within 80px
}
```

### Actions (output of brain per tick)
```rust
struct MerchantAction {
    turn: f32,                     // delta heading, clamped [-π/6, π/6]
    speed_mult: f32,               // 0.0–1.0
    deposit_signal: Option<ReputationChannel>,
    signal_strength: f32,          // 0.0–1.0
    market_action: MarketAction,   // None | Buy{commodity, max_price, quantity} | Sell{commodity, min_price, quantity}
    extract: bool,                 // harvest at resource node
    craft: Option<Recipe>,         // attempt crafting
    rest: bool,                    // recover fatigue (at city)
    join_caravan: bool,            // attempt to join/form caravan with nearby merchants
    leave_caravan: bool,           // leave current caravan
}
```

### Physics / Movement
- Heading updated by `turn`, then position advanced by
  `speed × speed_mult × terrain_mult × road_mult × fatigue_mult × season_mult`.
- `fatigue_mult = max(0.3, 1.0 - fatigue / 200.0)`.
- Caravan movement: all members move at speed of slowest member but in
  a cohesive group (leader picks heading, others follow with slight offset).
- Mountain/water collision: slide along edge.
- World bounds: reflect heading.
- Fatigue cost: `0.03 + 0.02 × speed_mult + 0.04 × (inventory_weight / max_carry)`.
- Fatigue ≥ 100 → collapse: drop 15% of inventory as ground items, fatigue → 80.
- At city: fatigue recovers at 1.5/tick. Can access market, craft, rest.
- Gold < 0 for 200 consecutive ticks → bankrupt → removed from simulation.
  All inventory dropped. A `DANGER` signal deposited at death location.

### Caravans (Emergent Grouping)
Merchants can form temporary caravans for safety:
- **Formation**: merchant with `sociability > 0.5` near 2+ other merchants
  heading in similar direction (heading difference < π/4) can initiate.
- **Benefits**: bandits will not attack caravans of 4+ merchants. Caravans
  of 3 have 70% chance of repelling attack. Information sharing is instant
  within caravan (all price memories merged).
- **Dissolution**: caravan dissolves when members diverge (>100px spread)
  or when destination city is reached.
- **Soldier escort**: soldiers near a caravan automatically attach and
  receive 0.01g/tick from each caravan member as protection fee.

### Gossip System
Merchants within 25px of each other automatically exchange information:
- One random price entry from each merchant's memory is shared.
- If either merchant knows about a bandit camp location, that info is shared.
- Gossip rate scales with `sociability` trait: `base_chance × (s1 + s2) / 2`.
- Within caravans: full price memory merge (all entries shared).

---

## 2) Brain: Rule-Based State Machine

The brain is a sophisticated multi-state FSM per profession with personality
modulation via `MerchantTraits`:

### Trader FSM
```
SCOUTING → BUYING → TRANSPORTING → SELLING → RESTING? → SCOUTING
                                         ↓
                                    FLEEING (if bandits near)
```

**SCOUTING:**
- Wander with bias toward PROFIT and DEMAND reputation signals.
- Weight signals by `greed` trait (high greed → stronger PROFIT bias).
- Evaluate price differentials from price memory.
- If profitable route found (margin > 15% after estimated tax + transport
  cost), transition to BUYING at the cheap city.
- If no known opportunity, head toward least-recently-visited city.
- Deposit `DEMAND` signal.

**BUYING:**
- At a city: scan order book for commodities where
  `best_known_sell_price - local_buy_price > estimated_transport_cost × (1 + tax)`.
- Rank commodities by expected margin. Buy highest-margin goods first.
- Personality: high `greed` merchants go all-in on one commodity; low `greed`
  diversify across 2–3 commodities.
- Fill inventory up to carry capacity. Transition to TRANSPORTING.

**TRANSPORTING:**
- Navigate toward target sell city. Use A* on terrain grid for pathfinding
  (recomputed every 200 ticks or when obstacle encountered).
- Deposit `PROFIT` signal along route (strength ∝ expected margin).
- Bias away from `DANGER` signals, modulated by `risk_tolerance`:
  `danger_avoidance = (1.0 - risk_tolerance) × danger_gradient`.
- If fatigue > 70: detour to nearest city for rest.
- If bandit detected within 50px and not in caravan of 3+: transition to FLEEING.

**SELLING:**
- At target city: sell goods at market.
- If actual margin > 50%: deposit strong `PROFIT` signal.
- If loss: deposit `DANGER` signal, mark route as unprofitable for 1000 ticks.
- Update price memory for this city.
- If fatigue > 50: transition to RESTING, else back to SCOUTING.

**FLEEING:**
- Turn away from bandit. Max speed. Head toward nearest city.
- Deposit `DANGER` signal at high strength.
- Attempt to join any nearby caravan.
- If no bandit within 80px for 100 ticks: resume previous state.

### Miner FSM
```
TRAVELING_TO_NODE → EXTRACTING → TRAVELING_TO_CITY → SELLING → RESTING? → TRAVELING_TO_NODE
```
- Selects resource node by: proximity, yield rate, and competition (fewer
  nearby miners = better). Preference for ORE and CLAY.
- Extracts until inventory full (extraction takes 3 ticks per unit).
- Travels to nearest city. Sells. Deposits `OPPORTUNITY` signal at
  high-yield nodes.
- Seasonal awareness: avoids CLAY nodes in winter if in northern map.

### Farmer FSM
Same structure as Miner but targets GRAIN, HERBS, FISH.
- Strong seasonal awareness: prioritizes GRAIN/HERBS in summer (peak yield),
  switches to FISH in winter.
- Deposits `OPPORTUNITY` signal at abundant farming sites.

### Craftsman FSM
```
BUYING_MATERIALS → CRAFTING → SELLING_GOODS → RESTING? → BUYING_MATERIALS
```
- Evaluates all recipes: `(output_sell_price - input_buy_cost) / craft_ticks`.
- Buys raw materials for highest-margin recipe at current city.
- Crafts at city (must remain stationary for recipe duration).
- Sells refined goods. May travel to another city if local sell price is low.
- Exploits city specialization bonus (1.5× craft speed) when available.

### Soldier FSM
```
PATROLLING → ESCORTING → FIGHTING → PATROLLING
```
- Patrols trade routes (follows high road-value cells between cities).
- If caravan nearby: attach as escort, receive protection fees.
- If bandit detected within 30px: engage. 50% win chance.
  Win: bandit destroyed, soldier gets +10 reputation.
  Lose: soldier loses 30% gold + all inventory, +50 fatigue.
- Deposits `DANGER` signals where bandits are spotted.
- High `risk_tolerance` soldiers venture further from cities.

### Shipwright FSM
```
LOADING → SAILING → UNLOADING → LOADING
```
- Only moves along COAST cells (water-adjacent land).
- 2.0× base speed on coast. 3.0× carry capacity.
- Loads goods at coastal city with harbor. Sails to another harbor city.
- Arbitrages coastal price differentials.
- Cannot operate in winter (harbors frozen).
- Deposits `OPPORTUNITY` signal along profitable sea routes.

### Profession Assignment
- Default: 40% trader, 12% miner, 10% farmer, 18% craftsman, 8% soldier, 5% shipwright, 7% idle.
- Rebalancing every 500 ticks: compute average income per profession over
  last 1000 ticks. Transfer 5% of worst-performing profession to
  best-performing. Idle merchants assigned to most-needed profession.
- Emergency rebalancing: if food shortage (< 20% of cities have food
  stocked), force 20% of idle + low-performing traders to become farmers.

---

## 3) Crafting System

### Tier 1 (Raw → Basic Refined)
```
TIMBER + ORE    → TOOLS       (2:1 → 1, 5 ticks)
GRAIN  + HERBS  → MEDICINE    (2:1 → 1, 5 ticks)
CLAY   + TIMBER → BRICKS      (2:1 → 1, 5 ticks)
ORE    + CLAY   → METALWORK   (2:1 → 1, 8 ticks)
GRAIN  + FISH   → PROVISIONS  (1:1 → 1, 3 ticks)
HERBS  + CLAY   → POTTERY     (1:1 → 1, 4 ticks)
```

### Tier 2 (Basic Refined → Advanced)
```
TOOLS     + ORE       → WEAPONS     (1:2 → 1, 10 ticks)
TOOLS     + TIMBER    → FURNITURE   (1:2 → 1, 8 ticks)
METALWORK + CLAY      → ARMOR       (1:1 → 1, 12 ticks)
MEDICINE  + POTTERY   → ALCHEMY     (1:1 → 1, 10 ticks)
BRICKS    + METALWORK → MACHINERY   (1:1 → 1, 15 ticks)
PROVISIONS+ HERBS     → FEAST_GOODS (2:1 → 1, 6 ticks)
```

### Tier 3 (Advanced — requires city WORKSHOP upgrade)
```
WEAPONS  + ARMOR     → ELITE_GEAR    (1:1 → 1, 20 ticks)
MACHINERY+ TOOLS     → AUTOMATON     (1:1 → 1, 25 ticks)
ALCHEMY  + MEDICINE  → ELIXIR        (1:1 → 1, 18 ticks)
FURNITURE+ FEAST_GOODS → LUXURY_SET  (1:1 → 1, 15 ticks)
```

Price multipliers (approximate, driven by supply/demand):
- Tier 1: 2.5–4× raw material cost
- Tier 2: 3–6× input cost
- Tier 3: 4–10× input cost

---

## 4) Market Engine

### Order Book (per city, per commodity)
```rust
struct Order {
    agent_id: u32,
    commodity: Commodity,
    side: Side,         // Buy | Sell
    price: f32,
    quantity: f32,
    tick_placed: u32,
    ttl: u32,           // ticks until expiry
}
```

### Matching Rules
- Orders match when `buy_price ≥ sell_price`.
- Execution price: midpoint `(buy + sell) / 2`.
- Partial fills: remaining quantity stays on book.
- City tax: `tax_rate × transaction_value` deducted from buyer's gold.
- Unmatched orders expire after TTL (default 200 ticks).
- Price-time priority: best price first, then earliest order.
- No self-matching.

### NPC Demand
When no player orders exist for a commodity:
- City generates passive buy orders:
  `demand_qty = population × 0.01 × commodity_necessity_weight`.
- Necessity weights: PROVISIONS 3.0, GRAIN 2.0, TOOLS 1.5, MEDICINE 1.5,
  BRICKS 1.0, others 0.5.
- NPC buy price: `last_known_price × (1.0 + 0.05 × scarcity_factor)`.
- NPC gold comes from city treasury (accumulated tax). Cities go into
  austerity (no NPC buying) when treasury < 50g.

### Price History
Each city maintains per-commodity price series (last 2000 ticks):
- Used for agent price memory updates.
- Used for statistical emergence tests (price convergence, boom-bust).
- Accessible for UI price charts.

### Dynamic Tax
Every 500 ticks, each city adjusts tax rate:
- Trade volume above city average → increase tax by 1% (max 15%).
- Trade volume below average → decrease tax by 1% (min 0%).
- Creates feedback loops: high-tax cities may lose merchants.

---

## 5) Emergent Behaviors to Verify

| Behavior | Description | Metric |
|---|---|---|
| **Trade route formation** | Merchants converge on efficient paths between complementary cities | Road grid entropy decreases over time; corridor width narrows |
| **Market specialization** | Cities near specific resources develop matching crafting clusters | Herfindahl index of per-city craft output increases over time |
| **Price convergence** | Same goods trend toward similar prices at connected cities | Cross-city price variance per commodity decreases |
| **Supply chain emergence** | Multi-hop chains: miner→city₁→craftsman→city₂→trader→city₃ | Avg commodity "touch count" (distinct agents handling it) > 2.0 |
| **Boom-bust cycles** | Oversupply → price crash → producers leave → shortage → spike | Detrended price autocorrelation shows periodicity (Ljung-Box, p<0.05) |
| **Seasonal price waves** | GRAIN/HERBS expensive in winter, cheap in summer | Seasonal decomposition shows significant seasonal component |
| **Economic migration** | Merchants relocate toward high-profit regions | Population Gini across cities increases then stabilizes |
| **Guild clustering** | Same-profession agents cluster near relevant resources | DBSCAN spatial clustering per profession produces ≤ 4 clusters |
| **Caravan formation** | Merchants spontaneously group on dangerous routes | Caravan frequency higher on high-DANGER routes (correlation > 0.3) |
| **Information propagation** | Price gossip ripples outward from discovery | Price-knowledge wavefront speed measurable and > 0 |
| **Wealth inequality** | Power-law distribution emerges from equal starts | Gini coefficient of agent gold > 0.35 after 5000 ticks |
| **Profession adaptation** | Resource scarcity → profession redistribution | Profession distribution correlates with resource availability |
| **City growth/decline** | Well-traded cities grow; neglected ones shrink | Population variance across cities increases over time |
| **Bandit avoidance** | Trade routes shift away from bandit camps | DANGER-weighted route overlap decreases after bandit camp appears |
| **Tax competition** | High-tax cities lose volume to low-tax neighbors | Negative correlation between tax rate and trade volume |

---

## 6) Interactive Controls (macroquad UI)

- **Pause/Resume**: Space
- **Speed**: `+`/`-` → 0.5×, 1×, 2×, 5×, 10×
- **Reputation overlay**: `1`–`4` per channel, `0` off
- **Road overlay**: `O` to highlight emergent road network
- **Place mountain**: right-click drag to paint impassable terrain
- **Place resource**: middle-click to drop a random resource node
- **Remove obstacle**: Shift+right-click to clear painted terrain
- **Market crash**: `C` → all commodity prices to 10% (test recovery)
- **Famine**: `F` → remove all GRAIN/FISH/HERBS nodes
- **Gold injection**: `G` → every merchant +500g (test inflation)
- **Bandit surge**: `B` → spawn 5 extra bandit camps
- **Force winter**: `W` → immediately switch to winter season
- **Kill merchants**: `K` → remove 20% of merchants randomly
- **Economy HUD**: `S` to toggle overlay showing:
  - Population (alive / bankrupt)
  - Total gold in circulation
  - Trade volume (last 200 ticks)
  - Per-commodity average price bars
  - Profession distribution bars
  - Season indicator
  - Avg merchant wealth + Gini coefficient
  - Reputation signal mass per channel
  - Top 3 trade routes by volume
  - Active caravan count
  - Bandit camp count
- **Merchant inspector**: left-click merchant → side panel with full state,
  inventory, price memory, ledger, current FSM state, traits, caravan info
- **City inspector**: left-click city → population, treasury, tax rate,
  warehouse contents, upgrade status, prosperity, order book summary
- **Heatmap mode**: `H` → merchant density heatmap
- **Trade flow arrows**: `A` → animated arrows between cities ∝ trade volume
- **Price chart**: `P` → live multi-line commodity price chart (last 1000 ticks)
- **Wealth histogram**: `E` → live agent wealth distribution
- **Season timeline**: always visible at top — shows current season and
  progress through the year cycle

---

## 7) Reputation Engine

Grid resolution: `world_size / 8px` = 200×125 cells per channel.
4 channels → 4 grids of 200×125 f32.

Per tick:
1. **Deposit**: merchants write signals (additive, clamped 1.0).
2. **Diffusion**: separable Gaussian blur per channel.
3. **Decay**: multiply by channel-specific decay factor.
4. **Sampling**: bilinear interpolation at scanner positions.

Road grid (separate, same resolution):
- Traversal: `+0.002` per agent crossing.
- Decay: `×0.9998` per tick.
- Speed bonus: `1.0 + 0.6 × road_value`.

**Target**: reputation + road processing < 3ms per tick. Leverage Rust's
SIMD-friendly iteration and avoid heap allocation in hot loop.

---

## 8) Configuration

All parameters loaded from `economy_config.toml`:

```toml
[world]
width = 1600
height = 1000
terrain_seed = 42
terrain_octaves = 4
sea_level = 0.25
num_cities = 10
num_resource_nodes = 25
season_length_ticks = 2500

[city]
radius = 30
population_range = [50, 500]
tax_rate_range = [0.0, 0.15]
warehouse_capacity = 500
warehouse_decay_rate = 0.001
npc_demand_base = 0.01
order_ttl = 200
upgrade_costs = { market_hall = 500, walls = 800, harbor = 1000, workshop = 600 }

[merchant]
initial_population = 200
max_population = 400
spawn_rate = 0.05
initial_gold = 100.0
base_speed = 1.5
max_carry = 10.0
shipwright_carry_mult = 3.0
shipwright_speed_mult = 2.0
fatigue_max = 100.0
fatigue_cost_base = 0.03
fatigue_cost_speed = 0.02
fatigue_cost_carry = 0.04
fatigue_recovery_rate = 1.5
scanner_angle_deg = 35
scanner_range = 60.0
neighbor_radius = 30.0
terrain_ray_count = 5
terrain_ray_range = 40.0
terrain_ray_arc_deg = 120
gossip_range = 25.0
price_memory_ttl = 1000
caravan_join_range = 30.0
caravan_min_size = 3
caravan_safe_size = 4
bankruptcy_grace_ticks = 200

[bandit]
num_camps = 8
patrol_radius_range = [100, 200]
agents_per_camp = [2, 4]
rob_gold_pct = [0.1, 0.3]
rob_goods_pct = [0.2, 0.4]
attack_range = 15.0
starvation_ticks = 3000
respawn_interval = 5000
seasonal_activity = { spring = 1.0, summer = 1.3, autumn = 1.0, winter = 0.5 }

[reputation]
cell_size = 8

[reputation.channels.profit]
decay = 0.993
diffusion_sigma = 0.6
color = [0, 255, 100]

[reputation.channels.demand]
decay = 0.990
diffusion_sigma = 0.8
color = [0, 150, 255]

[reputation.channels.danger]
decay = 0.985
diffusion_sigma = 0.5
color = [255, 50, 50]

[reputation.channels.opportunity]
decay = 0.975
diffusion_sigma = 1.2
color = [255, 220, 0]

[road]
cell_size = 8
increment = 0.002
decay = 0.9998
max_speed_bonus = 0.6

[professions]
default_distribution = { trader = 0.40, miner = 0.12, farmer = 0.10, craftsman = 0.18, soldier = 0.08, shipwright = 0.05, idle = 0.07 }
rebalance_interval = 500
```

---

## 9) Project Structure

```
swarm-economy/
├── Cargo.toml
├── economy_config.toml
├── README.md
├── src/
│   ├── main.rs                      # entry point, macroquad game loop
│   ├── config.rs                    # TOML config loader + validation
│   ├── lib.rs                       # crate root, module declarations
│   ├── types.rs                     # shared enums, Vec2, Commodity, Season, etc.
│   ├── world/
│   │   ├── mod.rs
│   │   ├── world.rs                 # World struct: terrain, cities, resources, bounds
│   │   ├── terrain.rs               # Perlin noise heightmap, terrain classification
│   │   ├── reputation.rs            # ReputationGrid: deposit, diffuse, decay, sample
│   │   ├── road.rs                  # RoadGrid: wear-in, decay, speed bonus
│   │   ├── city.rs                  # City struct, warehouse, population, tax, upgrades
│   │   ├── resource_node.rs         # ResourceNode: extraction, depletion, regen, seasons
│   │   └── bandit.rs                # BanditCamp, bandit agents, patrol, robbery
│   ├── agents/
│   │   ├── mod.rs
│   │   ├── merchant.rs              # Merchant struct + physics/movement
│   │   ├── sensory.rs               # SensoryInput builder (scanner, raycasts, neighbors)
│   │   ├── actions.rs               # MerchantAction struct + action execution
│   │   ├── traits.rs                # MerchantTraits: personality generation + effects
│   │   ├── caravan.rs               # Caravan formation, movement, dissolution
│   │   └── economy_manager.rs       # Spawning, professions, stats, rebalancing
│   ├── brain/
│   │   ├── mod.rs
│   │   ├── interface.rs             # Brain trait definition
│   │   ├── trader.rs                # Trader FSM
│   │   ├── miner.rs                 # Miner FSM
│   │   ├── farmer.rs                # Farmer FSM
│   │   ├── craftsman.rs             # Craftsman FSM
│   │   ├── soldier.rs               # Soldier FSM
│   │   ├── shipwright.rs            # Shipwright FSM
│   │   └── idle.rs                  # Idle behavior
│   ├── market/
│   │   ├── mod.rs
│   │   ├── order_book.rs            # OrderBook: place, match, cancel, price history
│   │   ├── crafting.rs              # CraftingEngine: recipe registry, validation, execution
│   │   └── gossip.rs                # Price gossip: info sharing logic
│   ├── rendering/
│   │   ├── mod.rs
│   │   ├── renderer.rs              # Main macroquad renderer
│   │   ├── hud.rs                   # Stats overlay, inspector panels
│   │   ├── reputation_overlay.rs    # Reputation heatmap rendering
│   │   ├── trade_flow.rs            # Animated trade arrows
│   │   ├── price_chart.rs           # Live commodity price line chart
│   │   └── controls.rs              # Input handling
│   └── metrics/
│       ├── mod.rs
│       ├── tracker.rs               # MetricsTracker: time series recording
│       ├── emergence.rs             # Emergent behavior detectors
│       ├── inequality.rs            # Gini, Lorenz, wealth distribution
│       └── reporter.rs              # JSON summary report
├── tests/
│   ├── common/
│   │   └── mod.rs                   # Shared test fixtures (mini worlds, single merchants)
│   ├── test_merchant_physics.rs     # Movement, collision, fatigue, bankruptcy
│   ├── test_reputation.rs           # Deposit, diffusion, decay, sampling
│   ├── test_road.rs                 # Wear-in, decay, speed bonus
│   ├── test_terrain.rs              # Heightmap, classification, pathability, seasons
│   ├── test_sensory.rs              # Scanner cones, raycasts, neighbors
│   ├── test_order_book.rs           # Matching, partial fills, tax, expiry, priority
│   ├── test_crafting.rs             # Recipes, tiers, duration, city requirements
│   ├── test_gossip.rs               # Range, staleness, propagation, caravan sharing
│   ├── test_bandit.rs               # Patrol, robbery, camp lifecycle, soldier combat
│   ├── test_city.rs                 # Growth, decline, tax adjustment, upgrades, treasury
│   ├── test_caravan.rs              # Formation, movement, dissolution, safety threshold
│   ├── test_seasonal.rs             # Resource yield mods, travel speed, harbor freeze
│   ├── test_brain_trader.rs         # All state transitions, margin calc, fleeing
│   ├── test_brain_miner.rs         # Node selection, extraction, seasonal switching
│   ├── test_brain_farmer.rs         # Crop rotation, seasonal awareness
│   ├── test_brain_craftsman.rs      # Recipe selection, specialization bonus usage
│   ├── test_brain_soldier.rs        # Patrol, escort, combat outcomes
│   ├── test_brain_shipwright.rs     # Coast pathfinding, harbor requirement, winter lockout
│   ├── test_economy_manager.rs      # Spawning, rebalancing, emergency profession shift
│   ├── test_world.rs                # Resource depletion/regen, city placement, bounds
│   ├── test_emergence.rs            # Statistical tests for all 15 emergent behaviors
│   └── test_replay.rs              # Deterministic replay
└── benches/
    ├── bench_reputation.rs          # Reputation engine throughput
    ├── bench_market.rs              # Order matching throughput
    ├── bench_colony_tick.rs         # Full tick for 200 merchants
    └── bench_pathfinding.rs         # A* on terrain grid
```

---

## 10) Test Suite

### Unit Tests

**test_merchant_physics.rs**
- Movement: `speed × speed_mult × terrain_mult × road_mult × fatigue_mult × season_mult`
- Heading wraps at ±π
- Mountain/water collision: slide, never penetrate
- World-bound reflection
- Fatigue: correct drain rate for each component
- Fatigue collapse: 15% inventory dropped, fatigue → 80
- Fatigue recovery at city at configured rate
- Inventory weight sums all commodities correctly
- Gold debit/credit on buy/sell
- Bankruptcy after 200 ticks with gold < 0
- Bankrupt merchant drops all inventory + deposits DANGER signal
- Caravan speed: slowest member governs group

**test_reputation.rs**
- Single deposit creates correct cell value
- Additive stacking, clamped at 1.0
- Diffusion conserves total mass ±1%
- Decay: value ≈ initial × decay^N (within 0.1%)
- Bilinear interpolation correct at cell boundaries
- Channels are independent
- Performance: 1000 ticks on default grid < 2s (Rust should crush this)

**test_road.rs**
- Traversal increment correct
- Clamped at 1.0
- Decay rate correct over time
- Speed bonus formula: `1.0 + max_bonus × road_value`
- No cross-cell contamination
- High-traffic corridor builds faster than surrounding cells

**test_terrain.rs**
- Perlin noise output in [0, 1]
- Same seed → identical heightmap
- Terrain type matches height thresholds
- Mountains and water impassable
- Speed multiplier correct per type
- Coast cells correctly identified (water adjacent to land)
- Season modifiers apply to correct terrain regions

**test_sensory.rs**
- Scanner cone geometry: correct cells sampled
- Left/right discrimination when signal source is off-center
- Terrain raycasts: correct distance to known obstacles
- Neighbor detection within radius
- City direction vector accuracy
- Price memory: updated on city visit, stale after TTL
- Bandit proximity detection at correct range

**test_order_book.rs**
- Overlapping buy/sell → execute at midpoint
- Partial fill: remainder stays on book
- Tax deducted from buyer correctly
- TTL expiry removes stale orders
- No self-matching
- NPC demand generation: quantity and price formulas correct
- Price-time priority: better price first, then older order
- Price history records correct execution prices
- Empty book: no matches
- City treasury funds NPC buying; austerity when treasury low

**test_crafting.rs**
- Each tier: correct input consumption and output generation
- Insufficient materials → rejection
- Crafting duration enforced (not instant)
- Must be at a city
- City specialization bonus: 1.5× speed → fewer ticks
- Tier 3 requires city WORKSHOP upgrade (reject without it)
- Concurrent crafting by multiple agents at same city

**test_gossip.rs**
- Within range: one price entry exchanged per tick per pair
- Out of range: no exchange
- Stale entries (past TTL) treated as unknown
- Caravan members: full memory merge
- Bandit location info shared correctly
- Sociability trait modulates gossip probability

**test_bandit.rs**
- Bandits patrol within configured radius of camp
- Robbery: correct gold and goods percentage stolen
- Robbery range: only within attack_range
- Caravans of 4+: bandits don't attack
- Caravans of 3: 70% repel chance
- Soldier combat: 50/50 outcome, correct consequences for each
- Camp starvation: removed after configured ticks without robbery
- Camp respawn at configured interval
- Seasonal activity modifier applies correctly
- Bandits avoid walled cities

**test_city.rs**
- Population grows when prosperity > 60 and food stocked
- Population declines when no food for 200+ ticks
- Tax adjustment: high volume → raise, low volume → lower
- Tax clamped to [0, 0.15]
- Upgrade purchase deducts from treasury
- Upgrade effects active (walls block bandits, harbor enables ships, etc.)
- Warehouse overflow decay at 0.1%/tick
- Prosperity score computed from correct components

**test_caravan.rs**
- Formation: requires 2+ nearby agents heading similar direction
- Sociability threshold respected
- Caravan movement at slowest member speed
- Dissolution when spread > 100px
- Dissolution at destination city
- Safety: size 4+ immune to bandits
- Soldier auto-attaches, receives protection fee
- Price memory merge within caravan

**test_seasonal.rs**
- GRAIN/HERBS yield ×2 summer, ×0.3 winter
- FISH yield ×1.5 spring, ×0.5 autumn
- CLAY yield ×0 in winter for northern nodes
- TIMBER/ORE: no seasonal modifier
- Winter global speed multiplier ×0.7
- Winter food consumption ×1.5
- Harbors frozen in winter (ships cannot operate)
- Season cycles at correct interval

**test_brain_trader.rs**
- SCOUTING: biases toward PROFIT/DEMAND signals
- SCOUTING → BUYING: when profitable route identified
- BUYING: selects highest-margin commodity, greed trait affects diversification
- TRANSPORTING: heads toward target city, avoids DANGER (modulated by risk_tolerance)
- SELLING: deposits correct signals based on profit/loss
- Fatigued → RESTING transition at threshold
- FLEEING: triggered by bandit proximity, attempts caravan join

**test_brain_miner.rs**
- Selects node by proximity × yield × inverse competition
- Extracts at configured rate
- Heads to nearest city when full
- Avoids depleted nodes
- Seasonal: avoids frozen CLAY nodes in winter

**test_brain_farmer.rs**
- Summer: prioritizes GRAIN/HERBS (peak yield)
- Winter: switches to FISH
- Returns to city when inventory full
- Deposits OPPORTUNITY at abundant sites

**test_brain_craftsman.rs**
- Selects recipe by `(output_price - input_cost) / craft_ticks`
- Uses city specialization bonus when available
- Travels to another city if local sell price low
- Tier 3 crafting only at cities with WORKSHOP

**test_brain_soldier.rs**
- Patrols along high road-value cells
- Attaches to nearby caravans
- Engages bandits within range
- Correct combat outcomes (win/lose consequences)
- Risk tolerance affects patrol distance from cities

**test_brain_shipwright.rs**
- Only moves on COAST cells
- 2.0× speed on coast
- 3.0× carry capacity
- Requires harbor at both endpoints
- Cannot operate in winter
- Arbitrages coastal price differentials

**test_economy_manager.rs**
- Initial population matches config
- Spawn rate correct when economy healthy
- Profession distribution within ±5% of config
- Bankrupt merchants properly removed
- Rebalancing shifts professions toward higher income
- Emergency rebalancing: food shortage → farmers increase
- Gold conservation check (total system gold constant ignoring NPC market)

**test_world.rs**
- Resource depletion formula correct
- Exhausted nodes produce nothing
- Regeneration rate correct when not harvested
- City Poisson disk spacing maintained
- Terrain deterministic for same seed
- World bounds checking all edges
- Multiple nodes of same type coexist
- Resource type distribution not overly clustered

### Integration Tests

**test_emergence.rs** (headless, accelerated)

- **Trade route formation**: 2 cities with complementary resources, 200
  merchants, 3000 ticks. Assert: road corridor connecting them, width < 120px.
- **Market specialization**: 10 cities, 5000 ticks. Assert: ≥ 3 cities
  with Herfindahl index > 0.4 for craft output.
- **Price convergence**: 5000 ticks. Assert: cross-city price variance
  decreases ≥ 30% for ≥ 4 commodities.
- **Boom-bust**: 10000 ticks. Assert: ≥ 1 commodity with significant
  autocorrelation (Ljung-Box, p < 0.05) at lag 500–3000.
- **Seasonal pricing**: 10000 ticks (4 full year cycles). Assert: GRAIN
  price seasonal component amplitude > 15% of mean price.
- **Wealth inequality**: equal start, 5000 ticks. Assert: Gini > 0.3.
- **Guild clustering**: 5000 ticks. Assert: ≥ 2 professions with DBSCAN
  clustering producing ≤ 4 spatial clusters.
- **Caravan frequency**: 5000 ticks. Assert: caravan formation rate
  positively correlated with route DANGER level (Pearson r > 0.3).
- **Information propagation**: new high-value node at tick 1000. Assert:
  > 50% of merchants learn nearest-city prices within 2000 ticks.
- **Economic migration**: new resource at tick 2000. Assert: merchant
  density near it increases ≥ 20% within 1500 ticks.
- **Profession adaptation**: remove all ORE at tick 2000. Assert: miner %
  drops ≥ 50% within 1500 ticks.
- **City growth**: 8000 ticks. Assert: population variance across cities
  increases and ≥ 1 city population > 1.5× initial.
- **Bandit avoidance**: place bandit camp on active trade route at tick
  2000. Assert: road traffic through camp radius drops ≥ 40% within 1000 ticks.
- **Tax competition**: 5000 ticks. Assert: negative Pearson correlation
  (r < -0.2) between city tax rate and trade volume.

**test_replay.rs**
- Save state at tick N, run to N+500, reload N with same seed, re-run
  to N+500. Assert: merchant positions match within 0.01px.

### Benchmarks (`cargo bench`)

- Reputation + road engine: μs per tick (target < 2ms)
- Order book matching: μs per tick for 10 cities × 12 commodities (target < 1ms)
- Full colony tick for 200 merchants (target < 8ms)
- A* pathfinding: μs per query on full terrain grid (target < 500μs)
- Full frame (update + render): ms at 200 merchants (target < 12ms)

---

## 11) Experiment Mode (Headless)

```bash
cargo run --release -- --headless --ticks 10000 --merchants 300 --seed 42
cargo run --release -- --headless --ticks 10000 --merchants 200 --seed 42 --no-bandits
cargo run --release -- --headless --ticks 10000 --merchants 200 --seed 42 --eternal-summer
```

Outputs JSON:

```json
{
  "config": { "ticks": 10000, "merchants": 300, "seed": 42 },
  "total_trade_volume": 34821,
  "total_gold_circulation": 48300,
  "gini_coefficient_final": 0.44,
  "population_final": 278,
  "bankruptcies": 22,
  "robberies": 87,
  "caravans_formed": 34,
  "price_convergence_ratio": 0.58,
  "num_active_trade_routes": 8,
  "route_entropy_over_time": [3.9, 3.2, 2.5],
  "specialization_herfindahl_avg": 0.41,
  "avg_trade_profit_margin": 0.21,
  "profession_distribution_final": {
    "trader": 0.42, "miner": 0.11, "farmer": 0.09,
    "craftsman": 0.19, "soldier": 0.09, "shipwright": 0.04, "idle": 0.06
  },
  "commodity_prices_final": {
    "timber": 11.2, "ore": 19.4, "grain": 9.8, "fish": 7.1,
    "clay": 10.3, "herbs": 15.1, "tools": 48.7, "medicine": 36.2,
    "weapons": 92.4, "machinery": 145.8, "elite_gear": 310.2
  },
  "city_populations_final": [412, 287, 156, 503, 234, 89, 341, 267, 178, 445],
  "city_upgrades": { "city_0": ["walls", "market_hall"], "city_3": ["harbor", "workshop"] },
  "seasonal_price_amplitude": { "grain": 0.22, "herbs": 0.19, "fish": 0.14 },
  "boom_bust_detected": true,
  "performance": {
    "avg_tick_us": 4200,
    "p99_tick_us": 8100
  }
}
```

---

## 12) Deliverables

Source code for:
- World engine (Perlin terrain, reputation grid, road grid, seasons)
- City system (markets, population, tax, warehouses, upgrades, prosperity)
- Resource nodes (extraction, depletion, regeneration, seasonal modifiers)
- Bandit system (camps, patrol, robbery, starvation lifecycle)
- Merchant agent (physics, sensory, inventory, gold, fatigue, traits)
- Caravan system (formation, cohesive movement, dissolution, safety)
- Market engine (order books, matching, NPC demand, price history, dynamic tax)
- Crafting system (3-tier recipe tree, duration, city specialization)
- Gossip system (price propagation, bandit intel sharing)
- 7 profession FSMs (trader, miner, farmer, craftsman, soldier, shipwright, idle)
- macroquad renderer (terrain, agents, cities, reputation, roads, HUD, inspectors)
- Metrics tracker + 15 emergent behavior detectors
- Headless experiment runner
- Full test suite (unit + integration + emergence + replay + benchmarks)
- TOML config with documented defaults

README.md including:
- Architecture overview (ECS-style agent loop, market engine, reputation grid)
- How to run (GUI mode, headless mode, experiments)
- Controls reference
- Profession behaviors explained (all 7 FSMs)
- Crafting tree diagram
- Emergent behaviors guide (what to look for, what the metrics mean)
- Market engine mechanics (order books, tax dynamics, NPC demand)
- Seasons and their effects
- Test suite description + `cargo test` / `cargo bench` instructions
- Performance tuning notes
- Config reference (all TOML fields documented)
- Known limitations / future ideas