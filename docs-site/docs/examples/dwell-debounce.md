---
sidebar_position: 3
---

# Dwell Debounce

This walkthrough covers `03-dwell.ts`, which shows how dwell thresholds suppress spurious events from boundary oscillation.

See [Dwell Thresholds](../dwell) for the full explanation of how the thresholds work.

## 1. The problem

A vehicle parked near a zone boundary can generate readings alternating inside and outside the zone. Without debouncing, each crossing fires an event — you might see dozens of `enter`/`exit` pairs in seconds for a stationary vehicle.

Dwell thresholds solve this by requiring the entity to remain inside (or outside) continuously for a minimum duration before any event fires.

## 2. Zone with dwell options

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream/types'

const engine = new GeoEngine()

engine.registerZone(
  'loading-bay',
  {
    type: 'Polygon',
    coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]],
  },
  {
    minInsideMs: 5_000,   // must dwell inside >= 5 s before "enter" fires
    minOutsideMs: 3_000,  // must stay outside >= 3 s before "exit" fires
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

`van-1` is inside the zone for only 2 000 ms. Since `minInsideMs` is 5 000, the engine never reaches the threshold and no `enter` fires. When `van-1` exits at `t0 + 2000`, the inside timer is reset. No `exit` fires either, because there was never an `enter`.

## 4. Entity 2: sustained stay — enter and exit both fire

```typescript
const sustainedEvents = engine.ingest([
  { id: 'van-2', x: 1.0, y: 1.0, tMs: t0 },          // enters zone
  { id: 'van-2', x: 1.0, y: 1.0, tMs: t0 + 5_000 },  // still inside at 5 s → enter fires at t_ms=1700000005000
  { id: 'van-2', x: 5.0, y: 5.0, tMs: t0 + 10_000 }, // exits zone
  { id: 'van-2', x: 5.0, y: 5.0, tMs: t0 + 13_000 }, // still outside at 3 s → exit fires at t_ms=1700000013000
])

for (const ev of sustainedEvents) {
  console.log(ev)
}
// { kind: 'enter', id: 'van-2', zone: 'loading-bay', t_ms: 1700000005000 }
// { kind: 'exit',  id: 'van-2', zone: 'loading-bay', t_ms: 1700000013000 }
```

The `enter` fires when the update at `t0 + 5_000` confirms that `van-2` has been continuously inside for at least 5 000 ms. The `t_ms` of the event is that update's timestamp.

The `exit` fires when the update at `t0 + 13_000` confirms that `van-2` has been continuously outside for at least 3 000 ms (it exited at `t0 + 10_000`, and `t0 + 13_000 - t0 + 10_000 = 3_000`).

---

For a full description of `DwellOptions` fields and practical threshold values, see [Dwell Thresholds](../dwell).
