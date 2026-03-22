# NDJSON protocol v1.1 (extends v1)

This document **extends** [ndjson-v1.md](ndjson-v1.md). All v1 input lines, events, batching, and streams behave as before. v1.1 adds **corridors** (pre-buffered polygons), **catalog regions** (assignment semantics), and **radius zones** (fixed-anchor disks), plus matching stdout events.

## CRS and distance

Radius and polygon tests use the **same planar coordinate system** as v1 (`location` and polygon rings). Distances are **Euclidean** in those units. The engine does not reproject or use geodesic distance.

## Global zone ids

Every `id` for `register_geofence`, `register_corridor`, `register_catalog_region`, and `register_radius` must be **unique across all four registration kinds**. Duplicate ids are rejected.

## New input lines

### Register corridor (pre-buffered polygon)

Same GeoJSON **Polygon** shape as geofences. Corridors are stored as polygons; clients supply a **buffered** corridor footprint. Events use `enter_corridor` / `exit_corridor`.

```json
{"type":"register_corridor","id":"corridor-main","polygon":{"type":"Polygon","coordinates":[[[0,0],[2,0],[2,0.5],[0,0.5],[0,0]]]}}
```

### Register catalog region

Same **Polygon** geometry. Semantics: at most one **primary** catalog assignment per entity: the **lexicographically smallest** `id` among all catalog polygons that contain the point. When that primary value changes, an `assignment_changed` event is emitted (including transitions to/from unassigned).

```json
{"type":"register_catalog_region","id":"ward-north","polygon":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}}
```

### Register radius zone

`center` is `[x, y]`; `radius` must be **positive** (same units as coordinates). A point is inside when Euclidean distance from `center` is **≤ radius** (boundary inclusive).

```json
{"type":"register_radius","id":"anchor-1","center":[0,0],"radius":100}
```

## New output events

Corridor:

```json
{"event":"enter_corridor","id":"c1","corridor":"corridor-main","t":0}
{"event":"exit_corridor","id":"c1","corridor":"corridor-main","t":0}
```

Radius:

```json
{"event":"approach","id":"c1","zone":"anchor-1","t":0}
{"event":"recede","id":"c1","zone":"anchor-1","t":0}
```

Catalog assignment (`region` is `null` when not inside any catalog polygon):

```json
{"event":"assignment_changed","id":"c1","region":"ward-north","t":0}
{"event":"assignment_changed","id":"c1","region":null,"t":0}
```

## Event ordering (determinism)

Within one `process_batch` call, updates are first ordered by **ascending entity `id`**, then by **ascending `t`** (milliseconds) when the same entity appears more than once. Emitted events are sorted stably by:

1. Entity `id`
2. Observation time **`t`** (milliseconds)
3. Category: **geofence** (`enter` / `exit`), then **corridor**, then **radius** (`approach` / `recede`), then **assignment** (`assignment_changed`)
4. Within a category, by zone / geofence / corridor / radius id (lexicographic)
5. For geofence, corridor, and radius: **enter-type** (or `approach`) before **exit-type** (or `recede`)

Every stdout event object includes **`t`** (same semantics as v1).

## HTTP v2

Matching registration endpoints (JSON bodies):

| Endpoint | Body shape |
|----------|----------------|
| `POST /v2/register_corridor` | `{"id":"...","polygon":{ GeoJSON Polygon }}` |
| `POST /v2/register_catalog_region` | same |
| `POST /v2/register_radius` | `{"id":"...","cx":0,"cy":0,"r":1.5}` |

`POST /v2/ingest` responses may include any v1 or v1.1 event objects.
