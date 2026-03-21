# Geo-stream roadmap and positioning

## 1. What this system is

**Geo-stream** is a **single-node, in-memory geospatial stream processor**. It accepts **batches of point location updates**, maintains **per-entity state** (last position and current geofence membership), and emits **Enter** / **Exit** events when point-in-polygon containment changes. The core is **batch-oriented** (`ingest(Vec<PointUpdate>)`) rather than a callback per point.

It is designed to run as a **standalone process** (CLI + NDJSON) or behind **thin adapters** (HTTP, future Kafka consumers, etc.) that perform only IO and serialization — **not** spatial logic.

## 2. What it is NOT

- **Not a database** — no persistence, querying, or transactional storage.
- **Not a GIS platform** — no editing, styling, reprojection catalog, or analyst UI.
- **Not a distributed system (yet)** — Phase 1 is one process, one memory space.
- **Not a wrapper around desktop GIS** — it uses Rust geometry crates (`geo`, `geojson`) for correctness and control, not ArcGIS/QGIS as engines.

## 3. Why Rust

- **Performance:** Tight memory layout, predictable CPU use for hot paths (point-in-polygon over moderate fence counts).
- **Memory control:** No GC pauses; suitable for latency-sensitive streaming adapters later.
- **Concurrency potential:** The engine API is intentionally side-effect free on IO; internal parallelism (parallel batches, sharded state) can be added without changing the **semantics** of `ingest` if done carefully.
- **Correctness:** Strong types for geometry and explicit error handling (`register_geofence` validation, closed rings).

## 4. Why language-agnostic design

- **NDJSON over stdin/stdout** works from **any** language that can spawn a process and read/write text.
- **Containers** standardize deployment; clients only need the **protocol** ([`ndjson-v1.md`](ndjson-v1.md)), not Rust.
- **Optional HTTP v2** lets services integrate without subprocess management, still using the same **core engine** crate.

This separation (engine library + adapters) keeps **business logic** in one place and **integration glue** replaceable.

## 5. Evolution path

1. **Single-node POC (current):** correct enter/exit, naive linear scan over fences, NDJSON CLI.
2. **Adapters:** stdin/stdout (done), HTTP sketch (`http-adapter` with `server` feature), later Kafka/File adapters **outside** the engine crate.
3. **Performance pass:** fewer allocations per batch, reuse buffers, micro-optimizations on containment checks — **still no complex spatial index** until requirements demand it.
4. **Spatial indexing:** R-tree or grid when fence count or QPS requires it; keep `SpatialIndex` trait so callers can swap implementations.
5. **Distributed (future):** partition by entity id, deterministic merge of events, or embed engine shards behind a router — **out of scope** until single-node limits are understood.

## 6. Comparison vs existing approaches

| Approach | How geo-stream differs |
|----------|-------------------------|
| **PostGIS** | PostGIS is **store + query**: you issue SQL, get rows. Geo-stream is a **stateful stream processor** that **diffs membership over time** and emits **events**, with no database contract. |
| **General streaming (Flink, Beam, Kafka Streams)** | Powerful windows and joins, but **no built-in geofence lifecycle**; you would hand-roll point-in-polygon, state, and ordering. Geo-stream **narrows** the problem to geofencing transitions with a **small, testable core**. |
| **Desktop / web GIS tools** | Optimized for human workflows and visualization. Geo-stream is **developer-first**: crates, tests, deterministic batches, container entrypoints. |

---

## Implemented layout (reference)

```text
crates/
  engine/     — GeoEngine, Engine, PointUpdate
  spatial/    — Geofence, point-in-polygon, NaiveSpatialIndex, SpatialIndex
  state/      — EntityState, Event, membership diff + sort
  adapters/
    stdin-stdout/
    http/     — optional Axum server (`server` feature)
  cli/        — geo-stream (default), geo-stream-http (--features http)
protocol/     — NDJSON v1 + this roadmap
docker/       — multi-stage image, CLI entrypoint
```
