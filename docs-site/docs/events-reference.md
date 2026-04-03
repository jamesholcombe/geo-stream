---
id: events-reference
title: Events Reference
sidebar_position: 7
description: Complete reference for all GeoEvent types, fields, and handling patterns.
---

All output from `ingest()` is a `GeoEvent[]`. The `kind` field is a discriminant — use a `switch` on it for exhaustive TypeScript handling.

## GeoEvent union

```typescript
type GeoEvent =
  | ({ kind: 'enter';              id: string; zone: string;           t_ms: number } & EventMeta)
  | ({ kind: 'exit';               id: string; zone: string;           t_ms: number } & EventMeta)
  | ({ kind: 'approach';           id: string; circle: string;         t_ms: number } & EventMeta)
  | ({ kind: 'recede';             id: string; circle: string;         t_ms: number } & EventMeta)
  | {  kind: 'assignment_changed'; id: string; region: string | null;  t_ms: number }
  | ({ kind: 'rule';               id: string; name: string;           t_ms: number; [key: string]: unknown } & EventMeta)
  | {  kind: 'sequence_complete';  id: string; sequence: string;       t_ms: number }
```

## Event table

| `kind` | Key fields | Emitted when |
|--------|-----------|-------------|
| `enter` | `id`, `zone`, `t_ms` | Entity enters a polygon zone |
| `exit` | `id`, `zone`, `t_ms` | Entity exits a polygon zone |
| `approach` | `id`, `circle`, `t_ms` | Entity enters a circle |
| `recede` | `id`, `circle`, `t_ms` | Entity exits a circle |
| `assignment_changed` | `id`, `region \| null`, `t_ms` | Entity's catalog region changes |
| `rule` | `id`, `name`, `t_ms` | A defined rule's conditions are met |
| `sequence_complete` | `id`, `sequence`, `t_ms` | All steps of a sequence are completed |

## EventMeta — speed and heading

Spatial events (`enter`, `exit`, `approach`, `recede`, `rule`) carry optional movement metadata when the engine has enough position history to compute it:

```typescript
type EventMeta = {
  speed?: number    // units/s — same units as your coordinate system
  heading?: number  // degrees 0–360, north-up clockwise
}
```

Both fields are `undefined` on the first event for a given entity, since computing them requires at least two positions.

```typescript
engine.on('enter', (ev) => {
  if (ev.speed !== undefined) {
    console.log(`${ev.id} entered ${ev.zone} at ${ev.speed.toFixed(1)} units/s`)
  }
})
```

The `historySize` option on `GeoEngine` controls how many past positions the engine retains for these calculations. See [Ingesting Updates](./ingest#engine-options).

## Handling events

Use a `switch` on `kind` for exhaustive TypeScript handling. The compiler will warn you if a new case is missing:

```typescript
import type { GeoEvent } from '@jamesholcombe/geo-stream'

function handleEvent(ev: GeoEvent): void {
  switch (ev.kind) {
    case 'enter':
      console.log(`${ev.id} entered zone "${ev.zone}" at ${ev.t_ms}`)
      break
    case 'exit':
      console.log(`${ev.id} left zone "${ev.zone}" at ${ev.t_ms}`)
      break
    case 'approach':
      console.log(`${ev.id} approached circle "${ev.circle}" at ${ev.t_ms}`)
      break
    case 'recede':
      console.log(`${ev.id} receded from circle "${ev.circle}" at ${ev.t_ms}`)
      break
    case 'assignment_changed':
      console.log(`${ev.id} is now in region "${ev.region ?? 'none'}" at ${ev.t_ms}`)
      break
    case 'rule':
      console.log(`Rule "${ev.name}" fired for ${ev.id} at ${ev.t_ms}`)
      break
    case 'sequence_complete':
      console.log(`Sequence "${ev.sequence}" completed by ${ev.id} at ${ev.t_ms}`)
      break
  }
}
```

## Common fields

**`id`** — The entity identifier from the `PointUpdate` that triggered the event.

**`t_ms`** — Unix epoch milliseconds, taken from `tMs` on the triggering `PointUpdate`. Not the system clock at processing time.

## rule events

`rule` events are emitted when a named rule's conditions are met. See [Rules and Sequences](./rules).

The `name` field is the string you passed to `defineRule`. Any extra `data` you attached to the rule is spread onto the event object alongside `id`, `name`, and `t_ms`.

## assignment_changed

`region` is `null` when the entity is outside all catalog regions. This lets you detect when an entity leaves the last known region.

## sequence_complete

`sequence` is the name of the completed sequence. The event fires on the update that triggered the final step. See [Rules and Sequences](./rules#sequences).

## GeoJsonPolygonInput

Zone and catalog-region registration methods accept any of these GeoJSON shapes:

```typescript
type GeoJsonPolygonInput =
  | Polygon
  | MultiPolygon
  | Feature<Polygon>
  | Feature<MultiPolygon>
```

These types come from `@types/geojson`, which is installed automatically as a peer dependency.

## Coordinate system

`x` = easting or longitude, `y` = northing or latitude. The engine is unit-agnostic. See [Zone Types — Circles](./zone-types#circles) for a note on Euclidean vs. geodesic distance.
