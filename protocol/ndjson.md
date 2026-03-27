# NDJSON protocol

Language-agnostic **newline-delimited JSON** over **stdin** / **stdout**. Each line is one JSON object.

**JSON Schema (draft 2020-12)** for stdin, stdout, and stderr line shapes lives under [`protocol/schema/`](schema/); see [`protocol/schema/README.md`](schema/README.md).

The wire format is **pre-release** (working toward `0.0.1`); shapes may change until a release is published.

## Streams

| Stream  | Role |
|---------|------|
| **stdin** | Input commands (one JSON object per line). |
| **stdout** | Output **events** (one JSON object per line). |
| **stderr** | Output **errors** (one JSON object per line). |

Clients must not mix error payloads on stdout if they parse events line-by-line.

## Optional fields on input

Input lines may include an optional numeric **`v`** field. Parsers should tolerate **unknown fields** where possible.

## Input: register a geofence

Registers a polygon before processing updates. `polygon` must be a GeoJSON **Polygon** geometry object (including `type` and `coordinates`).

```json
{"type":"register_geofence","id":"zone-1","polygon":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}}
```

Registration is applied **immediately** when the line is read (before subsequent updates on later lines).

## Input: point update

```json
{"type":"update","id":"c1","location":[0.5,0.5],"t":1700000000000}
```

`location` is `[x, y]` in the same coordinate system as geofence rings (typically WGS-84 longitude/latitude or a projected CRS — the engine does not reproject).

Optional **`t`**: Unix epoch time in **milliseconds** for this observation. If omitted, it defaults to **`0`**. The engine sorts batched updates with the same `id` by `t` ascending before processing.

## CRS and distance

Radius and polygon tests use the **same planar coordinate system** as `location` and polygon rings. Distances are **Euclidean** in those units. The engine does not reproject or use geodesic distance.

## Global zone ids

Every `id` for `register_geofence`, `register_catalog_region`, and `register_radius` must be **unique across all registration kinds**. Duplicate ids are rejected.

## Input: register catalog region

Same **Polygon** geometry. Semantics: at most one **primary** catalog assignment per entity: the **lexicographically smallest** `id` among all catalog polygons that contain the point. When that primary value changes, an `assignment_changed` event is emitted (including transitions to/from unassigned).

```json
{"type":"register_catalog_region","id":"ward-north","polygon":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}}
```

## Input: register radius zone

`center` is `[x, y]`; `radius` must be **positive** (same units as coordinates). A point is inside when Euclidean distance from `center` is **≤ radius** (boundary inclusive).

```json
{"type":"register_radius","id":"anchor-1","center":[0,0],"radius":100}
```

## Timestamps on stdout events

Every emitted event includes **`t`**: the same millisecond value as the `update` line that produced it (the observation time for that transition).

## Output: geofence events

Enter:

```json
{"event":"enter","id":"c1","geofence":"zone-1","t":1700000000000}
```

Exit:

```json
{"event":"exit","id":"c1","geofence":"zone-1","t":1700000000001}
```

## Output: radius events

```json
{"event":"approach","id":"c1","zone":"anchor-1","t":0}
{"event":"recede","id":"c1","zone":"anchor-1","t":0}
```

## Output: catalog assignment

`region` is `null` when not inside any catalog polygon:

```json
{"event":"assignment_changed","id":"c1","region":"ward-north","t":0}
{"event":"assignment_changed","id":"c1","region":null,"t":0}
```

## Output: errors (stderr)

```json
{"error":"line 3: ..."}
```

## Batching (CLI)

The `geo-stream` binary accepts `--batch-size N`:

- **`N = 1` (default):** each `update` line triggers one engine `process_batch` of a single point (streaming-friendly).
- **`N > 1`:** buffer `N` updates then call `process_batch` once with that batch.
- **`N = 0`:** buffer **all** updates until EOF, then a single `process_batch`.

Registration lines are never batched; they always take effect immediately.

## Event ordering (determinism)

Within one `process_batch` call, updates are first ordered by **ascending entity `id`**, then by **ascending `t`** (milliseconds) when the same entity appears more than once. Emitted events are sorted stably by:

1. Entity `id`
2. Observation time **`t`** (milliseconds)
3. Category: **geofence** (`enter` / `exit`), then **radius** (`approach` / `recede`), then **assignment** (`assignment_changed`)
4. Within a category, by zone / geofence / radius id (lexicographic)
5. For geofence and radius: **enter-type** (or `approach`) before **exit-type** (or `recede`)

## Example: pipe a file

```bash
cargo run -p cli --bin geo-stream -- < examples/sample-input.ndjson
```

A larger example with catalog regions and radius zones: [`examples/sample-zones.ndjson`](../examples/sample-zones.ndjson).

Docker (from **this repository root**, where `Cargo.toml` lives):

```bash
docker build -f docker/Dockerfile -t geo-stream .
docker run --rm -i geo-stream < examples/sample-input.ndjson
```

## HTTP adapter (optional)

See [`ROADMAP.md`](ROADMAP.md). The optional `http` adapter exposes JSON endpoints for the same engine; that is **separate** from this NDJSON process contract.

| Endpoint | Body shape |
|----------|------------|
| `POST /v1/register_geofence` | `{"id":"...","polygon":{ GeoJSON Polygon }}` |
| `POST /v1/register_catalog_region` | same |
| `POST /v1/register_radius` | `{"id":"...","cx":0,"cy":0,"r":1.5}` |
| `POST /v1/ingest` | `{"updates":[...]}`; response events match the stdout event shapes above (HTTP may omit some optional fields where serde skips them). |
