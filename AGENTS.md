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
| Event-first API | Core path is **`GeoEngine::process_event`**. Multi-update ordering uses **`Engine::process_batch`** (sort ids → `process_event` each → global `sort_events_deterministic`). |
| Spatial is pluggable | Default pipeline is **`SpatialRule`** implementations composed in **`Engine`** (`default_rules()`). Add rules via **`Engine::with_rules`**. |
| State is explicit | Transitions are `(old_state, event) → (new_state, outputs)`; no hidden cross-module mutation. |
| Errors | Prefer `Result`; avoid panics in engine/state/spatial core paths. |

**If you are unsure where code belongs:**

- New rule or orchestration → `crates/engine` (and traits/types as appropriate in `crates/state` / `crates/spatial`).
- New geometry or index behaviour → `crates/spatial` (no domain “business” rules).
- New wire format or stdio handling → `crates/adapters/*` or `crates/cli`, plus `protocol/` docs if the contract changes.
- Shared state shape or transitions → `crates/state`.

**Workspace layout (actual paths):**

```text
Cargo.toml                 # workspace members
crates/engine/             # event processing, rules orchestration
crates/state/              # entity state, transitions
crates/spatial/            # geometry, SpatialIndex, no business rules
crates/polygon-json/       # supporting crate (JSON polygons)
crates/adapters/stdin-stdout/
crates/cli/                # CLI entrypoint
protocol/                  # NDJSON contract: ndjson.md, ROADMAP.md, schema/
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

**Today:** `process_event` on `GeoEngine`; batch helper `Engine::process_batch` only on the concrete engine.

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
    Zone => { /* ... */ }
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

**As implemented:** `NaiveSpatialIndex` uses an **R-tree** (`rstar`) on polygon bounding boxes with exact `contains` refinement for zones and catalog regions. **Circles** are also R-tree indexed. Zones are registered incrementally (per insert), not rebuilt each update.

### 6. Protocol is a contract

The `protocol/` directory defines the external interface and versioned behaviour. Requirements: backwards-compatibility awareness, clear versioning, eventually machine-readable schema (e.g. JSON Schema).

### 7. Adapters must be thin

Adapters (CLI, HTTP, future Kafka, Redis, etc.) **must only**: parse input, call the engine, return output.

Adapters **must not**: implement spatial logic, own long-lived engine state beyond wiring, or encode business rules.

---

## Crate responsibilities

| Crate | Responsibility |
|-------|----------------|
| `crates/engine` | Owns event processing; `GeoEngine` (`process_event` + registration), `Engine` (`process_batch`, `SpatialRule` pipeline). |
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

- Trait `GeoEngine`: zone registration + **`process_event(&mut self, PointUpdate) -> Vec<Event>`**.
- **`Engine`**: `process_batch`, **`with_rules`**, **`register_zone_with_dwell`** (`ZoneDwell`: min inside before Enter, min outside before Exit). Plain **`register_zone`** uses default (instant) dwell. Default **`SpatialRule`** pipeline in `crates/engine/src/rules.rs`.
- Input update: `PointUpdate { id, x, y, t_ms }` (Unix epoch milliseconds). Wire JSON field `t` in adapters.
- Concrete type: `Engine` backed by `spatial::NaiveSpatialIndex` and `HashMap<String, EntityState>`.

**Emitted events** (`crates/state`): `Event` is an enum — `Enter` / `Exit`, `Approach` / `Recede` (radius), `AssignmentChanged` (catalog); each variant includes **`t_ms`** (same as the causing `PointUpdate`). After `process_batch`, event order is deterministic (`sort_events_deterministic`).

**Per-entity state** (`crates/state`): `EntityState` holds `position`, `last_t_ms`, zone membership plus **`zone_enter_pending` / `zone_exit_pending`** (dwell timers), radius set, and `catalog_region`.

**Supporting crate:** `crates/polygon-json` parses GeoJSON polygons for HTTP and stdin-stdout adapters (not used inside `crates/engine`).

---

## Execution model (target)

```text
Event → Engine → Rules → State transition → Output events
```

---

## Current state of the codebase

The project currently:

- **API:** `process_event` is primary; **`process_batch`** for buffered NDJSON/HTTP batches. CLI defaults `batch_size` to 1 (one `process_batch` per update line).
- **Zone kinds:** Zones (enter/exit), catalog regions (assignment / tie-break by smallest id), circles (approach/recede).
- **Spatial:** `SpatialIndex` trait exists; `NaiveSpatialIndex` implements R-tree–accelerated polygon queries plus linear radius checks.
- **`SpatialRule` pipeline** in `crates/engine/src/rules.rs` (default: zone → circle → catalog).
- **Adapters:** stdin-stdout calls **`Engine::process_batch`**; `run()` is **`&mut Engine`** (not generic over `GeoEngine`).
- **Protocol:** NDJSON wire contract under `protocol/ndjson.md` (pre-release).

**Known gap:** Optional non-`Engine` adapter generics if needed. JSON Schema for NDJSON and HTTP wire shapes lives under `protocol/schema/` (see `protocol/schema/README.md`).

---

## Immediate goals (V1 direction)

1. **Done (baseline):** `process_event` + `SpatialRule` pipeline + `Engine::process_batch` for multi-update ordering.
2. Optional: custom rule sets per deployment, adapter ergonomics, JSON Schema for protocol.
3. Explicit state transitions: keep `state` helpers pure where possible.
4. Batch remains a thin wrapper (`process_batch`), not the core abstraction.

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
- At this stage backwards compatibility is not important. As long as tests pass.
 
**Guiding principle:** Build the engine abstraction first, not the ecosystem.
