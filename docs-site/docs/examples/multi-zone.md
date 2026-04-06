---
id: multi-zone
title: All zone types
sidebar_position: 2
description: Polygon zones, catalog regions, and circles registered on a single engine.
---

This walkthrough covers `02-multi-zone.ts` — all three zone types registered on a single engine, with one entity moving through each.

## 1. Register all three zone types

```typescript
import { GeoEngine, type GeoEvent } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

// Polygon zone: rectangular area from (0,0) to (2,2)
engine.registerZone('warehouse', {
  type: 'Polygon',
  coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]],
})

// Catalog regions: mutually exclusive named areas
// district-south covers y: 0–5, district-north covers y: 5–10
engine.registerCatalogRegion('district-north', {
  type: 'Polygon',
  coordinates: [[[0, 5], [10, 5], [10, 10], [0, 10], [0, 5]]],
})
engine.registerCatalogRegion('district-south', {
  type: 'Polygon',
  coordinates: [[[0, 0], [10, 0], [10, 5], [0, 5], [0, 0]]],
})

// Circle: centred at (7, 7), radius 1.5 units
engine.registerCircle('depot-beacon', 7, 7, 1.5)
```

All three types coexist on the same engine instance. `ingest()` evaluates every registered zone for every update.

## 2. Waypoint sequence

`truck-1` moves through five positions, each triggering different events:

| Time | Position | Location | Events |
|------|----------|----------|--------|
| t+0s | (1.0, 1.0) | inside warehouse (district-south) | `enter` warehouse, `assignment_changed` → district-south |
| t+30s | (4.0, 0.25) | open road (district-south) | `exit` warehouse |
| t+60s | (6.0, 6.0) | in district-north | `assignment_changed` → district-north |
| t+90s | (7.0, 7.0) | inside depot-beacon (district-north) | `approach` depot-beacon |
| t+120s | (20.0, 20.0) | open space (no zones) | `recede` depot-beacon, `assignment_changed` → null |

```typescript
const t0 = 1_700_000_000_000

const waypoints = [
  { x: 1.0,  y: 1.0,  label: 'inside warehouse (district-south)' },
  { x: 4.0,  y: 0.25, label: 'open road (district-south)'        },
  { x: 6.0,  y: 6.0,  label: 'in district-north'                 },
  { x: 7.0,  y: 7.0,  label: 'inside depot-beacon (district-north)' },
  { x: 20.0, y: 20.0, label: 'open space (no zones)'             },
]

for (let i = 0; i < waypoints.length; i++) {
  const { x, y, label } = waypoints[i]
  const tMs = t0 + i * 30_000

  const events = engine.ingest([{ id: 'truck-1', x, y, tMs }])

  console.log(`\n[t+${i * 30}s]  pos=(${x}, ${y})  — ${label}`)
  for (const ev of events) {
    console.log(' ', formatEvent(ev))
  }
}
```

## 3. The event handler switch

```typescript
function formatEvent(ev: GeoEvent): string {
  switch (ev.kind) {
    case 'enter':
      return `ENTER zone "${ev.zone}"`
    case 'exit':
      return `EXIT  zone "${ev.zone}"`
    case 'approach':
      return `APPROACH circle "${ev.circle}"`
    case 'recede':
      return `RECEDE   circle "${ev.circle}"`
    case 'assignment_changed':
      return `ASSIGNED → catalog region "${ev.region ?? 'none (unassigned)'}"`
  }
}
```

This `switch` is exhaustive: TypeScript will produce a compile error if a new `kind` is added to `GeoEvent` and this function is not updated. The pattern is recommended over `if/else` chains or `ev.kind === '...'` checks.

---

Next: [Dwell debounce](./dwell-debounce)
