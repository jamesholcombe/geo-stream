---
id: dwell-debounce
title: Dwell debounce
sidebar_position: 3
description: Use dwell thresholds to reduce spurious enter, exit, approach, and recede events.
---

This walkthrough shows how dwell thresholds suppress spurious events from boundary oscillation — for both polygon zones and circles.

See [Dwell Thresholds](../dwell) for the full explanation of how the thresholds work.

## 1. The problem

A vehicle parked near a boundary can generate readings alternating inside and outside. Without debouncing, each crossing fires an event — you might see dozens of `enter`/`exit` or `approach`/`recede` pairs in seconds for a stationary vehicle.

Dwell thresholds solve this by requiring the entity to remain inside (or outside) continuously for a minimum duration before any event fires.

## 2. Zone with dwell options

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

engine.registerZone(
  'loading-bay',
  {
    type: 'Polygon',
    coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]],
  },
  {
    dwell: {
      minInsideMs: 5_000,   // must dwell inside >= 5 s before 'enter' fires
      minOutsideMs: 3_000,  // must stay outside >= 3 s before 'exit' fires
    },
  },
)

const t0 = 1_700_000_000_000
```

## 3. Entity 1: brief incursion — no events

```typescript
// van-1 enters at t0, exits at t0+2000 (2 s inside — below the 5 s threshold)
const briefEvents = engine.ingest([
  { id: 'van-1', x: 1.0, y: 1.0, tMs: t0 },
  { id: 'van-1', x: 5.0, y: 5.0, tMs: t0 + 2_000 },
])

console.log(briefEvents.length) // 0
```

`van-1` is inside the zone for only 2 000 ms. Since `minInsideMs` is 5 000, the engine never reaches the threshold and no `enter` fires. When `van-1` exits at `t0 + 2000`, the inside timer resets. No `exit` fires either, because there was never an `enter`.

## 4. Entity 2: sustained stay — enter and exit both fire

```typescript
const sustainedEvents = engine.ingest([
  { id: 'van-2', x: 1.0, y: 1.0, tMs: t0 },          // enters zone
  { id: 'van-2', x: 1.0, y: 1.0, tMs: t0 + 5_000 },  // still inside at 5 s → enter fires
  { id: 'van-2', x: 5.0, y: 5.0, tMs: t0 + 10_000 }, // exits zone
  { id: 'van-2', x: 5.0, y: 5.0, tMs: t0 + 13_000 }, // still outside at 3 s → exit fires
])

for (const ev of sustainedEvents) {
  console.log(ev)
}
// { kind: 'enter', id: 'van-2', zone: 'loading-bay', t_ms: 1700000005000 }
// { kind: 'exit',  id: 'van-2', zone: 'loading-bay', t_ms: 1700000013000 }
```

The `enter` fires when the update at `t0 + 5_000` confirms that `van-2` has been continuously inside for at least 5 000 ms. The `exit` fires when the update at `t0 + 13_000` confirms 3 000 ms continuously outside.

## 5. Circle with dwell options

The same thresholds apply to circles — pass them as the fifth argument to `registerCircle`. The events are `approach` (entry) and `recede` (exit) rather than `enter`/`exit`, but the debounce behaviour is identical.

```typescript
engine.registerCircle(
  'depot-beacon',
  7, 7, 1.5,
  {
    dwell: {
      minInsideMs: 4_000,   // must be inside >= 4 s before 'approach' fires
      minOutsideMs: 2_000,  // must be outside >= 2 s before 'recede' fires
    },
  },
)
```

```typescript
// truck-1 briefly enters and exits — no events
const noEvents = engine.ingest([
  { id: 'truck-1', x: 7.0, y: 7.0, tMs: t0 },          // inside circle
  { id: 'truck-1', x: 20.0, y: 20.0, tMs: t0 + 1_000 }, // exits after 1 s (below 4 s threshold)
])
console.log(noEvents.length) // 0

// truck-2 stays inside long enough, then leaves long enough
const circleEvents = engine.ingest([
  { id: 'truck-2', x: 7.0, y: 7.0, tMs: t0 },
  { id: 'truck-2', x: 7.0, y: 7.0, tMs: t0 + 4_000 },   // inside >= 4 s → approach fires
  { id: 'truck-2', x: 20.0, y: 20.0, tMs: t0 + 8_000 },  // exits
  { id: 'truck-2', x: 20.0, y: 20.0, tMs: t0 + 10_000 }, // outside >= 2 s → recede fires
])

for (const ev of circleEvents) {
  console.log(ev)
}
// { kind: 'approach', id: 'truck-2', circle: 'depot-beacon', t_ms: 1700000004000 }
// { kind: 'recede',   id: 'truck-2', circle: 'depot-beacon', t_ms: 1700000010000 }
```

---

For a full description of threshold fields and practical values, see [Dwell Thresholds](../dwell).
