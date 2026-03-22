# AGENTS.md — Geo Events Engine

Instructions for humans and **automated coding agents** working in this repository. Read the **Agent quick reference** first; use the rest for depth.

---

## Agent quick reference

**What this repo is:** A Rust workspace for a geo-native, stateful, **streaming-oriented** event engine: location updates in → structured spatial events out (enter/exit, proximity, etc.). It is **not** a GIS library, PostGIS wrapper, viz stack, or batch-first ETL system.

**Invariant rules (obey these on every change):**

| Rule | Detail |
|------|--------|
| Engine owns logic | Core processing lives in `crates/engine`. Deterministic, no IO, no protocol types leaking in. |
| Adapters are thin | `crates/cli`, `crates/adapters/*`: parse/serialize, call engine, return results. No spatial rules, no business logic, no owning application state. |
| Event-first API | **Target:** `process_event`-style single-update handling; batch is only a loop. **Today:** primary entrypoint is `GeoEngine::ingest(batch)`. |
| Spatial is pluggable | **Target:** traits/composition (e.g. `SpatialRule`), not closed `match` trees in core. **Today:** orchestration is explicit in `Engine::ingest` calling `state` transition helpers; still avoid growing ad-hoc `match`es—extract when adding rules. |
| State is explicit | Transitions are `(old_state, event) → (new_state, outputs)`; no hidden cross-module mutation. |
| Errors | Prefer `Result`; avoid panics in engine/state/spatial core paths. |

**If you are unsure where code belongs:**

- New rule or orchestration → `crates/engine` (and traits/types as appropriate in `crates/state` / `crates/spatial`).
- New geometry or index behaviour → `crates/spatial` (no domain “business” rules).
- New wire format or HTTP/stdio handling → `crates/adapters/*` or `crates/cli`, plus `protocol/` docs if the contract changes.
- Shared state shape or transitions → `crates/state`.

**Workspace layout (actual paths):**

```text
Cargo.toml                 # workspace members
crates/engine/             # event processing, rules orchestration
crates/state/              # entity state, transitions
crates/spatial/            # geometry, SpatialIndex, no business rules
crates/polygon-json/       # supporting crate (JSON polygons)
crates/adapters/http/      # HTTP adapter
crates/adapters/stdin-stdout/
crates/cli/                # CLI entrypoint
protocol/                  # NDJSON contract: ndjson-v1.md, ndjson-v1.1.md, ROADMAP.md
```

**Testing expectations:** Engine and `state` unit tests; CLI integration tests with NDJSON fixtures under `crates/cli/tests/fixtures/` and examples under `examples/`.

**Default when tradeoffs are unclear:** Prefer simplicity and a composable abstraction over a large one-off implementation. **Build the engine abstraction first, not the ecosystem.**

---

## Project overview

**geo-events** transforms raw location updates into higher-level spatial events: enter/exit, proximity alerts, transitions, and (future) spatial joins and temporal patterns.

### What this project is not

- Not a GIS toolkit
- Not a PostGIS wrapper
- Not a visualization tool
- Not a batch processing pipeline (batch may exist only as a thin wrapper)

### What this project is

- An in-memory streaming engine
- A stateful event processor
- Developer-first ergonomics (Prisma-like DX as a product goal)
- A language-agnostic core over time (Rust core today)

### Core vision

Build a **Prisma-like developer experience for real-time geospatial computation**: simple APIs, powerful Rust core, multiple surfaces (CLI, HTTP, future SDKs).

---

## Architecture principles (mandatory)

### 1. Engine-first design

- Core logic **must** live in `crates/engine`.
- The engine **must** be: deterministic, side-effect free except intentional state updates, and independent of IO.
- Adapters **must** be thin wrappers.

### 2. Event-driven (not batch-driven)

The system **must** center on single-event processing:

```rust
fn process_event(&mut self, event: Event) -> Vec<OutputEvent>
```

Batch ingestion is **only** a wrapper:

```rust
for event in events {
    process_event(event);
}
```

Do **not** design core logic around batches.

**Today:** the shipped API is `ingest(&mut self, batch: Vec<PointUpdate>)`; treat that as the batch wrapper to migrate, not the long-term shape.

### 3. Explicit state model

All computation **must** follow:

```text
(old_state, event) -> (new_state, output_events)
```

State **must** be: explicit, isolated, deterministic, and not implicitly mutated across modules.

### 4. Pluggable spatial logic (critical)

Do **not** hardcode spatial behaviour like:

```rust
match rule_type {
    Geofence => { /* ... */ }
}
```

Prefer extensible hooks, for example:

```rust
trait SpatialRule {
    fn evaluate(&self, state: &EntityState, event: &Event) -> Option<OutputEvent>;
}
```

Spatial behaviour **must** be composable, extensible, and decoupled from the engine core.

### 5. Spatial indexing is first-class

The spatial crate **must**:

- Expose a `SpatialIndex` trait (or equivalent abstraction).
- Support efficient point → region lookup.
- Avoid full scans where an index applies.

**As implemented:** `NaiveSpatialIndex` uses an **R-tree** (`rstar`) on polygon bounding boxes with exact `contains` refinement for geofences, corridors, and catalog regions. **Radius zones** are a linear scan over registered disks. Zones are registered incrementally (per insert), not rebuilt each ingest.

### 6. Protocol is a contract

The `protocol/` directory defines the external interface and versioned behaviour. Requirements: backwards-compatibility awareness, clear versioning, eventually machine-readable schema (e.g. JSON Schema).

### 7. Adapters must be thin

Adapters (CLI, HTTP, future Kafka, Redis, etc.) **must only**: parse input, call the engine, return output.

Adapters **must not**: implement spatial logic, own long-lived engine state beyond wiring, or encode business rules.

---

## Crate responsibilities

| Crate | Responsibility |
|-------|----------------|
| `crates/engine` | Owns event processing; exposes `GeoEngine` (zone registration + `ingest`). **Direction:** add or migrate to `process_event`-style single-update handling. |
| `crates/state` | Defines `EntityState` and state transitions; deterministic and testable. |
| `crates/spatial` | Spatial primitives, geometry, spatial indexing (`SpatialIndex`); **no** business logic. |
| `crates/adapters/*` | IO boundaries; protocol parsing and serialization only. |
| `crates/cli` | Developer entrypoint; wraps adapters; **no** core engine logic. |

---

## Core data model (target)

**Event**

```rust
struct Event {
    entity_id: String,
    location: Point,
    timestamp: u64,
}
```

**State**

```rust
struct EntityState {
    last_location: Point,
    active_regions: HashSet<RegionId>,
}
```

**Output**

```rust
struct OutputEvent {
    entity_id: String,
    event_type: EventType,
    region_id: Option<RegionId>,
}
```

---

## Implemented API and types (today)

These differ from the target model above; adapters and tests should match **what exists in code**.

**Engine surface** (`crates/engine`):

- Trait `GeoEngine`: `register_geofence`, `register_corridor`, `register_catalog_region`, `register_radius_zone`, and **`ingest(&mut self, batch: Vec<PointUpdate>) -> Vec<Event>`**.
- Input update: `PointUpdate { id, x, y }` (no timestamp in the core type).
- Concrete type: `Engine` backed by `spatial::NaiveSpatialIndex` and `HashMap<String, EntityState>`.

**Emitted events** (`crates/state`): `Event` is an enum — `Enter` / `Exit`, `EnterCorridor` / `ExitCorridor`, `Approach` / `Recede` (radius), `AssignmentChanged` (catalog). Ordering within an ingest is deterministic (`sort_events_deterministic`).

**Per-entity state** (`crates/state`): `EntityState` holds `position: Option<(f64, f64)>`, membership sets (`inside`, `inside_corridor`, `inside_radius` as `BTreeSet<String>`), and `catalog_region: Option<String>`.

**Supporting crate:** `crates/polygon-json` parses GeoJSON polygons for HTTP and stdin-stdout adapters (not used inside `crates/engine`).

---

## Execution model (target)

```text
Event → Engine → Rules → State transition → Output events
```

---

## Current state of the codebase

The project currently:

- **Batch-first API:** `ingest` takes `Vec<PointUpdate>`; CLI defaults to `batch_size` 1 (stream-like) but the engine API is still batch-shaped.
- **Zone kinds:** Geofences (enter/exit), corridors (corridor enter/exit), catalog regions (assignment / tie-break by smallest id), radius zones (approach/recede).
- **Spatial:** `SpatialIndex` trait exists; `NaiveSpatialIndex` implements R-tree–accelerated polygon queries plus linear radius checks.
- **No `SpatialRule` trait yet** — behaviour is composed via `state` transition functions and explicit steps in `Engine::ingest`.
- **Adapters:** `crates/adapters/stdin-stdout`, `crates/adapters/http`, and `crates/cli` (including `http_main` for HTTP mode).
- **Protocol:** NDJSON v1 and v1.1 docs under `protocol/`.

**Known gap:** Move toward a true **`process_event`-style** API and pluggable rule traits without breaking determinism or thin adapters.

---

## Immediate goals (V1 direction)

1. Event-first engine API: `process_event(&mut self, update: …) -> Vec<Event>` (or equivalent), with **`ingest` as a thin loop** over single updates if it remains.
2. Rule abstraction via traits (e.g. `SpatialRule`) rather than an ever-growing imperative sequence in `ingest`.
3. Explicit state transitions: no hidden mutations; predictable, testable behaviour (already largely true in `state`; preserve when refactoring).
4. Keep batch as a wrapper only; batch must not define architecture.

---

## Future direction (non-blocking)

- **Clients:** TypeScript (high priority), Python (optional).
- **Integrations:** Kafka, Redis Streams.
- **Advanced:** Spatial joins (entity ↔ entity), dwell time, trajectory analysis, temporal rules.
- **Optional:** WASM / edge execution.

---

## Testing strategy

- **Scenario tests:** Movement through regions; enter/exit correctness.
- **Determinism:** Same inputs → same outputs.
- **Protocol tests:** NDJSON fixtures in `crates/cli/tests/fixtures/`; sample streams in `examples/`; contract text in `protocol/`.

---

## Anti-patterns (do not do)

- Mixing engine logic with adapters
- Hardcoding spatial logic via large `match` trees in core
- Designing around batch ingestion as the primary model
- Implicit or shared mutable state
- Overengineering infrastructure before the core abstractions exist
- Leaking protocol concerns into the engine

---

## Mental model

Treat the system as a **compiler for spatial events**:

- **Input:** raw location updates  
- **Output:** structured spatial events  

---

## Definition of success

A developer can run locally via CLI, integrate via SDK (future), and process real-time streams **without** requiring Kafka, Flink, or PostGIS for the core story.

---

## Coding rules

**General**

- Prefer composition over inheritance.
- Prefer traits for extensibility where it avoids a closed set of enums for “open” behaviour.
- Avoid premature abstraction; keep modules small and focused.

**State and engine**

- No hidden side effects; explicit inputs and outputs; pure functions where possible.

**Error handling**

- Use `Result<T, E>`; avoid panics in core engine paths; meaningful error types.

**Naming**

- Clear, domain-driven names; avoid abbreviations; prefer `process_event` over vague `handle`.

**Performance**

- Avoid unnecessary allocations and cloning large structures; prefer references where sensible.

---

## Instructions for agents (editing this repo)

When making changes:

- Preserve separation of concerns and thin adapters.
- Prefer composable abstractions over branching-heavy cores.
- Keep engine behaviour deterministic and testable.
- Move the codebase toward event-driven processing when touching relevant areas.
- Avoid unnecessary complexity; if unsure, choose **simplicity + extensibility** over completeness.

**Guiding principle:** Build the engine abstraction first, not the ecosystem.
