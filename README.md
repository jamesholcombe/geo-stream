<div align="center">

# geo-stream

**In-memory geospatial stream processor — location updates in, spatial events out.**

[![npm version](https://img.shields.io/npm/v/geo-stream?style=flat-square&color=cb3837)](https://www.npmjs.com/package/geo-stream)
[![CI](https://img.shields.io/github/actions/workflow/status/jamesholcombe/geo-stream/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/jamesholcombe/geo-stream/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Node.js](https://img.shields.io/badge/node-%3E%3D18-brightgreen?style=flat-square)](https://nodejs.org)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey?style=flat-square)](#platform-support)

</div>

---

Feed `geo-stream` a stream of `{ id, x, y, t_ms }` location updates and it emits structured spatial events — enter/exit zones, approach/recede circles, and catalog region assignment changes. The engine is a zero-copy Rust core exposed as a native Node.js module via NAPI, with no runtime dependencies.

```
location update → ┌──────────────────┐ → enter / exit
                  │   geo-stream     │ → approach / recede
location update → │   engine         │ → assignment_changed
                  │   (Rust + NAPI)  │ 
location update → └──────────────────┘
```

## Features

- **Four zone types** — polygon zones,  circles, and catalog regions
- **Dwell / debounce** — configurable `minInsideMs` / `minOutsideMs` thresholds per zone
- **Polygon holes** — GeoJSON polygons with interior rings are supported natively
- **Typed events** — discriminated union `GeoEvent` with full TypeScript inference
- **Native performance** — Rust R-tree spatial index; no JS overhead on the hot path
- **Embeddable** — use as a Node.js package, a Rust crate, or an NDJSON CLI

---

## Installation

```bash
npm install @jamesholcombe/geo-stream
```

Pre-built native binaries are distributed for all supported platforms — no Rust toolchain required.

### Platform support

| Platform | Architecture | Status |
|----------|-------------|--------|
| macOS | arm64 (Apple Silicon) | ✅ |
| macOS | x64 (Intel) | ✅ |
| Linux (glibc) | x64 | ✅ |
| Linux (glibc) | arm64 | ✅ |
| Windows | x64 | ✅ |

---

## Quick start

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

// Register a polygon zone (GeoJSON)
engine.registerZone('city-centre', {
  type: 'Polygon',
  coordinates: [[[0, 0], [1, 0], [1, 1], [0, 1], [0, 0]]],
})

// Ingest location updates
const events = engine.ingest([
  { id: 'vehicle-1', x: 0.5, y: 0.5, tMs: Date.now() },
])

console.log(events)
// [{ kind: 'enter', id: 'vehicle-1', zone: 'city-centre', t_ms: 1700000000000 }]
```

---

## API

### `new GeoEngine()`

Creates a new, empty engine instance. Each instance tracks its own set of zones and entity states independently.

---

### Zone registration

#### `registerZone(id, polygon, dwell?)`

Register a named zone from a GeoJSON `Polygon` object. Fires `enter` / `exit` events.

```typescript
// Basic
engine.registerZone('warehouse', polygon)

// With dwell thresholds (debounce boundary hover)
engine.registerZone('warehouse', polygon, {
  minInsideMs: 5_000,   // must be inside ≥ 5 s before 'enter' fires
  minOutsideMs: 3_000,  // must be outside ≥ 3 s before 'exit' fires
})
```

#### `registerCatalogRegion(id, polygon)`

Register a catalog region. Fires `assignment_changed` whenever an entity's current containing region changes, including when it leaves all regions (`region: null`).

#### `registerCircle(id, cx, cy, radius)`

Register a circular zone by centre point and radius (in the same coordinate units as your location data). Fires `approach` / `recede`.

```typescript
engine.registerCircle('depot', 51.5074, -0.1278, 0.05)
```

---

### `ingest(updates)`

Process a batch of location updates. Returns all spatial events produced by the batch as a typed `GeoEvent[]`.

```typescript
const events = engine.ingest([
  { id: 'vehicle-1', x: 0.50, y: 0.50, tMs: 1_700_000_000_000 },
  { id: 'vehicle-2', x: 5.00, y: 5.00, tMs: 1_700_000_000_000 },
])
```

Updates within a batch are sorted by `(id, tMs)` before processing, so order within a batch does not matter.

---

## Event types

All events are a discriminated union on `kind`. Switch exhaustively for compile-time completeness guarantees:

```typescript
type GeoEvent =
  | { kind: 'enter';              id: string; zone: string;      t_ms: number }
  | { kind: 'exit';               id: string; zone: string;      t_ms: number }
  | { kind: 'approach';           id: string; zone: string;          t_ms: number }
  | { kind: 'recede';             id: string; zone: string;          t_ms: number }
  | { kind: 'assignment_changed'; id: string; region: string | null; t_ms: number }
```

| `kind` | Trigger | Key field |
|--------|---------|-----------|
| `enter` | Entity enters a zone | `zone` |
| `exit` | Entity exits a zone | `zone` |
| `approach` | Entity enters a circle | `zone` |
| `recede` | Entity exits a circle | `zone` |
| `assignment_changed` | Entity's catalog region changes | `region` (`null` = unassigned) |

---

## Examples

Working examples are in [`examples/typescript/`](examples/typescript/):

| File | What it shows |
|------|---------------|
| [`01-basic-zone.ts`](examples/typescript/01-basic-zone.ts) | Register a polygon, ingest points, observe enter/exit events |
| [`02-multi-zone.ts`](examples/typescript/02-multi-zone.ts) | All three zone types — zone, catalog, circle — in one script |
| [`03-dwell.ts`](examples/typescript/03-dwell.ts) | Dwell thresholds to debounce boundary hover |

```bash
cd examples/typescript
npm install
npx ts-node 01-basic-zone.ts
```

---

## Other interfaces

<details>
<summary><strong>CLI (NDJSON over stdin/stdout)</strong></summary>

The `geo-stream` binary reads newline-delimited JSON from stdin and writes events to stdout.

```bash
cargo run -p cli --bin geo-stream -- < examples/sample-input.ndjson
```

```json
{"event":"enter","id":"c1","zone":"zone-1","t":1700000000000}
{"event":"exit","id":"c1","zone":"zone-1","t":1700000060000}
```

**Input shapes:**

```jsonc
// Register a zone
{"type":"register_zone","id":"zone-1","polygon":{...GeoJSON Polygon...}}

// Point update
{"type":"update","id":"c1","location":[x,y],"t":1700000000000}
```

**Batching:**

```bash
# Buffer all stdin, then one process_batch call
cargo run -p cli --bin geo-stream -- --batch-size 0 < examples/sample-input.ndjson
```

Full protocol spec: [`protocol/ndjson.md`](protocol/ndjson.md).

</details>

<details>
<summary><strong>Rust crate</strong></summary>

```rust
use engine::{Engine, GeoEngine};
use state::PointUpdate;

let mut engine = Engine::default();
engine.register_zone("zone-1", polygon)?;

let events = engine.process_event(PointUpdate {
    id: "c1".into(),
    x: 0.5,
    y: 0.5,
    t_ms: 1_700_000_000_000,
});
```

```bash
cargo build
cargo test
cargo bench -p engine    # Criterion benchmarks → target/criterion/
```

</details>

<details>
<summary><strong>Docker</strong></summary>

```bash
docker build -f docker/Dockerfile -t geo-stream .
docker run --rm -i geo-stream < examples/sample-input.ndjson
```

</details>

---

## Project layout

| Path | Role |
|------|------|
| `crates/adapters/napi` | TypeScript / Node.js NAPI bindings (`geo-stream` npm package) |
| `crates/engine` | `GeoEngine` trait, `Engine`, `SpatialRule` pipeline |
| `crates/spatial` | Point-in-polygon, `SpatialIndex`, R-tree |
| `crates/state` | `EntityState`, `Event` enum |
| `crates/adapters/stdin-stdout` | NDJSON CLI adapter |
| `crates/cli` | `geo-stream` binary |
| `protocol/` | NDJSON wire spec and JSON Schema |
| `examples/` | Sample NDJSON, GeoJSON, and TypeScript scripts |

Architecture, invariants, and roadmap: [ROADMAP.md](ROADMAP.md).

---

## Building the native module from source

Requires a Rust toolchain ([rustup.rs](https://rustup.rs)) and Node.js 18+.

```bash
make napi-build           # debug (fast iteration)
make napi-build-release   # optimised release
```

---

## License

[MIT](LICENSE)
