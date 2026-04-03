<div align="center">

# geo-stream

**Turn location streams into meaningful events**

[![npm version](https://img.shields.io/npm/v/geo-stream?style=flat-square&color=cb3837)](https://www.npmjs.com/package/@jamesholcombe/geo-stream)
[![CI](https://img.shields.io/github/actions/workflow/status/jamesholcombe/geo-stream/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/jamesholcombe/geo-stream/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Node.js](https://img.shields.io/badge/node-%3E%3D18-brightgreen?style=flat-square)](https://nodejs.org)

</div>

---

An embeddable geospatial stream processor. Feed it location updates; receive structured spatial events — enter/exit zones, approach/recede circles, assignment changes. Runs in-process with no external dependencies.

**[Documentation →](https://jamesholcombe.github.io/geo-stream/)**

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
