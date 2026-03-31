---
slug: /
sidebar_position: 1
---

# Introduction

geo-stream turns a stream of `{ id, x, y, tMs }` location updates into structured spatial events — in-process, with no external dependencies.

Hand-rolling geofencing requires tracking membership state per entity and per zone, debouncing boundary noise so a vehicle oscillating near a fence doesn't flood you with spurious enter/exit pairs, and ensuring deterministic event ordering across heterogeneous zone types. geo-stream handles all of this. You register zones once, call `ingest()` with batches of location updates, and receive typed events.

## Quick start

```bash
npm install @jamesholcombe/geo-stream
```

```typescript
import { GeoEngine, GeoEvent } from '@jamesholcombe/geo-stream/types'

const engine = new GeoEngine()

// Register a polygon zone
engine.registerZone('city-centre', {
  type: 'Polygon',
  coordinates: [[[0, 0], [1, 0], [1, 1], [0, 1], [0, 0]]],
})

// Ingest a location update
const events = engine.ingest([
  { id: 'vehicle-1', x: 0.5, y: 0.5, tMs: Date.now() },
])

// Handle events
for (const ev of events) {
  switch (ev.kind) {
    case 'enter':
      console.log(`${ev.id} entered zone ${ev.zone}`)
      // vehicle-1 entered zone city-centre
      break
    case 'exit':
      console.log(`${ev.id} left zone ${ev.zone}`)
      break
  }
}
```

## Zone types

| Zone type | Registration method | Events emitted |
|-----------|--------------------|--------------:|
| Polygon zone | `registerZone` | `enter` + `exit` |
| Catalog region | `registerCatalogRegion` | `assignment_changed` |
| Circle | `registerCircle` | `approach` + `recede` |

## What it is not

geo-stream is not a GIS platform, a spatial database, or a map visualisation tool. It is an embeddable stream processor: you call it directly from Node.js, it holds state in memory, and it returns typed events synchronously. There is no server to run, no schema to migrate, and no network calls.
