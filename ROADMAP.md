# Geo-stream Roadmap

This document is the living checklist for past, present, and future development. It covers what is done, what needs fixing, planned features. It is not comprehensive, once work has been completed eventually checked items are removed.

## v1 milestones

These define what a stable, reliable v1 looks like.

### Correctness and abstraction cleanup

- [x] Dwell / debounce support for circles
- [ ] Refactor: extract `process_event()` into focused subfunctions (e.g. speed/heading, history, spatial rules, configurable rules, sequence rules) for isolated tests
- [ ] Tests: ConfigurableRule (`trigger_matches` / `fire`, each filter type, heading wrap, multiple triggers on one event)
- [ ] Tests: SequenceRule timeouts (`within_ms` reset, parallel entities, reset after completion)
- [ ] Refactor: `DwellContext` (or equivalent) to trim `membership_with_dwell_impl` parameter surface
- [ ] Docs: heading/`enrich` convention and `EventTier` ordering rationale (`process_event` monotonicity, `process_batch` errors, `RuleContext` as needed)
- [ ] `process_batch`: optional error logging or callback so skipped errors are not silent
- [ ] Integration test: malformed NDJSON input (and CLI edge cases: partial geometry, `batch_size=0` if applicable)
- [ ] Criterion benchmark: configurable / sequence rule firing hot path

### v1.1 — Operability

- [x] Engine state snapshot + restore (serialize `EntityState` map to JSON/msgpack for process restart)
- [ ] Tests: snapshot round-trips for all rule types (dwell, configurable, sequence), multi-entity state, corrupted/truncated restore paths
- [ ] Structured tracing in the engine (enter, exit, dwell pending state changes)
- [ ] Runtime zone deregistration (remove a zone by ID without restarting)
- [ ] Zone update (replace a polygon for an existing ID without losing entity state)

### Client SDKs

- [x] **TypeScript/Node.js SDK**: NAPI bindings (`crates/adapters/napi`); `GeoEngine` class; `registerZone`, `registerCatalogRegion`, `registerCircle`, `ingest`; typed `GeoEvent` discriminated union and `GeoJsonPolygonInput`; pre-built native binaries for macOS/Linux/Windows; npm README

### TypeScript adapters

- [x] **EventEmitter** (`/emitter`): wraps `GeoEngine` as a Node.js `EventEmitter`; typed `on`/`once`/`off` overloads per event kind; no extra deps
- [x] **Kafka** (`/kafka`): `PointUpdate` JSON in, `GeoEvent` JSON out via Kafka topics; structural typing — works with any kafkajs-compatible client
- [x] **Redis Streams** (`/redis`): `XREAD BLOCK` input, `XADD` output; structural typing — works with ioredis and node-redis v4+
- [ ] TypeScript: tighten `RuleEvent` typing (replace loose index signature with specific fields)
- [ ] TypeScript (`rules.ts`): pass rule `name` into `emit()` instead of overwriting after emit
- [ ] **WebSockets**: bidirectional adapter — devices push GPS fixes over WS, events pushed back on the same connection; natural fit for live dashboards and mobile clients
- [ ] **MQTT**: subscribe to `devices/{id}/location`, publish to `events/{id}`; `mqtt.js` compatible; low overhead, good for IoT/embedded device fleets
- [ ] **HTTP SSE**: long-lived HTTP response streaming `GeoEvent` as `data: {...}\n\n`; browser-native, no WS upgrade needed; good for read-only dashboards
- [ ] **Webhook**: receive location updates via HTTP POST, emit `GeoEvent` to a configurable outbound URL; useful for third-party SaaS GPS integrations
- [ ] **NDJSON file replay** (TypeScript): read a `.ndjson` history file, process through `GeoEngine`, collect events; useful for backtesting zone configurations

### Zone management

- [ ] Batch zone registration (load a GeoJSON FeatureCollection in one call)

---

## v2 milestones — Advanced spatial logic

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

