# Geo-stream Roadmap

This document is the canonical reference for past, present, and future development. It covers what is done, what needs fixing, planned features, and the longer-term product direction.

---

## What is done (v0.0.1 baseline)

### Core engine

- `GeoEngine` trait: zone registration + `process_event(PointUpdate) -> Vec<Event>`
- `Engine::process_batch`: sort by `(id, t_ms)` → process each → `sort_events_deterministic`
- `SpatialRule` trait: composable, ordered pipeline of spatial checks per update
- Default pipeline: `GeofenceRule → CorridorRule → RadiusRule → CatalogRule`
- `Engine::with_rules`: custom rule sets per deployment
- `geofence_dwell`: per-fence `min_inside_ms` / `min_outside_ms` with pending-map cancellation on bounce-back

### Zone types

| Zone | Events emitted | Index |
|------|---------------|-------|
| Geofence (polygon) | `Enter` / `Exit` | R-tree (bounding box) + exact point-in-polygon |
| Corridor (polygon layer) | `EnterCorridor` / `ExitCorridor` | R-tree |
| Catalog region (polygon layer) | `AssignmentChanged` (lex-smallest containing region) | R-tree |
| Radius zone (disk) | `Approach` / `Recede` | Linear scan |

### State

- Per-entity `EntityState`: position, last timestamp, geofence membership (`inside`), corridor membership, radius membership, catalog assignment
- Dwell pending maps (`geofence_enter_pending`, `geofence_exit_pending`) cancel on bounce-back before threshold elapses
- `sort_events_deterministic`: stable ordering by `(entity_id, t_ms, tier, zone_id, enter_before_exit)`

### Adapters

- **stdin-stdout**: NDJSON line-by-line, batching strategies (`--batch-size N`)
- **HTTP** (optional `http` feature): Axum server, `/v1/{register_*,ingest}`, `GET /health`, `GET /openapi.json`
- **Protocol**: NDJSON wire contract at `protocol/ndjson.md`, JSON Schema under `protocol/schema/`

### Crate structure

```
crates/engine/          — GeoEngine, Engine, SpatialRule pipeline
crates/state/           — EntityState, Event enum, membership transitions
crates/spatial/         — Geofence, RadiusZone, NaiveSpatialIndex (R-tree)
crates/polygon-json/    — GeoJSON polygon parsing helper
crates/adapters/stdin-stdout/
crates/adapters/http/
crates/cli/             — geo-stream, geo-stream-http binaries
```

### Tooling

- Criterion benchmarks on `process_batch` hot path
- Multi-stage Docker image
- GitHub Actions CI
- Makefile for build / test / bench / docker

---

## Known issues and cleanup

These are correctness or design gaps that should be resolved before v1.

### High priority

**1. `SpatialRule::apply` is coupled to `NaiveSpatialIndex`**
The trait takes `&NaiveSpatialIndex` directly. Custom rules and alternate index implementations are blocked until this is `&dyn SpatialIndex` (or a generic bound). This is the most important abstraction gap.

**2. Polygon holes are silently ignored**
Point-in-polygon only tests the exterior ring. A point inside a hole of a registered geofence will incorrectly report as inside. This is a silent correctness bug for any geofence that has an exclusion zone.

**3. Out-of-order timestamps are undefined behavior**
`process_event` accepts any `t_ms`. No check prevents an update with a past timestamp from being processed against state that was built from future timestamps. The dwell timer logic in particular can produce incorrect events if timestamps go backwards. Document the contract explicitly and/or enforce monotonicity per entity.

**4. Radius zones have no spatial index**
`radius_membership_at` is a linear scan over `Vec<RadiusZone>`. This is O(n) per update per entity. At thousands of zones this will dominate the hot path. Radius zones are just inflated point AABBs and should get R-tree treatment identical to polygons.

### Medium priority

**5. Zone ID uniqueness is global across all types**
A geofence and a corridor cannot share the same ID even though they are distinct concepts. This is surprising to users who naturally namespace by type. Either: (a) document this strongly and enforce at the API surface with a clear error, or (b) make IDs scoped per zone type and update the wire protocol accordingly.

**6. Dwell / debounce is geofence-only**
Corridors, radius zones, and catalog regions have no equivalent of `min_inside_ms` / `min_outside_ms`. GPS noise near corridor boundaries causes flapping in the same way as near geofence boundaries. At minimum corridors should get dwell support.

**7. `polygon-json` is a 30-line utility that does not need to be its own crate**
It could live in `crates/spatial` since it is purely a geometry helper. Reduces workspace overhead.

### Lower priority

**8. No test for global zone ID uniqueness across types**
The cross-type duplicate ID error is not exercised in any test.

**9. No test for out-of-order or equal-timestamp updates**
What happens when two updates for the same entity arrive with the same `t_ms`? The behavior is currently unspecified.

**10. `membership_scratch` swap pattern is hard to follow**
`CorridorRule` and `RadiusRule` use `std::mem::swap` to move new state into entity state and hand old state back to scratch. This is efficient but subtle. A comment explaining the invariant would prevent future regressions.

---

## v1 milestones

These define what a stable, reliable v1 looks like.

### v1.0 — Correctness and abstraction cleanup

- [x] Fix `SpatialRule::apply` to use `SpatialIndex` trait (or generic bound) rather than `NaiveSpatialIndex`
- [x] Implement R-tree spatial index for radius zones
- [x] Handle polygon holes correctly in point-in-polygon
- [x] Define and enforce timestamp monotonicity contract per entity; add tests for violations
- [x] Add dwell / debounce support for corridors
- [x] Resolve zone ID scoping (global vs per-type); update protocol if changed
- [x] Merge `polygon-json` into `crates/spatial`
- [ ] Add missing tests (cross-type duplicate IDs, timestamp edge cases)
- [ ] Stabilise the NDJSON wire protocol to v1 (no breaking changes after this)

### v1.1 — Observability and operability

- [ ] Structured tracing throughout the engine (enter, exit, dwell pending state changes)
- [ ] Per-entity and per-zone event counters (Prometheus-compatible or embeddable metrics)
- [ ] Health endpoint reports registered zone counts and entity state size
- [ ] Engine state snapshot + restore (serialize `EntityState` map to JSON/msgpack for process restart)

---

## v2 milestones — Ecosystem

These make geo-stream useful beyond direct Rust embedding.

### Adapters

- [ ] **Kafka consumer adapter**: consume location updates from a Kafka topic, emit events to another topic; offset commit after processing
- [ ] **Redis Streams adapter**: XREAD input, XADD output; compatible with Redis cluster
- [ ] **File ingestion adapter**: replay CSV / GeoJSON NDJSON history; useful for backtesting fence configurations

### Client SDKs

- [x] **TypeScript/Node.js SDK** (high priority): wrap the CLI subprocess or HTTP adapter; typed event types; async iterator interface
- [ ] **Python SDK**: subprocess or HTTP; matches TypeScript API shape

### Zone management

- [ ] Runtime zone deregistration (remove a fence by ID without restarting the engine)
- [ ] Zone update (replace a polygon for an existing ID without losing entity state)
- [ ] Batch zone registration (load a GeoJSON FeatureCollection in one call)

---

## v3 milestones — Advanced spatial logic

### Rule extensions

- [ ] **Speed rules**: emit events when entity velocity exceeds a threshold between consecutive updates
- [ ] **Heading rules**: emit events when direction of travel changes relative to a corridor or zone
- [ ] **Dwell aggregation**: emit a `Dwelling` event after an entity has been inside a geofence for N ms (separate from the existing entry dwell which delays the `Enter` event itself)
- [ ] **Temporal rules**: suppress events between certain time windows (e.g. ignore exits at night)

### Spatial joins (entity ↔ entity)

- [ ] Track proximity between entities (not just entity ↔ zone)
- [ ] Emit `Proximity` events when two entities come within a radius of each other
- [ ] This requires per-entity position to be queryable by the spatial index; significant state model change

### Trajectory analysis

- [ ] Smoothing / dead-reckoning to reduce noise before rule evaluation
- [ ] Path interpolation between sparse GPS samples for more accurate enter/exit timestamps

---

## Big picture

### What geo-stream should become

A developer-first, embeddable geospatial stream processor that can run:

- **In-process** as a Rust library crate
- **As a subprocess** via NDJSON stdin/stdout from any language
- **As a sidecar** over HTTP, called by application services
- **As a stream processor** consuming from Kafka or Redis Streams

The goal is that a developer should be able to add geofencing to any application — regardless of language, infrastructure, or scale — without needing PostGIS, Flink, or a dedicated GIS team.

### Persistence strategy (future)

The engine is intentionally in-memory. For durability, two approaches are viable:

1. **State snapshots**: serialize `EntityState` map on shutdown, restore on startup. Acceptable for single-node deployments with occasional restarts.
2. **External state store**: replace `HashMap<String, EntityState>` with a pluggable state backend (Redis, DynamoDB). The engine's explicit state model makes this tractable without changing event semantics.

Neither approach requires changing the core engine API.

### Distribution strategy (future)

Partition by entity ID across multiple `Engine` instances. Each shard owns a subset of entity state and all zone definitions (zones are replicated, state is sharded). Events are emitted per-shard; a merge step applies `sort_events_deterministic` across shard outputs. The deterministic event ordering model already makes this feasible.

### Positioning

| Approach | How geo-stream differs |
|----------|------------------------|
| PostGIS | Store + query over rows. Geo-stream is a stateful stream processor that diffs membership over time and emits events. No database contract. |
| Flink / Kafka Streams | Powerful but no built-in geofence lifecycle. You hand-roll point-in-polygon, membership state, dwell logic, and ordering. Geo-stream does all of this in a small, tested core. |
| Cloud geofencing APIs | Managed, but vendor lock-in, limited programmability, per-event pricing at scale. Geo-stream is self-hosted and embeddable. |
| Desktop / web GIS tools | Optimized for human workflows and visualization. Geo-stream is developer-first: crates, deterministic tests, container entrypoints, SDKs. |

---

## What this project is not (and should stay not)

- Not a database — no persistence, querying, or transactional storage in the core
- Not a GIS platform — no editing, styling, reprojection, or analyst UI
- Not a general streaming framework — narrowly focused on spatial event transitions
- Not a visualization tool — events are data; rendering is someone else's job
