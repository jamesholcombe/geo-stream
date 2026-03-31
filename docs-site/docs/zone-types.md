---
sidebar_position: 3
---

# Zone Types

geo-stream supports three zone types, each emitting a distinct set of events.

## Polygon Zones

```typescript
engine.registerZone(id: string, polygon: GeoJsonPolygonInput, dwell?: DwellOptions): void
```

Polygon zones emit `enter` when an entity moves inside, and `exit` when it moves outside.

**Accepted shapes** (from `@types/geojson`):

- `Polygon`
- `MultiPolygon`
- `Feature<Polygon>`
- `Feature<MultiPolygon>`

Polygon holes (interior rings) are fully supported — a point inside a hole is considered outside the zone.

An optional `dwell` parameter lets you suppress spurious boundary crossings. See [Dwell Thresholds](./dwell) for details.

**Example:**

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream/types'

const engine = new GeoEngine()

// Register a square zone from (0,0) to (1,1)
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
engine.registerCatalogRegion(id: string, polygon: GeoJsonPolygonInput): void
```

Catalog regions represent mutually exclusive named areas such as districts, territories, or delivery zones. The engine emits `assignment_changed` when an entity's containing region changes.

An entity is always assigned to **at most one region** — the lexicographically smallest matching ID when regions overlap. `assignment_changed` fires when the entity:

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

// Entity starts in district-south
engine.ingest([{ id: 'truck-1', x: 5, y: 2, tMs: t0 }])
// [{ kind: 'assignment_changed', id: 'truck-1', region: 'district-south', t_ms: ... }]

// Entity crosses into district-north
engine.ingest([{ id: 'truck-1', x: 5, y: 8, tMs: t0 + 30_000 }])
// [{ kind: 'assignment_changed', id: 'truck-1', region: 'district-north', t_ms: ... }]

// Entity leaves all regions
engine.ingest([{ id: 'truck-1', x: 50, y: 50, tMs: t0 + 60_000 }])
// [{ kind: 'assignment_changed', id: 'truck-1', region: null, t_ms: ... }]
```

## Circles

```typescript
engine.registerCircle(id: string, cx: number, cy: number, r: number): void
```

Circles emit `approach` when an entity enters the radius, and `recede` when it exits.

Containment is tested using Euclidean distance: an entity at `(x, y)` is inside the circle when `sqrt((x - cx)² + (y - cy)²) <= r`.

:::caution
This is Euclidean distance, not geodesic. If your coordinates are WGS-84 longitude/latitude, `r` is measured in degrees, which is not constant across latitudes. For metre-accurate radius checks, use a projected coordinate system (such as a local UTM zone).
:::

**Example:**

```typescript
// Circle centred at (7, 7) with radius 1.5
engine.registerCircle('depot-beacon', 7, 7, 1.5)

const t0 = 1_700_000_000_000

// Entity moves into the circle
engine.ingest([{ id: 'truck-1', x: 7.0, y: 7.0, tMs: t0 }])
// [{ kind: 'approach', id: 'truck-1', circle: 'depot-beacon', t_ms: ... }]

// Entity moves outside
engine.ingest([{ id: 'truck-1', x: 20, y: 20, tMs: t0 + 30_000 }])
// [{ kind: 'recede', id: 'truck-1', circle: 'depot-beacon', t_ms: ... }]
```
