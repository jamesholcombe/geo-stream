---
id: intro
slug: /
title: Introduction
sidebar_position: 1
description: geo-stream turns location updates into spatial events — in-process, no infrastructure required.
---

Entities move through space. The engine tracks which zones they're in. When membership changes, events fire.

That is the entire mental model. You define the zones once, feed in location updates, and receive typed events — `enter`, `exit`, `approach`, `recede`, `assignment_changed` — whenever something meaningful happens. No polling, no queries, no database.

## How the system thinks

Three concepts are enough to reason about any geo-stream behavior:

**Entities** are anything you track: drivers, vehicles, assets, people. Each has an `id` and moves through space as location updates arrive.

**State** is what the engine remembers: which zones each entity is currently inside, which catalog region it belongs to, its last known position. State is updated on every `ingest()` call.

**Events** fire when state changes. An entity entering a zone produces `enter`. Leaving produces `exit`. Moving from one catalog region to another produces `assignment_changed`. Events carry the entity `id`, the zone or region identifier, and the timestamp (`t_ms`) of the update that caused the transition.

Nothing happens between updates. The engine is event-driven: you push updates in, events come out, the rest of the time it is quiet.

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

Three zone types produce four pairs of events:

| Zone type | Registration | Events |
|-----------|-------------|--------|
| Polygon zone | `registerZone` | `enter` / `exit` |
| Circle | `registerCircle` | `approach` / `recede` |
| Catalog region | `registerCatalogRegion` | `assignment_changed` |

All three coexist on the same engine. A single location update is evaluated against every registered zone and region simultaneously.

Rules and sequences let you compose these primitives further — emit a custom event when an entity enters a zone at speed, or detect when a driver completes a multi-stop route in order. See [Rules and Sequences](./rules).

You can also query the engine's in-memory state at any time: find every entity currently inside a zone, or ask for the nearest k entities to a point. See [Querying entities](./querying).

## No infrastructure required

geo-stream is a Rust library compiled to a native Node.js module. There is no server to run, no schema to migrate, no network calls. Drop it into any Node.js process and it works immediately, with state held in memory alongside your application.

For workloads that already use a message broker, ready-made adapters connect the engine to [Kafka and Redis Streams](./adapters).

## Rust, CLI, and NDJSON

This documentation focuses on the Node.js npm package. The same engine also ships as Rust crates (`crates/engine`, `crates/state`, and others), an NDJSON CLI binary, and a wire format under `protocol/`. For repository layout, building from source, and piping sample NDJSON through the CLI, see the [geo-events README on GitHub](https://github.com/jamesholcombe/geo-events).
