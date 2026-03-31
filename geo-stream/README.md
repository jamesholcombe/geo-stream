# @jamesholcombe/geo-stream

Native Node.js bindings for the **geo-stream** geospatial stream processor — a fast, in-memory engine that turns location updates into structured spatial events (enter/exit, approach/recede, assignment changes).

## Install

```bash
npm install @jamesholcombe/geo-stream
```

Pre-built native binaries are included for:

- macOS (arm64, x64)
- Linux (x64, arm64, gnu)
- Windows (x64)

No compilation step required.

## Quick start

```ts
import { GeoEngine } from '@jamesholcombe/geo-stream/types'

const engine = new GeoEngine()

// Register a polygon zone (GeoJSON Polygon)
engine.registerZone('city-centre', {
  type: 'Polygon',
  coordinates: [
    [[0, 0], [1, 0], [1, 1], [0, 1], [0, 0]],
  ],
})

// Process location updates
const events = engine.ingest([
  { id: 'vehicle-1', x: 0.5, y: 0.5, tMs: Date.now() },
])

for (const ev of events) {
  console.log(ev)
  // { kind: 'enter', id: 'vehicle-1', zone: 'city-centre', t_ms: ... }
}
```

## Zone types

### Polygon zones — `enter` / `exit`

```ts
engine.registerZone('warehouse', {
  type: 'Polygon',
  coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]],
})
```

Optionally debounce noisy GPS with dwell thresholds:

```ts
engine.registerZone(
  'loading-bay',
  { type: 'Polygon', coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]] },
  {
    minInsideMs: 5_000,   // must dwell inside ≥ 5 s before 'enter' fires
    minOutsideMs: 3_000,  // must stay outside ≥ 3 s before 'exit' fires
  },
)
```

### Catalog regions — `assignment_changed`

Catalog regions represent mutually exclusive named areas (e.g. districts, territories). The engine emits `assignment_changed` when an entity's containing region changes, including when it leaves all regions (`region: null`).

```ts
engine.registerCatalogRegion('district-north', {
  type: 'Polygon',
  coordinates: [[[0, 5], [10, 5], [10, 10], [0, 10], [0, 5]]],
})
engine.registerCatalogRegion('district-south', {
  type: 'Polygon',
  coordinates: [[[0, 0], [10, 0], [10, 5], [0, 5], [0, 0]]],
})
```

### Circles — `approach` / `recede`

```ts
engine.registerCircle('depot-beacon', 7, 7, 1.5) // cx, cy, radius
```

## Processing updates

`ingest` accepts a batch of location updates (sorted by entity ID then timestamp internally) and returns all resulting events:

```ts
const events = engine.ingest([
  { id: 'truck-1', x: 1.0, y: 1.0, tMs: 1_700_000_000_000 },
  { id: 'truck-2', x: 7.0, y: 7.0, tMs: 1_700_000_000_000 },
])
```

Updates within a batch can be interleaved across entities. Timestamps must be monotonically non-decreasing per entity — earlier timestamps for the same entity are skipped.

## TypeScript types

Import from the `types` entry point for full type safety:

```ts
import { GeoEngine, GeoEvent, GeoJsonPolygonInput } from '@jamesholcombe/geo-stream/types'
```

### `GeoEvent` discriminated union

```ts
type GeoEvent =
  | { kind: 'enter';              id: string; zone: string;          t_ms: number }
  | { kind: 'exit';               id: string; zone: string;          t_ms: number }
  | { kind: 'approach';           id: string; circle: string;        t_ms: number }
  | { kind: 'recede';             id: string; circle: string;        t_ms: number }
  | { kind: 'assignment_changed'; id: string; region: string | null; t_ms: number }
```

Use a `switch` on `kind` for exhaustive handling:

```ts
function handle(ev: GeoEvent) {
  switch (ev.kind) {
    case 'enter':              console.log(ev.id, 'entered', ev.zone);           break
    case 'exit':               console.log(ev.id, 'left',    ev.zone);           break
    case 'approach':           console.log(ev.id, 'near',    ev.circle);         break
    case 'recede':             console.log(ev.id, 'far from', ev.circle);        break
    case 'assignment_changed': console.log(ev.id, 'now in',  ev.region ?? 'none'); break
  }
}
```

### `GeoJsonPolygonInput`

Zone and catalog-region registration methods accept any of:

```ts
type GeoJsonPolygonInput =
  | Polygon
  | MultiPolygon
  | Feature<Polygon>
  | Feature<MultiPolygon>
```

These types come from `@types/geojson`, which is installed automatically as a dependency.

### `PointUpdate`

```ts
interface PointUpdate {
  id: string   // entity identifier
  x: number    // longitude or easting (unit-agnostic)
  y: number    // latitude or northing
  tMs: number  // Unix epoch milliseconds
}
```

### `DwellOptions`

```ts
interface DwellOptions {
  minInsideMs?: number   // milliseconds entity must be inside before 'enter' fires
  minOutsideMs?: number  // milliseconds entity must be outside before 'exit' fires
}
```

## Coordinates

The engine is coordinate-system-agnostic. Use degrees (WGS-84), metres, or any other consistent unit. `x` = easting/longitude, `y` = northing/latitude.

---

## Adapters

### EventEmitter — `@jamesholcombe/geo-stream/emitter`

Wraps `GeoEngine` as a Node.js `EventEmitter`. Spatial events are emitted by kind rather than returned from `ingest()`. Each event kind has a fully typed listener signature.

```ts
import { GeoEventEmitter } from '@jamesholcombe/geo-stream/emitter'

const engine = new GeoEventEmitter()

engine.registerZone('warehouse', {
  type: 'Polygon',
  coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]],
})

engine.on('enter', (ev) => console.log(ev.id, 'entered', ev.zone))
engine.on('exit',  (ev) => console.log(ev.id, 'left',    ev.zone))
engine.on('assignment_changed', (ev) => console.log(ev.id, '→', ev.region ?? 'unassigned'))

// TypeScript narrows ev.zone / ev.circle / ev.region per event kind automatically.
engine.ingest([{ id: 'truck-1', x: 1, y: 1, tMs: Date.now() }])
```

`GeoEventEmitter` exposes the same `registerZone`, `registerCatalogRegion`, and `registerCircle` methods as `GeoEngine`. All methods return `this` for chaining. No extra dependencies required.

---

### Kafka — `@jamesholcombe/geo-stream/kafka`

Consumes `PointUpdate` JSON from a Kafka input topic, processes updates through a `GeoEngine`, and publishes `GeoEvent` JSON to an output topic.

Uses structural typing — no hard dependency on `kafkajs`. Any Kafka client that satisfies the `KafkaConsumer` / `KafkaProducer` interfaces works.

```ts
import { Kafka } from 'kafkajs'
import { GeoEngine } from '@jamesholcombe/geo-stream/types'
import { GeoStreamKafka } from '@jamesholcombe/geo-stream/kafka'

const kafka = new Kafka({ brokers: ['localhost:9092'] })
const engine = new GeoEngine()
engine.registerZone('site', { type: 'Polygon', coordinates: [...] })

const adapter = new GeoStreamKafka(engine, {
  consumer:    kafka.consumer({ groupId: 'geo-stream' }),
  producer:    kafka.producer(),
  inputTopic:  'location-updates',
  outputTopic: 'geo-events',
  onParseError: (raw, err) => console.error('Bad message:', raw, err),
})

await adapter.connect()
await adapter.start()  // subscribes and runs until stop() is called

// ...later...
await adapter.stop()
```

**Message format:**
- Input — JSON `PointUpdate`: `{ "id": "truck-1", "x": 0.5, "y": 0.5, "tMs": 1700000000000 }`
- Output — JSON `GeoEvent`: `{ "kind": "enter", "id": "truck-1", "zone": "site", "t_ms": 1700000000000 }`

**Options:**

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `consumer` | `KafkaConsumer` | required | Connected Kafka consumer |
| `producer` | `KafkaProducer` | required | Connected Kafka producer |
| `inputTopic` | `string` | required | Topic to consume `PointUpdate` messages from |
| `outputTopic` | `string` | required | Topic to publish `GeoEvent` messages to |
| `onParseError` | `function` | no-op | Called when a message cannot be parsed as a `PointUpdate` |

---

### Redis Streams — `@jamesholcombe/geo-stream/redis`

Reads `PointUpdate` entries from a Redis input stream via `XREAD BLOCK`, processes updates through a `GeoEngine`, and appends `GeoEvent` entries to an output stream via `XADD`.

Uses structural typing — no hard dependency on `ioredis`. Any Redis client that satisfies the `RedisStreamClient` interface works.

```ts
import Redis from 'ioredis'
import { GeoEngine } from '@jamesholcombe/geo-stream/types'
import { GeoStreamRedis } from '@jamesholcombe/geo-stream/redis'

const redis = new Redis()
const engine = new GeoEngine()
engine.registerZone('depot', { type: 'Polygon', coordinates: [...] })

const adapter = new GeoStreamRedis(engine, {
  client:       redis,
  inputStream:  'location-updates',
  outputStream: 'geo-events',
})

adapter.start()  // non-blocking — runs poll loop in the background

// ...later...
adapter.stop()
```

**Stream entry format:**
- Input — field-value pairs: `id <entityId> x <lon> y <lat> t_ms <epoch_ms>`
- Output — field-value pairs: one field per `GeoEvent` key, e.g. `kind enter id truck-1 zone depot t_ms 1700000000000`

**Options:**

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `client` | `RedisStreamClient` | required | Connected Redis client |
| `inputStream` | `string` | required | Stream key to read `PointUpdate` entries from |
| `outputStream` | `string` | required | Stream key to write `GeoEvent` entries to |
| `batchSize` | `number` | `100` | Max entries per `XREAD` call |
| `blockMs` | `number` | `1000` | Milliseconds to block waiting for new entries |
| `startId` | `string` | `'$'` | Stream ID to begin reading from. Use `'0'` to replay from the start |
| `onParseError` | `function` | no-op | Called when an entry cannot be parsed into a valid `PointUpdate` |

---

## Examples

More examples are in the [geo-stream repository](https://github.com/jamesholcombe/geo-stream/tree/main/examples/typescript):

- `01-basic-zone.ts` — single zone, enter/exit lifecycle
- `02-multi-zone.ts` — all three zone types in one engine
- `03-dwell.ts` — dwell thresholds to suppress boundary noise
