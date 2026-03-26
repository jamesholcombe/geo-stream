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

`cargo test -p cli` also runs NDJSON integration tests (fixtures under `crates/cli/tests/fixtures/`).

**Benchmarks** (Criterion, engine `process_batch`): `cargo bench -p engine` or `make bench`. Results and HTML plots land under `target/criterion/` when you run the full bench (omit `--no-run`).

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

Full contract: [protocol/ndjson.md](protocol/ndjson.md). Example with corridors, catalog regions, and radius: [`examples/sample-zones.ndjson`](examples/sample-zones.ndjson).

### Batching

```bash
cargo run -p cli --bin geo-stream -- --batch-size 0 -- < examples/sample-input.ndjson
```

- `--batch-size 1` (default): one `update` line → one engine batch.
- `--batch-size N` (`N > 1`): buffer `N` updates, then `process_batch`.
- `--batch-size 0`: buffer all updates until EOF, then one `process_batch`.

Fence registration lines are always applied immediately; they are not batched.

### Optional HTTP adapter

Build the Axum-based binary (same engine, JSON over HTTP):

```bash
cargo build -p cli --features http --bin geo-stream-http
./target/debug/geo-stream-http --listen 0.0.0.0:8080
```

- **`--listen`:** bind address (default `0.0.0.0:8080`).
- **`GET /health`:** returns `{"status":"ok"}` (use for load balancers; readiness matches health for this single-process MVP).
- **`GET /openapi.json`:** OpenAPI 3 document describing all HTTP routes and JSON shapes.
- **Errors:** failed requests return JSON `{"error":{"code":"<stable code>","message":"..."}}` (for example `invalid_json`, `invalid_input`, `conflict`, `internal_error`) with an appropriate HTTP status.
- **`RUST_LOG`:** set e.g. `RUST_LOG=info` for HTTP request tracing (requires `tracing-subscriber` init in the binary).

HTTP routes: `POST /v1/register_geofence`, `POST /v1/register_corridor`, `POST /v1/register_catalog_region`, `POST /v1/register_radius`, `POST /v1/ingest` with body `{"updates":[...]}` (see [protocol/ndjson.md](protocol/ndjson.md#http-adapter-optional) and `crates/adapters/http`).

## Project layout

| Path | Role |
|------|------|
| `crates/engine` | `GeoEngine`, `Engine`, `process_event`, `process_batch`, `SpatialRule` |
| `crates/spatial` | Point-in-polygon, `SpatialIndex`, R-tree (`NaiveSpatialIndex`) |
| `crates/state` | `EntityState`, spatial events (geofence, corridor, radius, catalog) |
| `crates/adapters/stdin-stdout` | NDJSON adapter |
| `crates/adapters/http` | Optional HTTP (`server` feature) |
| `crates/cli` | `geo-stream` / `geo-stream-http` binaries |
| `protocol/` | NDJSON spec and roadmap |
| `examples/` | Sample NDJSON / GeoJSON |
| `docker/` | Multi-stage image, CLI entrypoint |

Background, evolution, and planned features: [ROADMAP.md](ROADMAP.md).

## Docker

From the repository root:

```bash
docker build -f docker/Dockerfile -t geo-stream .
docker run --rm -i geo-stream < examples/sample-input.ndjson
```
