# Geo-stream Roadmap

This document is the canonical reference for past, present, and future development. It covers what is done, what needs fixing, planned features, and the longer-term product direction.

---

## What is done

### Core engine

- `GeoEngine` trait: zone registration + `process_event(PointUpdate) -> Result<Vec<Event>, EngineError>`
- Monotonicity enforcement: `process_event` returns `EngineError::MonotonicityViolation` for strictly backwards timestamps per entity
- `Engine::process_batch`: sort by `(id, t_ms)` → process each → `sort_events_deterministic`; returns `(Vec<Event>, Vec<EngineError>)` — monotonicity violations are skipped, not fatal
- `SpatialRule` trait: composable, ordered pipeline of spatial checks per update; takes `&dyn SpatialIndex` (not the concrete type)
- Default pipeline: `ZoneRule → RadiusRule → CatalogRule`
- `Engine::with_rules`: custom rule sets per deployment
- `ZoneDwell`: per-zone `min_inside_ms` / `min_outside_ms` with pending-map cancellation on bounce-back

### Zone types

| Zone | Events emitted | Index |
|------|---------------|-------|
| Zone (polygon, with holes) | `Enter` / `Exit` | R-tree (bounding box) + exact point-in-polygon |
| Catalog region (polygon layer) | `AssignmentChanged` (lex-smallest containing region) | R-tree |
| Circle (disk) | `Approach` / `Recede` | R-tree (inflated AABB) |

### State

- Per-entity `EntityState`: position, last timestamp, zone membership (`inside`), circle membership, catalog assignment
- Dwell pending maps (`zone_enter_pending`, `zone_exit_pending`) cancel on bounce-back before threshold elapses
- `sort_events_deterministic`: stable ordering by `(entity_id, t_ms, tier, zone_id, enter_before_exit)`

### Spatial abstraction

- `SpatialIndex` trait exposes `zone_membership_at`, `circle_membership_at`, `primary_catalog_at` — fully decoupled from `NaiveSpatialIndex`
- Custom rules and alternate index implementations are now possible without modifying engine code

### Adapters

- **stdin-stdout**: NDJSON line-by-line, batching strategies (`--batch-size N`)
- **NAPI (Node.js)**: native Rust bindings via NAPI; `GeoEngine` class with `registerZone`, `registerCatalogRegion`, `registerCircle`, `ingest`
- **Protocol**: NDJSON wire contract at `protocol/ndjson.md`, JSON Schema under `protocol/schema/`

### TypeScript package (`geo-stream/`)

- `GeoEngine` typed wrapper with `GeoEvent` discriminated union and `GeoJsonPolygonInput` (uses `@types/geojson`)
- `GeoEventEmitter` (`/emitter`) — wraps `GeoEngine` as a Node.js `EventEmitter`; fully typed `on`/`once`/`off` overloads per event kind
- `GeoStreamKafka` (`/kafka`) — consumes `PointUpdate` JSON from a Kafka topic, publishes `GeoEvent` JSON to an output topic; structural typing (no hard `kafkajs` dep)
- `GeoStreamRedis` (`/redis`) — `XREAD BLOCK` poll loop on a Redis input stream, `XADD` to output stream; structural typing (no hard `ioredis` dep)
- 38 unit tests across all adapters using `node:test` + `tsx`; no native binary or live broker required

### Crate structure

```
crates/engine/              — GeoEngine, Engine, SpatialRule pipeline
crates/state/               — EntityState, Event enum, membership transitions
crates/spatial/             — Zone, Circle, NaiveSpatialIndex (R-tree), GeoJSON polygon parsing
crates/adapters/stdin-stdout/
crates/adapters/napi/       — Node.js NAPI bindings (compiled into geo-stream/ npm package)
crates/cli/                 — geo-stream binary
geo-stream/                 — npm package: typed wrappers + TypeScript adapters
```

### Tooling

- Criterion benchmarks on `process_batch` hot path
- Multi-stage Docker image
- GitHub Actions CI
- Makefile for build / test / bench / docker

---

## Known issues and cleanup

These are correctness or design gaps that should be resolved before v1.

### Medium priority

**1. Dwell / debounce is zone-only**
Circles have no equivalent of `min_inside_ms` / `min_outside_ms`. GPS noise near a circle boundary causes approach/recede flapping in the same way as near a polygon boundary.

**2. No test for global zone ID uniqueness across types**
A zone, circle, and catalog region cannot share the same ID. The `DuplicateZoneId` error path is not exercised in any test.

**3. No test for equal-timestamp updates**
`process_event` allows equal timestamps (`t_ms == last_t_ms`) — this is intentional (same-timestamp batch items are valid). The behaviour is unspecified in the protocol and untested.

### Lower priority

**4. `membership_scratch` swap pattern is undocumented**
`RadiusRule` uses `std::mem::swap` to transfer new state into `EntityState` and hand old state back to scratch. This is efficient but non-obvious. A comment on the invariant would prevent future regressions.

---

## v1 milestones

These define what a stable, reliable v1 looks like.

### v1.0 — Correctness and abstraction cleanup

- [x] `SpatialRule::apply` uses `&dyn SpatialIndex`, not the concrete type
- [x] R-tree spatial index for circles
- [x] Polygon holes handled correctly in point-in-polygon
- [x] Timestamp monotonicity enforced per entity (`EngineError::MonotonicityViolation`)
- [x] Zone ID scoping resolved
- [x] `polygon-json` merged into `crates/spatial`
- [ ] Add missing tests: cross-type duplicate IDs, equal-timestamp updates
- [ ] Dwell / debounce support for circles
- [ ] Stabilise the NDJSON wire protocol to v1 (no breaking changes after this)

### v1.1 — Operability

- [ ] Engine state snapshot + restore (serialize `EntityState` map to JSON/msgpack for process restart)
- [ ] Structured tracing in the engine (enter, exit, dwell pending state changes)
- [ ] Runtime zone deregistration (remove a zone by ID without restarting)
- [ ] Zone update (replace a polygon for an existing ID without losing entity state)

---

## v2 milestones — Ecosystem

These make geo-stream useful beyond direct Rust embedding.

### Client SDKs

- [x] **TypeScript/Node.js SDK**: NAPI bindings (`crates/adapters/napi`); `GeoEngine` class; `registerZone`, `registerCatalogRegion`, `registerCircle`, `ingest`; typed `GeoEvent` discriminated union and `GeoJsonPolygonInput`; pre-built native binaries for macOS/Linux/Windows; npm README
- [ ] **Python SDK**: subprocess or HTTP; matches TypeScript API shape

### TypeScript adapters

- [x] **EventEmitter** (`/emitter`): wraps `GeoEngine` as a Node.js `EventEmitter`; typed `on`/`once`/`off` overloads per event kind; no extra deps
- [x] **Kafka** (`/kafka`): `PointUpdate` JSON in, `GeoEvent` JSON out via Kafka topics; structural typing — works with any kafkajs-compatible client
- [x] **Redis Streams** (`/redis`): `XREAD BLOCK` input, `XADD` output; structural typing — works with ioredis and node-redis v4+
- [ ] **WebSockets**: bidirectional adapter — devices push GPS fixes over WS, events pushed back on the same connection; natural fit for live dashboards and mobile clients
- [ ] **MQTT**: subscribe to `devices/{id}/location`, publish to `events/{id}`; `mqtt.js` compatible; low overhead, good for IoT/embedded device fleets
- [ ] **HTTP SSE**: long-lived HTTP response streaming `GeoEvent` as `data: {...}\n\n`; browser-native, no WS upgrade needed; good for read-only dashboards
- [ ] **Webhook**: receive location updates via HTTP POST, emit `GeoEvent` to a configurable outbound URL; useful for third-party SaaS GPS integrations
- [ ] **NDJSON file replay** (TypeScript): read a `.ndjson` history file, process through `GeoEngine`, collect events; useful for backtesting zone configurations

### Zone management

- [ ] Batch zone registration (load a GeoJSON FeatureCollection in one call)

---

## v3 milestones — Advanced spatial logic

### Rule extensions

- [ ] **Speed rules**: emit events when entity velocity exceeds a threshold between consecutive updates
- [ ] **Heading rules**: emit events when direction of travel changes relative to a zone
- [ ] **Dwell aggregation**: emit a `Dwelling` event after an entity has been inside a zone for N ms (separate from the existing entry dwell which delays the `Enter` event itself)
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
- **Embedded in Node.js** via NAPI bindings (HTTP serving handled by the TypeScript layer)
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
| Flink / Kafka Streams | Powerful but no built-in zone lifecycle. You hand-roll point-in-polygon, membership state, dwell logic, and ordering. Geo-stream does all of this in a small, tested core. |
| Cloud geofencing APIs | Managed, but vendor lock-in, limited programmability, per-event pricing at scale. Geo-stream is self-hosted and embeddable. |
| Desktop / web GIS tools | Optimized for human workflows and visualization. Geo-stream is developer-first: crates, deterministic tests, container entrypoints, SDKs. |

---

## What this project is not (and should stay not)

- Not a database — no persistence, querying, or transactional storage in the core
- Not a GIS platform — no editing, styling, reprojection, or analyst UI
- Not a general streaming framework — narrowly focused on spatial event transitions
- Not a visualization tool — events are data; rendering is someone else's job
