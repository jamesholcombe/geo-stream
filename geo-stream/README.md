# @jamesholcombe/geo-stream

Native Node.js bindings for the **geo-stream** geospatial stream processor. Feed it location updates; receive structured spatial events — enter/exit zones, approach/recede circles, assignment changes. Runs in-process with no external dependencies.

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
