# Geo-stream Roadmap

This document is the living checklist for past, present, and future development. It covers what is done, what needs fixing, planned features. It is not comprehensive, once work has been completed eventually checked items are removed.

## Critical gaps — blockers for real-world adoption

These are not v1 polish items; they are fundamental limitations that prevent the engine from being used on real GPS data.

- [ ] **Geodesic / WGS84 coordinate support**: all spatial math is currently Euclidean (flat plane). Passing GPS lat/lng coordinates directly produces silently wrong results — a "500 m" circle radius has no defined meaning in degree-space. Add a `CoordinateSystem` enum (`Planar` | `WGS84`) at engine construction time; `WGS84` mode routes distance and containment checks through Haversine/geodesic implementations already available in the `geo` crate (`HaversineDistance`, `GeodesicDistance`). Without this, the engine cannot be used for any mobile, fleet, or IoT use case.
- [ ] **Publish to crates.io**: `geo-stream-engine` and `geo-stream-state` are not on crates.io. Rust users cannot use the engine as a library dependency without vendoring the whole repository.
- [ ] **Publish benchmark numbers**: `cargo bench -p engine` runs but results are never published. A single table of throughput figures (updates/sec at various zone counts, on reference hardware) in the README removes a key evaluation blocker.

## v1 milestones

These define what a stable, reliable v1 looks like.

### Correctness and abstraction cleanup

- [x] Dwell / debounce support for circles
- [x] `process_batch`: errors logged to stderr as NDJSON (stdin-stdout adapter)
- [ ] Refactor: extract speed/heading and history-buffer logic from `process_event()` into focused helpers for isolated unit tests
- [ ] Tests: ConfigurableRule — missing coverage: `SpeedBelow` filter, `HeadingBetween` with wrap-around (e.g. 350°→10°), multiple triggers matching a single event
- [ ] Tests: SequenceRule — missing coverage: parallel entities maintaining independent progress, sequence reset after completion
- [ ] Refactor: `DwellContext` (or equivalent) to trim `membership_with_dwell_impl` 10-parameter surface (`#[allow(clippy::too_many_arguments)]`)
- [ ] Docs: heading/`enrich` convention and `EventTier` ordering rationale (`process_event` monotonicity, `process_batch` error handling, `RuleContext` as needed)
- [ ] Integration test: malformed NDJSON input (CLI edge cases: partial geometry, `batch_size=0`)
- [ ] Criterion benchmark: configurable / sequence rule firing hot path (current bench only covers zone/catalog/circle)

### v1.1 — Operability

- [x] Engine state snapshot + restore (serialize `EntityState` map to JSON for process restart)
- [ ] **Snapshot completeness**: in-flight sequence progress is not currently serialised into snapshots, meaning a process restart silently drops partially completed sequences. Fix before the snapshot API is considered stable.
- [ ] Tests: snapshot round-trips — missing coverage: dwell state, configurable rule config, sequence rule config, multi-entity state, corrupted/truncated restore
- [ ] Structured tracing in the engine (enter, exit, dwell pending state changes)
- [ ] Runtime zone deregistration (remove a zone by ID without restarting)
- [ ] Zone update (replace a polygon for an existing ID without losing entity state)

### Client SDKs

- [x] **TypeScript/Node.js SDK**: NAPI bindings (`crates/adapters/napi`); `GeoEngine` class; `registerZone`, `registerCatalogRegion`, `registerCircle`, `ingest`; typed `GeoEvent` discriminated union and `GeoJsonPolygonInput`; pre-built native binaries for macOS/Linux/Windows; npm README
- [ ] **Python bindings (PyO3)**: Python dominates logistics, mobility analytics, and ML-driven location services. A PyO3 wrapper with equivalent API surface to the NAPI bindings is the highest-leverage language expansion.

### TypeScript adapters

- [x] **EventEmitter** (`/emitter`): wraps `GeoEngine` as a Node.js `EventEmitter`; typed `on`/`once`/`off` overloads per event kind; no extra deps
- [x] **Kafka** (`/kafka`): `PointUpdate` JSON in, `GeoEvent` JSON out via Kafka topics; structural typing — works with any kafkajs-compatible client
- [x] **Redis Streams** (`/redis`): `XREAD BLOCK` input, `XADD` output; structural typing — works with ioredis and node-redis v4+
- [ ] TypeScript: tighten `RuleEvent` typing (replace `[key: string]: unknown` index signature with specific fields)
- [ ] TypeScript (`rules.ts`): thread rule `name` through `RuleBuilder.emit()` instead of setting `name: ""` and overwriting it in `GeoEngine.defineRule`
- [ ] **HTTP/SSE server mode**: a thin axum or actix layer wrapping the engine as an HTTP server — POST location updates, receive `GeoEvent` as Server-Sent Events. Makes the engine accessible from any language without native bindings, bridging the polyglot gap. Ship as an opt-in feature flag.
- [ ] **WebSockets**: bidirectional adapter — devices push GPS fixes over WS, events pushed back on the same connection; natural fit for live dashboards and mobile clients
- [ ] **MQTT**: subscribe to `devices/{id}/location`, publish to `events/{id}`; `mqtt.js` compatible; low overhead, good for IoT/embedded device fleets
- [ ] **Webhook**: receive location updates via HTTP POST, emit `GeoEvent` to a configurable outbound URL; useful for third-party SaaS GPS integrations
- [ ] **NDJSON file replay** (TypeScript): read a `.ndjson` history file, process through `GeoEngine`, collect events; useful for backtesting zone configurations

### Zone management

- [ ] Batch zone registration (load a GeoJSON FeatureCollection in one call)

---

## v1.2 — Entity-entity proximity (roaming geofences)

Entity ↔ entity proximity is a first-class use case in fleet management, delivery tracking, and crowd analytics — and one of the most-requested features for any geofencing system. Moving it to v1.2 (rather than v2) reflects its importance to adoption.

- [ ] Emit `Proximity` events when two entities come within a configurable radius of each other
- [ ] Emit `ProximityEnd` when they separate beyond the threshold
- [ ] Dwell support for proximity (avoid noisy fire/clear cycling on the boundary)
- [ ] `entities_near_entity(id, radius)` query
- [ ] Requires per-entity position to be maintained in the spatial index — a meaningful state model change; design carefully to preserve the existing `(old_state, event) → (new_state, outputs)` invariant

---

## v2 milestones — Advanced spatial logic

### Rule extensions

- [ ] **Speed rules**: emit events when entity velocity exceeds a threshold between consecutive updates
- [ ] **Heading rules**: emit events when direction of travel changes relative to a zone
- [ ] **Dwell aggregation**: emit a `Dwelling` event after an entity has been inside a zone for N ms (separate from the existing entry dwell which delays the `Enter` event itself)
- [ ] **Temporal rules**: suppress events between certain time windows (e.g. ignore exits at night)

### Geometry expansion (post-WGS84)

These are only meaningful once WGS84 support exists.

- [ ] **LineString corridors**: enter/exit events when an entity crosses into a buffer around a route line; useful for route compliance and road-snapping
- [ ] **MultiPolygon**: support complex administrative boundaries (countries, postal districts) without manually splitting into constituent polygons
- [ ] **GeoJSON FeatureCollection ingestion**: register an entire FeatureCollection as zones in one call

### Trajectory analysis

- [ ] Smoothing / dead-reckoning to reduce noise before rule evaluation
- [ ] Path interpolation between sparse GPS samples for more accurate enter/exit timestamps
