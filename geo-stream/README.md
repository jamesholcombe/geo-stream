# @jamesholcombe/geo-stream

An embeddable rules engine for location streams. Feed it position updates; receive typed spatial events — `enter`/`exit` zones, `approach`/`recede` circles, custom rule triggers, sequence completions. Runs inside your Node.js process with no server, no network round-trips, and no external dependencies.

**When to use this instead of an external geofencing service:** geo-stream is the right choice when you need deterministic event ordering (same inputs always produce the same events in the same order), built-in dwell/debounce to suppress noisy boundary crossings, multi-step sequence detection across a series of zones, or speed and heading filters on event conditions. These behaviours require external state if you are using a network-based service.

> **Coordinate system:** geo-stream currently operates on a flat Euclidean plane. It works correctly with any consistent unit (metres, degrees, etc.) as long as you do not mix units or expect accurate distance calculations across large WGS84 extents. WGS84/geodesic support is planned — see the [roadmap](https://github.com/jamesholcombe/geo-events/blob/main/ROADMAP.md).

## Install

```bash
npm install @jamesholcombe/geo-stream
```

Pre-built native binaries are included for macOS (arm64, x64), Linux (x64, arm64), and Windows (x64). No Rust toolchain required.

## Quick start

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

engine.registerZone('warehouse', {
  type: 'Polygon',
  coordinates: [[[0, 0], [10, 0], [10, 10], [0, 10], [0, 0]]],
})

const events = engine.ingest([
  { id: 'driver-1', x: 5, y: 5, tMs: Date.now() },
])

for (const ev of events) {
  console.log(ev)
  // { kind: 'enter', id: 'driver-1', zone: 'warehouse', t_ms: ... }
}
```

## Documentation

Full documentation, concepts, API reference, and examples:
**[jamesholcombe.github.io/geo-stream](https://jamesholcombe.github.io/geo-stream/)**

## Node.js requirement

Node.js 18 or later.
