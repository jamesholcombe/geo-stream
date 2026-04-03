---
id: dwell
title: Dwell Thresholds
sidebar_position: 5
description: Suppress spurious enter/exit events from GPS noise near zone boundaries.
---

GPS receivers near zone boundaries produce noisy readings. A vehicle stopped at a warehouse loading bay can appear to cross the boundary multiple times in a few seconds, generating spurious `enter`/`exit` pairs. Without debouncing, each crossing triggers downstream workflows — notifications, billing events, dispatch assignments — incorrectly.

Dwell thresholds require an entity to remain inside or outside a zone for a minimum duration before an event fires.

## ZoneOptions

Pass dwell thresholds as the third argument to `registerZone`:

```typescript
engine.registerZone(
  'loading-bay',
  { type: 'Polygon', coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]] },
  {
    dwell: {
      minInsideMs: 5_000,   // entity must be continuously inside >= 5 s before 'enter' fires
      minOutsideMs: 3_000,  // entity must stay outside >= 3 s before 'exit' fires
    },
  },
)
```

Both fields default to `0`, which means instant transitions — the same behaviour as omitting the `dwell` option entirely.

## How each threshold works

**`minInsideMs`**: The entity must remain continuously inside the zone for at least this many milliseconds before `enter` fires. If the entity exits before the threshold is reached, no `enter` fires and the inside timer resets.

**`minOutsideMs`**: Once inside, the entity must remain continuously outside the zone for at least this many milliseconds before `exit` fires. If the entity re-enters before the threshold is reached, no `exit` fires and the outside timer resets.

## Example

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

// Entity 1: brief incursion (2 s inside — below the 5 s threshold)
// No events fire.
const briefEvents = engine.ingest([
  { id: 'van-1', x: 1.0, y: 1.0, tMs: t0 },
  { id: 'van-1', x: 5.0, y: 5.0, tMs: t0 + 2_000 }, // exits after 2 s
])
console.log(briefEvents.length) // 0

// Entity 2: sustained stay (10 s inside, then 5 s outside)
// 'enter' fires when the update at t0+5000 confirms 5 s elapsed inside.
// 'exit' fires when the update at t0+13000 confirms 3 s elapsed outside.
const sustainedEvents = engine.ingest([
  { id: 'van-2', x: 1.0, y: 1.0, tMs: t0 },
  { id: 'van-2', x: 1.0, y: 1.0, tMs: t0 + 5_000 },  // still inside at 5 s → enter fires
  { id: 'van-2', x: 5.0, y: 5.0, tMs: t0 + 10_000 }, // moves outside
  { id: 'van-2', x: 5.0, y: 5.0, tMs: t0 + 13_000 }, // still outside at 3 s → exit fires
])

for (const ev of sustainedEvents) {
  console.log(ev)
}
// { kind: 'enter', id: 'van-2', zone: 'loading-bay', t_ms: 1700000005000 }
// { kind: 'exit',  id: 'van-2', zone: 'loading-bay', t_ms: 1700000013000 }
```

`t_ms` reflects the timestamp of the update that triggered the event — not when the entity first crossed the boundary.

## Practical values

| Use case | `minInsideMs` | `minOutsideMs` |
|----------|-------------|--------------|
| Vehicle GPS (5 Hz) | 5 000 – 10 000 | 3 000 – 5 000 |
| Pedestrian GPS (1 Hz) | 2 000 – 5 000 | 1 000 – 3 000 |
| No debouncing needed | 0 (default) | 0 (default) |

:::info
Dwell thresholds apply to polygon zones only. Circles (`registerCircle`) do not support dwell options — `approach` and `recede` always fire on the first update that crosses the boundary.
:::
