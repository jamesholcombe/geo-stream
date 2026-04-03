---
id: zone-types
title: Zone Types
sidebar_position: 3
description: Polygon zones, circles, and catalog regions — the three zone types and the events they emit.
---

geo-stream has three zone types. Each emits a distinct pair of events and is registered once at setup time.

All registration methods return `this`, so they chain:

```typescript
const engine = new GeoEngine()
  .registerZone('warehouse', warehousePolygon)
  .registerCircle('depot-beacon', 7, 7, 1.5)
  .registerCatalogRegion('district-north', northPolygon)
```

## Polygon Zones

```typescript
engine.registerZone(id: string, polygon: GeoJsonPolygonInput, options?: ZoneOptions): this
```

Polygon zones emit `enter` when an entity moves inside, and `exit` when it moves outside.

**Accepted shapes** (from `@types/geojson`):

- `Polygon`
- `MultiPolygon`
- `Feature<Polygon>`
- `Feature<MultiPolygon>`

Polygon holes (interior rings) are fully supported — a point inside a hole is considered outside the zone.

An optional `dwell` threshold in `ZoneOptions` suppresses spurious boundary crossings from GPS noise. See [Dwell Thresholds](./dwell) for details.

```typescript
interface ZoneOptions {
  dwell?: {
    minInsideMs?: number   // ms entity must be continuously inside before 'enter' fires
    minOutsideMs?: number  // ms entity must be continuously outside before 'exit' fires
  }
}
```

**Example:**

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

engine.registerZone('city-centre', {
  type: 'Polygon',
  coordinates: [
    // (0,1)---(1,1)
    //   |         |
    // (0,0)---(1,0)
    [[0, 0], [1, 0], [1, 1], [0, 1], [0, 0]],
  ],
})

const events = engine.ingest([
  { id: 'vehicle-1', x: 0.5, y: 0.5, tMs: 1_700_000_000_000 },
])
// [{ kind: 'enter', id: 'vehicle-1', zone: 'city-centre', t_ms: 1700000000000 }]
```

## Catalog Regions

```typescript
engine.registerCatalogRegion(id: string, polygon: GeoJsonPolygonInput): this
```

Catalog regions represent mutually exclusive named areas — delivery zones, service territories, districts. The engine emits `assignment_changed` when an entity's containing region changes.

An entity is assigned to **at most one region** at a time — the lexicographically smallest matching ID when regions overlap. `assignment_changed` fires when the entity:

- Moves from one region into a different region
- Enters a region from outside all regions
- Leaves all regions (emitted with `region: null`)

:::caution
If regions overlap, the one with the lexicographically smallest ID wins. Design your regions to be non-overlapping to avoid unexpected assignments.
:::

**Example:**

```typescript
engine.registerCatalogRegion('district-north', {
  type: 'Polygon',
  coordinates: [[[0, 5], [10, 5], [10, 10], [0, 10], [0, 5]]],
})
engine.registerCatalogRegion('district-south', {
  type: 'Polygon',
  coordinates: [[[0, 0], [10, 0], [10, 5], [0, 5], [0, 0]]],
})

const t0 = 1_700_000_000_000

engine.ingest([{ id: 'truck-1', x: 5, y: 2, tMs: t0 }])
// [{ kind: 'assignment_changed', id: 'truck-1', region: 'district-south', t_ms: ... }]

engine.ingest([{ id: 'truck-1', x: 5, y: 8, tMs: t0 + 30_000 }])
// [{ kind: 'assignment_changed', id: 'truck-1', region: 'district-north', t_ms: ... }]

engine.ingest([{ id: 'truck-1', x: 50, y: 50, tMs: t0 + 60_000 }])
// [{ kind: 'assignment_changed', id: 'truck-1', region: null, t_ms: ... }]
```

## Circles

```typescript
engine.registerCircle(id: string, cx: number, cy: number, r: number): this
```

Circles emit `approach` when an entity enters the radius, and `recede` when it exits.

Containment uses Euclidean distance: an entity at `(x, y)` is inside the circle when `sqrt((x - cx)² + (y - cy)²) <= r`.

:::caution
This is Euclidean distance, not geodesic. If your coordinates are WGS-84 longitude/latitude, `r` is measured in degrees, which is not constant across latitudes. For metre-accurate radius checks, use a projected coordinate system such as a local UTM zone.
:::

**Example:**

```typescript
// Circle centred at (7, 7) with radius 1.5
engine.registerCircle('depot-beacon', 7, 7, 1.5)

const t0 = 1_700_000_000_000

engine.ingest([{ id: 'truck-1', x: 7.0, y: 7.0, tMs: t0 }])
// [{ kind: 'approach', id: 'truck-1', circle: 'depot-beacon', t_ms: ... }]

engine.ingest([{ id: 'truck-1', x: 20, y: 20, tMs: t0 + 30_000 }])
// [{ kind: 'recede', id: 'truck-1', circle: 'depot-beacon', t_ms: ... }]
```
