---
sidebar_position: 5
---

# Dwell Thresholds

## The problem

GPS receivers near zone boundaries produce noisy readings. A vehicle stopped at a warehouse loading bay can appear to cross the boundary multiple times in a few seconds, generating spurious `enter`/`exit` pairs. Without debouncing, this can trigger downstream workflows (notifications, billing events, dispatch assignments) incorrectly.

Dwell thresholds let you require that an entity remain inside or outside a zone for a minimum duration before an event fires.

## DwellOptions

```typescript
interface DwellOptions {
  minInsideMs?: number   // ms entity must be continuously inside before 'enter' fires (default: 0)
  minOutsideMs?: number  // ms entity must be continuously outside before 'exit' fires (default: 0)
}
```

Pass `DwellOptions` as the third argument to `registerZone`:

```typescript
engine.registerZone(
  'loading-bay',
  { type: 'Polygon', coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]] },
  { minInsideMs: 5_000, minOutsideMs: 3_000 },
)
```

Both fields default to `0`, which means instant transitions (the same behaviour as omitting `DwellOptions` entirely).

## How each threshold works

**`minInsideMs`**: The entity must remain continuously inside the zone for at least this many milliseconds before `enter` fires. If the entity exits the zone before the threshold is reached, no `enter` event fires and the inside timer resets.

**`minOutsideMs`**: Once inside, the entity must remain continuously outside the zone for at least this many milliseconds before `exit` fires. If the entity re-enters the zone before the threshold is reached, no `exit` event fires and the outside timer resets.

## Example

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
    minInsideMs: 5_000,   // must dwell inside >= 5 s before 'enter' fires
    minOutsideMs: 3_000,  // must stay outside >= 3 s before 'exit' fires
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
// 'enter' fires when the update at t0+5000 confirms 5 s have elapsed inside.
// 'exit' fires when the update at t0+13000 confirms 3 s have elapsed outside.
const sustainedEvents = engine.ingest([
  { id: 'van-2', x: 1.0, y: 1.0, tMs: t0 },
  { id: 'van-2', x: 1.0, y: 1.0, tMs: t0 + 5_000 },  // still inside at 5 s → enter fires here
  { id: 'van-2', x: 5.0, y: 5.0, tMs: t0 + 10_000 }, // moves outside
  { id: 'van-2', x: 5.0, y: 5.0, tMs: t0 + 13_000 }, // still outside at 3 s → exit fires here
])

for (const ev of sustainedEvents) {
  console.log(ev)
}
// { kind: 'enter', id: 'van-2', zone: 'loading-bay', t_ms: 1700000005000 }
// { kind: 'exit',  id: 'van-2', zone: 'loading-bay', t_ms: 1700000013000 }
```

Notice that `t_ms` in each event reflects the timestamp of the update that triggered it — not when the entity first crossed the boundary.

## Practical values

| Use case | `minInsideMs` | `minOutsideMs` |
|----------|-------------|--------------|
| Vehicle GPS (5 Hz) | 5 000 – 10 000 | 3 000 – 5 000 |
| Pedestrian GPS (1 Hz) | 2 000 – 5 000 | 1 000 – 3 000 |
| No debouncing needed | 0 (default) | 0 (default) |

:::info
Dwell thresholds apply to polygon zones only. Circles (`registerCircle`) do not support `DwellOptions` — `approach` and `recede` always fire on the first update that crosses the boundary.
:::
