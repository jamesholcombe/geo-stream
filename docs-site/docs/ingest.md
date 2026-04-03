---
id: ingest
title: Ingesting Updates
sidebar_position: 4
description: How to feed location updates into the engine and query entity state.
---

Every location update is a `PointUpdate`. You feed batches of them to `ingest()` and receive spatial events in return.

## PointUpdate

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

`ingest` accepts an array of any size. Updates for different entities can be interleaved — the engine sorts by `(entity_id, tMs)` before processing, so you do not need to pre-sort.

Returns a `GeoEvent[]` in deterministic order: `(entity_id, t_ms, tier, zone_id)`.

## Monotonicity

Timestamps for a given entity must be non-decreasing across `ingest()` calls. An update with a timestamp earlier than the entity's last-seen timestamp is silently skipped.

This rule applies per-entity, not globally. Entity A can send `t=1000` while Entity B sends `t=500`.

## Performance

Batching updates into a single `ingest()` call is more efficient than calling `ingest()` once per update — each call crosses the FFI boundary between JavaScript and the Rust engine. For high-throughput workloads, accumulate updates and flush in batches.

## Engine options

Pass `EngineOptions` to the `GeoEngine` constructor to tune behaviour:

```typescript
interface EngineOptions {
  historySize?: number  // Max position samples kept per entity. Default: 10.
}

const engine = new GeoEngine({ historySize: 20 })
```

`historySize` controls how many recent positions are retained per entity. These are used to compute `speed` and `heading` on emitted events. Increasing it smooths speed/heading estimates at the cost of memory.

## Querying entity state

Between ingests, you can inspect what the engine knows about any entity:

```typescript
engine.getEntityState(id: string): EntityState | undefined

interface EntityState {
  id: string
  x: number
  y: number
  t_ms: number
  speed?: number    // units/s — present after at least two updates
  heading?: number  // degrees 0–360, north-up clockwise
}
```

`getEntities()` returns the state of every tracked entity:

```typescript
engine.getEntities(): EntityState[]
```

**Example:**

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

engine.registerZone('warehouse', {
  type: 'Polygon',
  coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]],
})

const t0 = 1_700_000_000_000

const events = engine.ingest([
  { id: 'truck-1', x: 1.0, y: 1.0, tMs: t0 },
  { id: 'truck-2', x: 5.0, y: 2.0, tMs: t0 },
  { id: 'truck-1', x: 10.0, y: 10.0, tMs: t0 + 30_000 },
])

// Query state after the batch
const truck1 = engine.getEntityState('truck-1')
console.log(truck1)
// { id: 'truck-1', x: 10, y: 10, t_ms: 1700000030000, speed: 0.424, heading: 45 }

const all = engine.getEntities()
console.log(all.length) // 2
```
