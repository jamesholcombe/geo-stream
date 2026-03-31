import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { GeoStreamKafka } from '../kafka.js'
import type { KafkaConsumer, KafkaProducer } from '../kafka.js'
import type { GeoEngine, GeoEvent, PointUpdate } from '../types.js'

// ---------------------------------------------------------------------------
// Mock helpers
// ---------------------------------------------------------------------------

type EachMessageFn = (payload: { message: { value: Buffer | null } }) => Promise<void>

interface MockConsumer extends KafkaConsumer {
  _subscribed: Array<{ topic: string; fromBeginning?: boolean }>
  _eachMessage: EachMessageFn | null
  connectCalled: boolean
  disconnectCalled: boolean
}

function makeConsumer(): MockConsumer {
  return {
    connectCalled: false,
    disconnectCalled: false,
    _subscribed: [],
    _eachMessage: null,
    async connect() { this.connectCalled = true },
    async disconnect() { this.disconnectCalled = true },
    async subscribe(opts) { this._subscribed.push(opts) },
    async run({ eachMessage }) { this._eachMessage = eachMessage },
  }
}

interface MockProducer extends KafkaProducer {
  sent: Array<{ topic: string; messages: Array<{ value: string }> }>
  connectCalled: boolean
  disconnectCalled: boolean
}

function makeProducer(): MockProducer {
  return {
    sent: [],
    connectCalled: false,
    disconnectCalled: false,
    async connect() { this.connectCalled = true },
    async disconnect() { this.disconnectCalled = true },
    async send(record) { this.sent.push(record) },
  }
}

function makeEngine(events: GeoEvent[] = []): GeoEngine & { ingestCalls: PointUpdate[][] } {
  const ingestCalls: PointUpdate[][] = []
  return {
    ingestCalls,
    registerZone() {},
    registerCatalogRegion() {},
    registerCircle() {},
    ingest(updates: PointUpdate[]) {
      ingestCalls.push(updates)
      return events
    },
  } as unknown as GeoEngine & { ingestCalls: PointUpdate[][] }
}

function msg(value: string | null): { message: { value: Buffer | null } } {
  return { message: { value: value === null ? null : Buffer.from(value) } }
}

const INPUT_TOPIC = 'location-updates'
const OUTPUT_TOPIC = 'geo-events'

const VALID_UPDATE: PointUpdate = { id: 'v1', x: 1.5, y: 2.5, tMs: 1_700_000_000_000 }
const VALID_MSG = JSON.stringify(VALID_UPDATE)

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('GeoStreamKafka — lifecycle', () => {
  it('connect() calls connect on both consumer and producer', async () => {
    const consumer = makeConsumer()
    const producer = makeProducer()
    const adapter = new GeoStreamKafka(makeEngine(), { consumer, producer, inputTopic: INPUT_TOPIC, outputTopic: OUTPUT_TOPIC })
    await adapter.connect()
    assert.equal(consumer.connectCalled, true)
    assert.equal(producer.connectCalled, true)
  })

  it('stop() calls disconnect on both consumer and producer', async () => {
    const consumer = makeConsumer()
    const producer = makeProducer()
    const adapter = new GeoStreamKafka(makeEngine(), { consumer, producer, inputTopic: INPUT_TOPIC, outputTopic: OUTPUT_TOPIC })
    await adapter.stop()
    assert.equal(consumer.disconnectCalled, true)
    assert.equal(producer.disconnectCalled, true)
  })

  it('start() subscribes to inputTopic with fromBeginning: false', async () => {
    const consumer = makeConsumer()
    const adapter = new GeoStreamKafka(makeEngine(), { consumer, producer: makeProducer(), inputTopic: INPUT_TOPIC, outputTopic: OUTPUT_TOPIC })
    await adapter.start()
    assert.equal(consumer._subscribed.length, 1)
    assert.equal(consumer._subscribed[0].topic, INPUT_TOPIC)
    assert.equal(consumer._subscribed[0].fromBeginning, false)
  })
})

describe('GeoStreamKafka — message processing', () => {
  async function process(events: GeoEvent[], rawMsg: string | null, onParseError?: (raw: string, err: unknown) => void) {
    const consumer = makeConsumer()
    const producer = makeProducer()
    const engine = makeEngine(events)
    const adapter = new GeoStreamKafka(engine, { consumer, producer, inputTopic: INPUT_TOPIC, outputTopic: OUTPUT_TOPIC, onParseError })
    await adapter.start()
    await consumer._eachMessage!(msg(rawMsg))
    return { consumer, producer, engine }
  }

  it('valid message → engine.ingest called with parsed update', async () => {
    const { engine } = await process([], VALID_MSG)
    assert.equal(engine.ingestCalls.length, 1)
    assert.deepEqual(engine.ingestCalls[0][0], VALID_UPDATE)
  })

  it('valid message with events → producer.send called with serialised events', async () => {
    const events: GeoEvent[] = [{ kind: 'enter', id: 'v1', zone: 'z1', t_ms: 1000 }]
    const { producer } = await process(events, VALID_MSG)
    assert.equal(producer.sent.length, 1)
    assert.equal(producer.sent[0].topic, OUTPUT_TOPIC)
    assert.deepEqual(JSON.parse(producer.sent[0].messages[0].value), events[0])
  })

  it('multiple events → single send with all messages', async () => {
    const events: GeoEvent[] = [
      { kind: 'enter', id: 'v1', zone: 'z1', t_ms: 1000 },
      { kind: 'approach', id: 'v1', circle: 'c1', t_ms: 1000 },
    ]
    const { producer } = await process(events, VALID_MSG)
    assert.equal(producer.sent[0].messages.length, 2)
  })

  it('engine returns no events → producer.send not called', async () => {
    const { producer } = await process([], VALID_MSG)
    assert.equal(producer.sent.length, 0)
  })

  it('null message value → skipped, engine not called', async () => {
    const { engine, producer } = await process([], null)
    assert.equal(engine.ingestCalls.length, 0)
    assert.equal(producer.sent.length, 0)
  })

  it('unparseable JSON → onParseError called, no throw', async () => {
    const errors: string[] = []
    const { engine } = await process([], 'not-json', (raw) => errors.push(raw))
    assert.equal(errors.length, 1)
    assert.equal(errors[0], 'not-json')
    assert.equal(engine.ingestCalls.length, 0)
  })

  it('no onParseError provided → unparseable message silently skipped', async () => {
    // Should not throw
    await assert.doesNotReject(() => process([], 'bad'))
  })
})
