<div align="center">

# geo-stream

**An embeddable rules engine for location streams**

[![npm version](https://img.shields.io/npm/v/geo-stream?style=flat-square&color=cb3837)](https://www.npmjs.com/package/@jamesholcombe/geo-stream)
[![CI](https://img.shields.io/github/actions/workflow/status/jamesholcombe/geo-stream/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/jamesholcombe/geo-stream/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Node.js](https://img.shields.io/badge/node-%3E%3D18-brightgreen?style=flat-square)](https://nodejs.org)

</div>

---

Drop it into your Node.js or Rust service. Feed it location updates; receive typed spatial events — `enter`/`exit` zones, `approach`/`recede` circles, custom rule triggers, sequence completions. No server to run, no network round-trips, no schema to migrate.

**[Documentation →](https://jamesholcombe.github.io/geo-stream/)**

---

### Why in-process?

External geofencing services (database-backed or network-protocol-based) require a round-trip per position update and fire events asynchronously with no delivery or ordering guarantees. geo-stream runs inside your process:

- **Zero latency** — no network hop between position update and event
- **Deterministic** — same inputs always produce the same events in the same order; replay and testing are exact
- **Dwell/debounce built in** — suppress noisy enter/exit cycling without external state
- **Sequence rules** — detect multi-step patterns (entity enters zone A, then zone B, within N ms) that an external service cannot represent
- **Speed and heading filters** — emit custom events only when an entity is above speed or travelling in a direction

> **Coordinate system note:** geo-stream currently operates on a flat Euclidean plane. GPS lat/lng coordinates can be used, but distance-based operations (circles, `entities_near_point`) produce approximate results at scale. WGS84/geodesic mode is on the [roadmap](ROADMAP.md).

---

## Project layout

| Path | Role |
|------|------|
| `geo-stream/` | npm package — `GeoEngine` wrapper, TypeScript adapters (EventEmitter, Kafka, Redis) |
| `crates/adapters/napi/` | Rust NAPI bindings compiled into the npm package |
| `crates/engine/` | `GeoEngine` trait, `Engine`, `SpatialRule` pipeline |
| `crates/spatial/` | Point-in-polygon, `SpatialIndex`, R-tree |
| `crates/state/` | `EntityState`, `Event` enum |
| `crates/adapters/stdin-stdout/` | NDJSON CLI adapter |
| `crates/cli/` | `geo-stream` binary |
| `docs-site/` | Docusaurus documentation site |
| `protocol/` | NDJSON wire spec and JSON Schema |
| `examples/` | Sample NDJSON and GeoJSON files |

Architecture, invariants, and roadmap: [ROADMAP.md](ROADMAP.md)

---

## Building and testing

```bash
cargo build                          # debug build
cargo test                           # all workspace tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
make run                             # pipe examples/sample-input.ndjson through the CLI
```

Building the native Node.js module (requires Rust toolchain):

```bash
make napi-build           # debug
make napi-build-release   # optimised release
```

---

## License

[MIT](LICENSE)
