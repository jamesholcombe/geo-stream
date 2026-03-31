---
sidebar_position: 4
---

# Ingesting Updates

## PointUpdate

Each location update is a `PointUpdate` object:

```typescript
interface PointUpdate {
  id: string   // Entity identifier — any string; uniquely identifies a tracked object
  x: number    // Easting or longitude — unit-agnostic
  y: number    // Northing or latitude
  tMs: number  // Unix epoch milliseconds
}
```

## The `ingest` method

```typescript
engine.ingest(updates: PointUpdate[]): GeoEvent[]
```

`ingest` accepts an array of updates of any size. Updates for different entities can be freely interleaved — the engine automatically sorts by `(entity_id, tMs)` before processing, so you do not need to pre-sort your batch.

Returns a `GeoEvent[]` containing all events produced by the batch, in deterministic order: `(entity_id, t_ms, tier, zone_id)`.

## Monotonicity rule

Timestamps for a given entity must be non-decreasing across `ingest()` calls. If an update arrives with a timestamp earlier than the entity's last-seen timestamp, the update is silently skipped — no error is thrown.

This rule applies per-entity, not globally. Entity A can send a timestamp of `t=1000` at the same time Entity B sends `t=500`.

## Performance

Batching updates into a single `ingest()` call is more efficient than calling `ingest()` once per update. Each call crosses the FFI boundary between JavaScript and the native Rust engine. For high-throughput workloads, accumulate updates and flush them in batches.

## Example

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream/types'

const engine = new GeoEngine()

engine.registerZone('warehouse', {
  type: 'Polygon',
  coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]],
})
engine.registerCatalogRegion('district-south', {
  type: 'Polygon',
  coordinates: [[[0, 0], [10, 0], [10, 5], [0, 5], [0, 0]]],
})

const t0 = 1_700_000_000_000

// Multi-entity batch — truck-1 and truck-2 interleaved
const events = engine.ingest([
  { id: 'truck-1', x: 1.0, y: 1.0, tMs: t0 },           // inside warehouse
  { id: 'truck-2', x: 5.0, y: 2.0, tMs: t0 },           // inside district-south
  { id: 'truck-1', x: 10.0, y: 10.0, tMs: t0 + 30_000 }, // truck-1 exits warehouse
])

for (const ev of events) {
  console.log(ev)
}
// { kind: 'enter',              id: 'truck-1', zone: 'warehouse',       t_ms: 1700000000000 }
// { kind: 'assignment_changed', id: 'truck-2', region: 'district-south', t_ms: 1700000000000 }
// { kind: 'exit',               id: 'truck-1', zone: 'warehouse',       t_ms: 1700000030000 }
```
