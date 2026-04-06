---
id: driver-dispatch
title: Driver dispatch
sidebar_position: 4
description: Combine rules, sequences, and zone events for a realistic pickup and route scenario.
---

This walkthrough builds a realistic dispatch scenario: a driver becomes available when they slow down near a pickup zone, and a `sequence_complete` fires when they complete the full pickup route.

It demonstrates how rules, sequences, and basic zone events compose on a single engine.

## The scenario

A rideshare fleet operates in a city. Drivers:

1. Start their shift by entering a `staging-area` polygon
2. Become available for assignment when they approach the `pickup-circle` at low speed
3. Complete a pickup when they visit `pickup-circle` then `dropoff-zone` in order

## Setup

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine({ historySize: 10 })

// Staging area — a polygon in the city centre
engine.registerZone('staging-area', {
  type: 'Polygon',
  coordinates: [[[0, 0], [5, 0], [5, 5], [0, 5], [0, 0]]],
})

// Pickup zone — a circle around the pickup point
engine.registerCircle('pickup-circle', 10, 10, 1.5)

// Dropoff zone — destination polygon
engine.registerZone('dropoff-zone', {
  type: 'Polygon',
  coordinates: [[[20, 20], [25, 20], [25, 25], [20, 25], [20, 20]]],
})

// Rule: driver is available when they slow down near the pickup point
engine.defineRule('driver-available', rule =>
  rule
    .whenApproaches('pickup-circle')
    .speedBelow(3)              // moving slowly — ready to stop
    .emit('available-for-rider')
)

// Sequence: full pickup route — approach pickup, then enter dropoff zone
engine.defineSequence({
  name: 'pickup-complete',
  steps: ['pickup-circle', 'dropoff-zone'],
  withinMs: 30 * 60 * 1000,   // must complete within 30 minutes
})
```

## Ingesting updates

```typescript
const t0 = 1_700_000_000_000

// Driver starts shift in staging area
const shiftStart = engine.ingest([
  { id: 'driver-7', x: 2.5, y: 2.5, tMs: t0 },
])
console.log(shiftStart)
// [{ kind: 'enter', id: 'driver-7', zone: 'staging-area', t_ms: ... }]

// Driver moves toward pickup zone and arrives slowly
// (second update provides enough history to compute speed)
engine.ingest([
  { id: 'driver-7', x: 8.0, y: 8.0, tMs: t0 + 60_000 },
])
const atPickup = engine.ingest([
  { id: 'driver-7', x: 10.0, y: 10.0, tMs: t0 + 61_000 }, // slow final approach
])

for (const ev of atPickup) {
  console.log(ev)
}
// { kind: 'approach', id: 'driver-7', circle: 'pickup-circle', t_ms: ..., speed: 2.83 }
// { kind: 'rule',     id: 'driver-7', name: 'driver-available', t_ms: ... }
```

The `approach` fires first (the base spatial event), then the `rule` fires because speed is below 3.

## Completing the sequence

```typescript
// Driver picks up the rider and drives to the dropoff zone
const delivery = engine.ingest([
  { id: 'driver-7', x: 22.5, y: 22.5, tMs: t0 + 600_000 },
])

for (const ev of delivery) {
  console.log(ev)
}
// { kind: 'enter',             id: 'driver-7', zone: 'dropoff-zone', t_ms: ... }
// { kind: 'sequence_complete', id: 'driver-7', sequence: 'pickup-complete', t_ms: ... }
```

Both `enter` (the base zone event) and `sequence_complete` (the sequence step completion) are emitted from the same ingest call.

## Handling all event types

Use a `switch` or `GeoEventEmitter` to react to each event kind:

```typescript
for (const ev of events) {
  switch (ev.kind) {
    case 'enter':
      if (ev.zone === 'staging-area') onShiftStart(ev.id)
      break
    case 'rule':
      if (ev.name === 'driver-available') offerRiderAssignment(ev.id)
      break
    case 'sequence_complete':
      if (ev.sequence === 'pickup-complete') markTripComplete(ev.id)
      break
  }
}
```

---

For the event-driven equivalent using Node.js `EventEmitter` semantics, see [Adapters — GeoEventEmitter](../adapters#geoeventemitter).
