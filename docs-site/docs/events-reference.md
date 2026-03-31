---
sidebar_position: 6
---

# Events Reference

## GeoEvent discriminated union

All events share the `kind` discriminant field. The full type:

```typescript
type GeoEvent =
  | { kind: 'enter';              id: string; zone: string;          t_ms: number }
  | { kind: 'exit';               id: string; zone: string;          t_ms: number }
  | { kind: 'approach';           id: string; circle: string;        t_ms: number }
  | { kind: 'recede';             id: string; circle: string;        t_ms: number }
  | { kind: 'assignment_changed'; id: string; region: string | null; t_ms: number }
```

## Event table

| `kind` | Fields | Emitted when |
|--------|--------|-------------|
| `enter` | `id`, `zone`, `t_ms` | Entity enters a polygon zone |
| `exit` | `id`, `zone`, `t_ms` | Entity exits a polygon zone |
| `approach` | `id`, `circle`, `t_ms` | Entity enters a circle |
| `recede` | `id`, `circle`, `t_ms` | Entity exits a circle |
| `assignment_changed` | `id`, `region \| null`, `t_ms` | Entity's primary catalog region changes |

## Handling events

Use a `switch` on `kind` for exhaustive TypeScript handling. The compiler will warn you if a case is missing:

```typescript
import { GeoEvent } from '@jamesholcombe/geo-stream/types'

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
      console.log(
        `${ev.id} is now in region "${ev.region ?? 'none'}" at ${ev.t_ms}`
      )
      break
  }
}
```

## Common fields

**`id`** — The entity identifier from the `PointUpdate` that triggered the event. Matches the `id` you passed to `ingest()`.

**`t_ms`** — Unix epoch milliseconds. This is the timestamp of the *update* that triggered the event, taken directly from `tMs` on the `PointUpdate`. It is not the system clock at time of processing.

## assignment_changed

`region` is `null` when the entity is not inside any catalog region — it has moved outside all registered catalog regions. This lets you detect when an entity leaves the last known region.

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

`x` = easting or longitude, `y` = northing or latitude. The engine is unit-agnostic — use degrees (WGS-84), metres, or any other consistent unit. See the [Circles section](./zone-types#circles) for a note on Euclidean vs geodesic distance.
