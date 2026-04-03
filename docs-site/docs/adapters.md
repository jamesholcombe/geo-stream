---
id: adapters
title: Adapters
sidebar_position: 8
description: Connect the geo-stream engine to Node.js EventEmitter, Kafka, and Redis Streams.
---

The core `GeoEngine` class returns events synchronously from `ingest()`. Three adapters wrap it for the most common integration patterns — event-driven Node.js code, Kafka, and Redis Streams.

All adapters use structural typing: they define their own minimal interfaces and work with any compatible client library. There are no hard peer dependencies.

## GeoEventEmitter

`GeoEventEmitter` wraps a `GeoEngine` as a Node.js `EventEmitter`. Instead of collecting the array returned by `ingest()`, you subscribe to event kinds with `on()`.

```bash
import { GeoEventEmitter } from '@jamesholcombe/geo-stream/emitter'
```

```typescript
import { GeoEventEmitter } from '@jamesholcombe/geo-stream/emitter'

const engine = new GeoEventEmitter()

engine
  .registerZone('warehouse', warehousePolygon)
  .registerCircle('depot-beacon', 7, 7, 1.5)
  .defineRule('fast-entry', rule =>
    rule.whenEnters('warehouse').speedAbove(10).emit('speeding-entry')
  )

engine.on('enter', (ev) => {
  console.log(`${ev.id} entered ${ev.zone}`, ev.speed ? `at ${ev.speed} m/s` : '')
})

engine.on('rule', (ev) => {
  if (ev.name === 'fast-entry') {
    triggerSpeedAlert(ev.id)
  }
})

engine.on('sequence_complete', (ev) => {
  console.log(`${ev.id} completed sequence ${ev.sequence}`)
})

// Ingest returns `this` so it chains
engine.ingest(locationUpdates)
```

`GeoEventEmitter` exposes the same registration methods as `GeoEngine` (`registerZone`, `registerCircle`, `registerCatalogRegion`, `defineRule`, `defineSequence`), all chainable.

### Typed listeners

Each `on()` / `once()` / `off()` overload is typed to its event kind — no casts needed:

```typescript
engine.on('enter', (ev) => {
  // ev: { kind: 'enter', id: string, zone: string, t_ms: number } & EventMeta
  console.log(ev.zone)
})

engine.on('assignment_changed', (ev) => {
  // ev: { kind: 'assignment_changed', id: string, region: string | null, t_ms: number }
  if (ev.region === null) markUnassigned(ev.id)
})
```

## GeoStreamKafka

`GeoStreamKafka` consumes `PointUpdate` JSON messages from a Kafka topic, processes them through a `GeoEngine`, and publishes `GeoEvent` JSON to an output topic.

```bash
import { GeoStreamKafka } from '@jamesholcombe/geo-stream/kafka'
```

```typescript
import { Kafka } from 'kafkajs'
import { GeoEngine } from '@jamesholcombe/geo-stream'
import { GeoStreamKafka } from '@jamesholcombe/geo-stream/kafka'

const kafka = new Kafka({ brokers: ['localhost:9092'] })

const engine = new GeoEngine()
engine.registerZone('site', sitePolygon)

const adapter = new GeoStreamKafka(engine, {
  consumer: kafka.consumer({ groupId: 'geo-stream' }),
  producer: kafka.producer(),
  inputTopic: 'location-updates',
  outputTopic: 'geo-events',
  onParseError: (raw, err) => console.error('Bad message:', raw, err),
})

await adapter.connect()
await adapter.start()   // runs until stop() is called

// Later:
await adapter.stop()
```

**Message formats:**

- Input: JSON-encoded `PointUpdate` — `{ "id": "...", "x": 0, "y": 0, "tMs": 0 }`
- Output: JSON-encoded `GeoEvent` — `{ "kind": "enter", "id": "...", "zone": "...", "t_ms": 0 }`

One output message is produced per event. If an update produces no events, nothing is published.

**Works with any Kafka client** that satisfies the `KafkaConsumer` / `KafkaProducer` interfaces — kafkajs, confluent-kafka-javascript, or a mock in tests.

## GeoStreamRedis

`GeoStreamRedis` reads location updates from a Redis Stream (`XREAD BLOCK`) and writes events to an output stream (`XADD`).

```bash
import { GeoStreamRedis } from '@jamesholcombe/geo-stream/redis'
```

```typescript
import { createClient } from 'redis'
import { GeoEngine } from '@jamesholcombe/geo-stream'
import { GeoStreamRedis } from '@jamesholcombe/geo-stream/redis'

const redis = createClient()
await redis.connect()

const engine = new GeoEngine()
engine.registerZone('site', sitePolygon)

const adapter = new GeoStreamRedis(engine, {
  client: redis,
  inputStream: 'location-updates',
  outputStream: 'geo-events',
  batchSize: 100,     // XREAD COUNT — default 100
  blockMs: 1000,      // XREAD BLOCK ms — default 1000
  startId: '$',       // '$' = only new messages; '0' = from beginning
  onParseError: (fields) => console.error('Bad entry:', fields),
})

await adapter.start()  // runs until stop() is called

// Later:
adapter.stop()
```

**Entry formats:**

- Input Redis entry fields: `id`, `x`, `y`, `t_ms` (string-encoded numbers)
- Output: one XADD per event, with event fields spread as Redis hash fields

**Works with any Redis client** that satisfies the `RedisStreamClient` interface — ioredis, node-redis v4+, or a mock in tests.

:::tip
For high-throughput workloads, increase `batchSize` to process more updates per XREAD call and reduce the number of round trips.
:::
