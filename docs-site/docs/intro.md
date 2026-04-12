---
id: intro
slug: /
title: Introduction
sidebar_position: 1
description: geo-stream is an embeddable rules engine for location streams — in-process, deterministic, no infrastructure required.
---

geo-stream is an embeddable rules engine for location streams. You drop it into your Node.js or Rust process, define zones and rules, feed in position updates, and receive typed spatial events. No server to run, no network calls, no external dependencies.

## What makes it different

Most geofencing tools run as a separate networked service — you send a position update, a remote process evaluates it, and an event comes back asynchronously. This works, but it means:

- A round-trip per position update
- No guaranteed event ordering (two updates sent close together may produce events out of order)
- Noise filtering, dwell detection, and sequence tracking must be built on top externally

geo-stream runs inside your process. The result:

- **Zero latency** — no network hop between update and event
- **Deterministic ordering** — same inputs always produce the same events in the same order; backtesting and replay are exact
- **Dwell/debounce built in** — suppress noisy enter/exit cycling with a minimum-inside or minimum-outside time threshold, per zone
- **Sequence rules** — emit a custom event when an entity enters zone A, then zone B, within a time window
- **Speed and heading filters** — compose conditions like "fire only when above 30 km/h heading north-east"

## How the system thinks

Three concepts are enough to reason about any geo-stream behavior:

**Entities** are anything you track: drivers, vehicles, assets, people. Each has an `id` and moves through space as location updates arrive.

**State** is what the engine remembers: which zones each entity is currently inside, its last known position, speed, and heading. State is updated on every `ingest()` call.

**Events** fire when state changes. An entity entering a zone produces `enter`. Leaving produces `exit`. Entering a circle produces `approach`; leaving produces `recede`. Events carry the entity `id`, the zone or circle identifier, and the timestamp (`t_ms`) of the update that caused the transition.

Nothing happens between updates. The engine is event-driven: you push updates in, events come out, the rest of the time it is quiet.

:::note Coordinate system
geo-stream currently operates on a flat Euclidean plane. Pass coordinates in any consistent unit — metres, pixels, or decimal degrees — and the enter/exit logic works correctly. Distance-based operations (circles, `entities_near_point`) will produce approximate results when using GPS coordinates at large geographic scales. WGS84/geodesic mode is on the roadmap.
:::

## Quick start

A delivery fleet: drivers enter a warehouse to pick up loads, then leave.

```bash
npm install @jamesholcombe/geo-stream
```

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

// Register the warehouse as a polygon zone
engine.registerZone('warehouse', {
  type: 'Polygon',
  coordinates: [[[0, 0], [10, 0], [10, 10], [0, 10], [0, 0]]],
})

// Driver arrives at the warehouse
const arrivals = engine.ingest([
  { id: 'driver-42', x: 5, y: 5, tMs: Date.now() },
])

for (const ev of arrivals) {
  if (ev.kind === 'enter') {
    console.log(`${ev.id} entered ${ev.zone} — assign pickup job`)
    // driver-42 entered warehouse — assign pickup job
  }
}

// Driver departs
const departures = engine.ingest([
  { id: 'driver-42', x: 50, y: 50, tMs: Date.now() + 300_000 },
])

for (const ev of departures) {
  if (ev.kind === 'exit') {
    console.log(`${ev.id} left ${ev.zone} — job in progress`)
    // driver-42 left warehouse — job in progress
  }
}
```

## The primitives

Two zone types produce two pairs of events:

| Zone type | Registration | Events |
|-----------|-------------|--------|
| Polygon zone | `registerZone` | `enter` / `exit` |
| Circle | `registerCircle` | `approach` / `recede` |

Both coexist on the same engine. A single location update is evaluated against every registered zone simultaneously.

Rules and sequences let you compose these primitives further — emit a custom event when an entity enters a zone at speed, or detect when a driver completes a multi-stop route in order. See [Rules and Sequences](./rules).

You can also query the engine's in-memory state at any time: find every entity currently inside a zone, or ask for the nearest k entities to a point. See [Querying entities](./querying).

## No infrastructure required

geo-stream is a Rust library compiled to a native Node.js module. There is no server to run, no schema to migrate, no network calls. Drop it into any Node.js process and it works immediately, with state held in memory alongside your application.

For workloads that already use a message broker, ready-made adapters connect the engine to [Kafka and Redis Streams](./adapters).

## When not to use geo-stream

geo-stream is not a spatial database. It does not store objects between restarts (beyond snapshots), does not answer ad-hoc range queries over historical positions, and does not support multi-server fan-out. If you need a queryable, persistent store of geospatial objects with a Redis-compatible protocol accessible from any language, look at [Tile38](https://tile38.com). If you need deterministic in-process event processing with dwell logic, sequence detection, and zero infrastructure overhead, geo-stream is the right tool.

## Rust, CLI, and NDJSON

This documentation focuses on the Node.js npm package. The same engine also ships as Rust crates (`crates/engine`, `crates/state`, and others), an NDJSON CLI binary, and a wire format under `protocol/`. For repository layout, building from source, and piping sample NDJSON through the CLI, see the [geo-events README on GitHub](https://github.com/jamesholcombe/geo-events).
