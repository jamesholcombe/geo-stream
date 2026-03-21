# NDJSON protocol v1

Language-agnostic **newline-delimited JSON** over **stdin** / **stdout**. Each line is one JSON object.

## Streams

| Stream  | Role |
|---------|------|
| **stdin** | Input commands (one JSON object per line). |
| **stdout** | Output **events** (one JSON object per line). |
| **stderr** | Output **errors** (one JSON object per line). |

Clients must not mix error payloads on stdout if they parse events line-by-line.

## Optional version field

Any input line may include `"v": 1`. Future revisions may bump semantics; parsers should ignore unknown fields where possible.

## Input: register a geofence

Registers a polygon before processing updates. `polygon` must be a GeoJSON **Polygon** geometry object (including `type` and `coordinates`).

```json
{"type":"register_geofence","id":"zone-1","polygon":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}}
```

Registration is applied **immediately** when the line is read (before subsequent updates on later lines).

## Input: point update

```json
{"type":"update","id":"c1","location":[0.5,0.5]}
```

`location` is `[x, y]` in the same coordinate system as geofence rings (typically WGS-84 longitude/latitude or a projected CRS — the engine does not reproject).

## Output: events

Enter:

```json
{"event":"enter","id":"c1","geofence":"zone-1"}
```

Exit:

```json
{"event":"exit","id":"c1","geofence":"zone-1"}
```

## Output: errors (stderr)

```json
{"error":"line 3: ..."}
```

## Batching (CLI)

The `geo-stream` binary accepts `--batch-size N`:

- **`N = 1` (default):** each `update` line triggers one engine `ingest` of a single point (streaming-friendly).
- **`N > 1`:** buffer `N` updates then call `ingest` once with that batch.
- **`N = 0`:** buffer **all** updates until EOF, then a single `ingest`.

Geofence registration lines are never batched; they always take effect immediately.

## Determinism

Within one `ingest` batch, updates are processed in **ascending `id` order**. Emitted events for that batch are sorted by `(entity id, geofence id, enter before exit)`.

## Example: pipe a file

```bash
cargo run -p cli --bin geo-stream -- < examples/sample-input.ndjson
```

Docker (from **this repository root**, where `Cargo.toml` lives):

```bash
docker build -f docker/Dockerfile -t geo-stream .
docker run --rm -i geo-stream < examples/sample-input.ndjson
```

## HTTP v2 (optional)

See [`ROADMAP.md`](ROADMAP.md). The optional `http-adapter` crate exposes JSON endpoints (e.g. `POST /v2/ingest`) for the same engine; that is **separate** from this NDJSON process contract.
