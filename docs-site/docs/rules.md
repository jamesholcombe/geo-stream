---
id: rules
title: Rules and Sequences
sidebar_position: 6
description: Define conditional event rules and ordered zone sequences on top of the core spatial events.
---

Rules and sequences let you express higher-level behaviors by composing the core spatial primitives. Instead of processing raw `enter`/`exit` events in your application code, you declare the conditions once on the engine and receive purpose-built events.

## Rules

A rule watches for a spatial event — an entity entering or leaving a zone or circle — and emits a custom named event when additional filter conditions are met.

```typescript
engine.defineRule(name: string, fn: (rule: RuleBuilder) => RuleConfig): this
```

The fluent `RuleBuilder` collects triggers and filters, then produces a config object when you call `.emit()`:

```typescript
engine.defineRule('fast-entry', rule =>
  rule
    .whenEnters('restricted-area')
    .speedAbove(15)
    .emit('speeding-entry')
)
```

When `driver-7` enters `restricted-area` traveling faster than 15 units/s, the engine emits:

```typescript
{ kind: 'rule', id: 'driver-7', name: 'fast-entry', t_ms: ..., speed: 18.2 }
```

### Triggers

Each `.when*()` call adds a trigger. Multiple triggers on the same rule create an OR condition — the rule fires if any trigger matches.

| Method | Fires when |
|--------|-----------|
| `.whenEnters(zoneId)` | Entity enters a polygon zone |
| `.whenExits(zoneId)` | Entity exits a polygon zone |
| `.whenApproaches(circleId)` | Entity enters a circle |
| `.whenRecedes(circleId)` | Entity exits a circle |

### Filters

Filters narrow the trigger condition. Multiple filters on the same rule create an AND condition — all must pass for the rule to fire.

| Method | Passes when |
|--------|------------|
| `.speedAbove(mps)` | Entity speed > threshold |
| `.speedBelow(mps)` | Entity speed < threshold |
| `.headingBetween(from, to)` | Entity heading is within the arc `[from, to]` degrees |

Speed and heading are computed from position history. If the engine does not have enough history to compute them yet, speed and heading filters will not match.

### Custom event data

Attach extra fields to the emitted event by passing a data object to `.emit()`:

```typescript
engine.defineRule('depot-arrival', rule =>
  rule
    .whenApproaches('depot-circle')
    .speedBelow(5)
    .emit('slow-approach', { priority: 'high' })
)
// Emits: { kind: 'rule', id: '...', name: 'depot-arrival', t_ms: ..., priority: 'high' }
```

### Real-world example: rider dispatch

A driver needs to be in the pickup zone and moving slowly before being assigned a rider:

```typescript
import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

engine
  .registerCircle('pickup-zone', 37.7749, -122.4194, 0.001) // ~100m radius in degrees
  .defineRule('ready-for-dispatch', rule =>
    rule
      .whenApproaches('pickup-zone')
      .speedBelow(2)
      .emit('driver-available')
  )

engine.on // use GeoEventEmitter for event-based handling, or switch on ingest() results

const events = engine.ingest([
  { id: 'driver-12', x: -122.4194, y: 37.7749, tMs: Date.now() },
])

for (const ev of events) {
  if (ev.kind === 'rule' && ev.name === 'ready-for-dispatch') {
    console.log(`${ev.id} is available for pickup assignment`)
  }
}
```

## Sequences

A sequence detects when an entity completes a series of zone visits in order. The engine emits `sequence_complete` when all steps are checked off.

```typescript
engine.defineSequence({
  name: string,
  steps: string[],    // Zone or circle IDs to enter, in order
  withinMs?: number,  // Optional: reset if not completed within this window
}): this
```

Each step matches an `enter` event for polygon zones or an `approach` event for circles.

### Real-world example: delivery route verification

A driver must visit a depot, then a loading bay, then a customer site — in that order — within 2 hours:

```typescript
engine
  .registerZone('depot', depotPolygon)
  .registerZone('loading-bay', loadingBayPolygon)
  .registerZone('customer-site', customerPolygon)
  .defineSequence({
    name: 'delivery-route',
    steps: ['depot', 'loading-bay', 'customer-site'],
    withinMs: 2 * 60 * 60 * 1000, // 2 hours
  })

// When driver-5 visits all three zones in order within 2 hours:
// { kind: 'sequence_complete', id: 'driver-5', sequence: 'delivery-route', t_ms: ... }
```

If the driver visits `customer-site` before `loading-bay`, the sequence does not advance. If the 2-hour window expires before all steps are completed, the sequence resets and the driver must start again from `depot`.

### Combining rules and sequences

Rules and sequences work alongside basic zone events on the same engine. A single `ingest()` call can produce `enter`, `assignment_changed`, `rule`, and `sequence_complete` events simultaneously.

```typescript
const engine = new GeoEngine()
  .registerZone('zone-a', polygonA)
  .registerZone('zone-b', polygonB)
  .registerZone('zone-c', polygonC)
  .defineRule('fast-a-entry', rule =>
    rule.whenEnters('zone-a').speedAbove(10).emit('speeding')
  )
  .defineSequence({
    name: 'a-to-c',
    steps: ['zone-a', 'zone-b', 'zone-c'],
    withinMs: 30 * 60 * 1000,
  })
```
