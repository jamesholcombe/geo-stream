---
id: querying
title: Querying entities
sidebar_position: 5
description: Read current entity state by zone membership, circle membership, region, or proximity to a point.
---

Between ingests you can query the engine's in-memory state directly — no events required. Queries come in two shapes: membership lookups that filter entities by the zones or regions they are currently inside, and spatial queries that find entities by their distance from a point.

## Membership queries

### By zone

```typescript
engine.entitiesInZone(zoneId: string): EntityState[]
```

Returns every entity whose logical zone membership currently includes `zoneId`. Entities are included only after the zone's dwell threshold has been met (if one is configured).

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

engine.registerZone('warehouse', {
  type: 'Polygon',
  coordinates: [[[0, 0], [10, 0], [10, 10], [0, 10], [0, 0]]],
})

engine.ingest([
  { id: 'truck-1', x: 5, y: 5, tMs: Date.now() },
  { id: 'truck-2', x: 50, y: 50, tMs: Date.now() },
])

const inWarehouse = engine.entitiesInZone('warehouse')
// [{ id: 'truck-1', x: 5, y: 5, t_ms: ..., speed: undefined, heading: undefined }]
```

### By circle

```typescript
engine.entitiesInCircle(circleId: string): EntityState[]
```

Returns every entity currently inside the named circle (i.e., those for which an `approach` event has fired and no `recede` has followed).

```typescript
engine.registerCircle('loading-bay', 0, 0, 5)

engine.ingest([{ id: 'van-1', x: 3, y: 0, tMs: Date.now() }])

const atBay = engine.entitiesInCircle('loading-bay')
// [{ id: 'van-1', ... }]
```

### By catalog region

```typescript
engine.entitiesInRegion(regionId: string): EntityState[]
```

Returns every entity whose current catalog region matches `regionId`.

```typescript
engine.registerCatalogRegion('north-district', {
  type: 'Polygon',
  coordinates: [[[0, 0], [20, 0], [20, 20], [0, 20], [0, 0]]],
})

engine.ingest([{ id: 'driver-5', x: 10, y: 10, tMs: Date.now() }])

const northDrivers = engine.entitiesInRegion('north-district')
// [{ id: 'driver-5', ... }]
```

## Spatial queries

Spatial queries search by Euclidean distance from a point. Results always include a `distance` field and are sorted nearest-first.

```typescript
type EntityWithDistance = EntityState & { distance: number }
```

### All entities within a radius

```typescript
engine.entitiesNearPoint(x: number, y: number, radius: number): EntityWithDistance[]
```

Returns every known entity within `radius` of `(x, y)`, sorted by distance ascending. Entities with no known position (i.e., never ingested) are excluded.

```typescript
const nearby = engine.entitiesNearPoint(10, 10, 50)
for (const entity of nearby) {
  console.log(entity.id, entity.distance.toFixed(1))
}
```

### k-nearest entities

```typescript
engine.nearestToPoint(x: number, y: number, k: number): EntityWithDistance[]
```

Returns the `k` closest entities to `(x, y)`, sorted by distance ascending. Useful for dispatch — find the nearest available resource without scanning the full fleet.

```typescript
const nearest3 = engine.nearestToPoint(10, 10, 3)
// nearest3[0] is closest, nearest3[2] is furthest of the three
```

## Examples

### Fleet dispatch — find the nearest driver

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

// Ingest the latest position for every active driver
engine.ingest([
  { id: 'driver-1', x: 12.1, y: 8.4, tMs: Date.now() },
  { id: 'driver-2', x: 3.2,  y: 14.7, tMs: Date.now() },
  { id: 'driver-3', x: 27.5, y: 2.1,  tMs: Date.now() },
  { id: 'driver-4', x: 9.9,  y: 11.3, tMs: Date.now() },
])

// Pickup request arrives at (10, 10) — find the 3 closest drivers
const pickup = { x: 10, y: 10 }
const candidates = engine.nearestToPoint(pickup.x, pickup.y, 3)

for (const driver of candidates) {
  console.log(`${driver.id}: ${driver.distance.toFixed(2)} units away`)
}
// driver-4: 1.34 units away
// driver-1: 2.69 units away
// driver-2: 9.56 units away
```

### Depot occupancy dashboard

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

engine.registerZone('depot-a', {
  type: 'Polygon',
  coordinates: [[[0, 0], [10, 0], [10, 10], [0, 10], [0, 0]]],
})
engine.registerZone('depot-b', {
  type: 'Polygon',
  coordinates: [[[20, 0], [30, 0], [30, 10], [20, 10], [20, 0]]],
})

// Refresh on any new ingest batch
function depotSummary() {
  return {
    'depot-a': engine.entitiesInZone('depot-a').map(e => e.id),
    'depot-b': engine.entitiesInZone('depot-b').map(e => e.id),
  }
}

engine.ingest([
  { id: 'truck-1', x: 5, y: 5, tMs: Date.now() },
  { id: 'truck-2', x: 25, y: 5, tMs: Date.now() },
])

console.log(depotSummary())
// { 'depot-a': ['truck-1'], 'depot-b': ['truck-2'] }
```

## Performance

All five query methods scan the full set of known entities in O(N). For fleets of up to tens of thousands of entities this is fast enough to call synchronously after each ingest batch. There is no separate index to maintain — the engine's in-memory state is the source of truth.
