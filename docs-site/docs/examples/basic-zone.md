---
id: basic-zone
title: Basic zone
sidebar_position: 1
description: One polygon zone, one entity, and a full enter or exit lifecycle.
---

This walkthrough covers `01-basic-zone.ts` — the simplest possible use of geo-stream: one polygon zone, one entity, enter/exit lifecycle.

## 1. Register a polygon zone

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

// Register a unit square from (0,0) to (1,1)
//
//  (0,1)------(1,1)
//    |              |
//    |              |
//  (0,0)------(1,0)
//
engine.registerZone('city-centre', {
  type: 'Polygon',
  coordinates: [
    [
      [0, 0],
      [1, 0],
      [1, 1],
      [0, 1],
      [0, 0], // close the ring
    ],
  ],
})
```

The coordinates are `[x, y]` pairs following GeoJSON convention. The engine is unit-agnostic — these could be degrees or metres.

## 2. Ingest a point inside — enter event

```typescript
const baseMs = 1_700_000_000_000

const enterEvents = engine.ingest([
  { id: 'vehicle-1', x: 0.5, y: 0.5, tMs: baseMs },
])

console.log('After moving inside the zone:')
for (const ev of enterEvents) {
  console.log(ev)
}
// { kind: 'enter', id: 'vehicle-1', zone: 'city-centre', t_ms: 1700000000000 }
```

The point `(0.5, 0.5)` is inside the unit square, so `enter` fires immediately. `t_ms` is copied from `tMs` on the update.

## 3. Move outside — exit event

```typescript
const exitEvents = engine.ingest([
  { id: 'vehicle-1', x: 5.0, y: 5.0, tMs: baseMs + 60_000 },
])

console.log('After moving outside the zone:')
for (const ev of exitEvents) {
  console.log(ev)
}
// { kind: 'exit', id: 'vehicle-1', zone: 'city-centre', t_ms: 1700000060000 }
```

The point `(5.0, 5.0)` is outside the square. Because the engine last saw `vehicle-1` inside the zone, it emits `exit`.

## 4. A second entity

```typescript
const otherEvents = engine.ingest([
  { id: 'vehicle-2', x: 0.2, y: 0.8, tMs: baseMs + 120_000 },
])

console.log('A second entity entering (independent state):')
for (const ev of otherEvents) {
  console.log(ev)
}
// { kind: 'enter', id: 'vehicle-2', zone: 'city-centre', t_ms: 1700000120000 }
```

Each entity has completely independent membership state. `vehicle-2` has never been seen before, so the engine treats `(0.2, 0.8)` as its first known position — inside the zone — and emits `enter`. The earlier movements of `vehicle-1` have no effect.

---

Next: [All zone types](./multi-zone)
