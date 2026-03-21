# Geo-stream

A small, in-memory **geospatial stream processor** in Rust: point updates go in, **enter** / **exit** geofence events come out. The core engine has **no network or Kafka**—only adapters (NDJSON CLI, optional HTTP) talk to the outside world.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (stable), with `cargo` on your `PATH`
- Optional: [Docker](https://docs.docker.com/get-docker/) for container builds

## Local development

Clone the repository and from the **repository root** (where this `README.md` and `Cargo.toml` live):

```bash
cargo build
cargo test
```

`cargo test -p cli` also runs NDJSON integration tests (v1 smoke + v1.1 fixtures under `crates/cli/tests/fixtures/`).

Release build (matches what the Dockerfile compiles):

```bash
cargo build --release -p cli --bin geo-stream
```

## Getting started

### Run the CLI on sample data

The `geo-stream` binary reads **newline-delimited JSON** from stdin and writes events to stdout (errors go to stderr).

```bash
cargo run -p cli --bin geo-stream -- < examples/sample-input.ndjson
```

You should see two lines similar to:

```json
{"event":"enter","id":"c1","geofence":"zone-1"}
{"event":"exit","id":"c1","geofence":"zone-1"}
```

### Input shape (quick reference)

- Register a fence (GeoJSON polygon):

  `{"type":"register_geofence","id":"zone-1","polygon":{...}}`

- Point update:

  `{"type":"update","id":"c1","location":[x,y]}`

Full contract: [protocol/ndjson-v1.md](protocol/ndjson-v1.md). Corridors, catalog assignment, and radius zones: [protocol/ndjson-v1.1.md](protocol/ndjson-v1.1.md).

### Batching

```bash
cargo run -p cli --bin geo-stream -- --batch-size 0 -- < examples/sample-input.ndjson
```

- `--batch-size 1` (default): one `update` line → one engine batch.
- `--batch-size N` (`N > 1`): buffer `N` updates, then ingest.
- `--batch-size 0`: buffer all updates until EOF, then one ingest.

Fence registration lines are always applied immediately; they are not batched.

### Optional HTTP adapter

Build the Axum-based binary (same engine, JSON over HTTP):

```bash
cargo build -p cli --features http --bin geo-stream-http
./target/debug/geo-stream-http --listen 0.0.0.0:8080
```

- **`--listen`:** bind address (default `0.0.0.0:8080`).
- **`GET /health`:** returns `{"status":"ok"}` (use for load balancers; readiness matches health for this single-process MVP).
- **`RUST_LOG`:** set e.g. `RUST_LOG=info` for HTTP request tracing (requires `tracing-subscriber` init in the binary).

Endpoints (v2 sketch): `POST /v2/register_geofence`, `POST /v2/register_corridor`, `POST /v2/register_catalog_region`, `POST /v2/register_radius`, `POST /v2/ingest` with body `{"updates":[...]}` (see [protocol/ndjson-v1.1.md](protocol/ndjson-v1.1.md) and `crates/adapters/http`).

## Project layout

| Path | Role |
|------|------|
| `crates/engine` | `GeoEngine`, `Engine`, batch `ingest` |
| `crates/spatial` | Point-in-polygon, `SpatialIndex`, naive index |
| `crates/state` | `EntityState`, spatial events (geofence, corridor, radius, catalog) |
| `crates/adapters/stdin-stdout` | NDJSON adapter |
| `crates/adapters/http` | Optional HTTP (`server` feature) |
| `crates/cli` | `geo-stream` / `geo-stream-http` binaries |
| `protocol/` | NDJSON spec and roadmap |
| `examples/` | Sample NDJSON / GeoJSON |
| `docker/` | Multi-stage image, CLI entrypoint |

Background and evolution: [protocol/ROADMAP.md](protocol/ROADMAP.md).

## Docker

From the repository root:

```bash
docker build -f docker/Dockerfile -t geo-stream .
docker run --rm -i geo-stream < examples/sample-input.ndjson
```
