import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { GeoEventEmitter } from '../emitter.js'
import type { GeoEngine, GeoEvent, GeoJsonPolygonInput, PointUpdate, DwellOptions } from '../types.js'

// ---------------------------------------------------------------------------
// Mock GeoEngine — returns a preset event list, records calls
// ---------------------------------------------------------------------------

function makeEngine(events: GeoEvent[] = []): GeoEngine & { calls: PointUpdate[][] } {
  const calls: PointUpdate[][] = []
  return {
    calls,
    registerZone(_id: string, _polygon: GeoJsonPolygonInput, _dwell?: DwellOptions) {},
    registerCatalogRegion(_id: string, _polygon: GeoJsonPolygonInput) {},
    registerCircle(_id: string, _cx: number, _cy: number, _r: number) {},
    ingest(updates: PointUpdate[]) {
      calls.push(updates)
      return events
    },
  } as unknown as GeoEngine & { calls: PointUpdate[][] }
}

const POLYGON: GeoJsonPolygonInput = {
  type: 'Polygon',
  coordinates: [[[0, 0], [1, 0], [1, 1], [0, 1], [0, 0]]],
}

const UPDATE: PointUpdate = { id: 'v1', x: 0.5, y: 0.5, tMs: 1_700_000_000_000 }

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('GeoEventEmitter — event emission', () => {
  it('emits enter event with correct payload', () => {
    const event: GeoEvent = { kind: 'enter', id: 'v1', zone: 'z1', t_ms: 1000 }
    const emitter = new GeoEventEmitter(makeEngine([event]))
    const received: GeoEvent[] = []
    emitter.on('enter', (ev) => received.push(ev))
    emitter.ingest([UPDATE])
    assert.equal(received.length, 1)
    assert.deepEqual(received[0], event)
  })

  it('emits exit event with correct payload', () => {
    const event: GeoEvent = { kind: 'exit', id: 'v1', zone: 'z1', t_ms: 2000 }
    const emitter = new GeoEventEmitter(makeEngine([event]))
    const received: GeoEvent[] = []
    emitter.on('exit', (ev) => received.push(ev))
    emitter.ingest([UPDATE])
    assert.deepEqual(received[0], event)
  })

  it('emits approach event with correct payload', () => {
    const event: GeoEvent = { kind: 'approach', id: 'v1', circle: 'c1', t_ms: 3000 }
    const emitter = new GeoEventEmitter(makeEngine([event]))
    const received: GeoEvent[] = []
    emitter.on('approach', (ev) => received.push(ev))
    emitter.ingest([UPDATE])
    assert.deepEqual(received[0], event)
  })

  it('emits recede event with correct payload', () => {
    const event: GeoEvent = { kind: 'recede', id: 'v1', circle: 'c1', t_ms: 4000 }
    const emitter = new GeoEventEmitter(makeEngine([event]))
    const received: GeoEvent[] = []
    emitter.on('recede', (ev) => received.push(ev))
    emitter.ingest([UPDATE])
    assert.deepEqual(received[0], event)
  })

  it('emits assignment_changed with non-null region', () => {
    const event: GeoEvent = { kind: 'assignment_changed', id: 'v1', region: 'r1', t_ms: 5000 }
    const emitter = new GeoEventEmitter(makeEngine([event]))
    const received: GeoEvent[] = []
    emitter.on('assignment_changed', (ev) => received.push(ev))
    emitter.ingest([UPDATE])
    assert.deepEqual(received[0], event)
  })

  it('emits assignment_changed with null region (unassigned)', () => {
    const event: GeoEvent = { kind: 'assignment_changed', id: 'v1', region: null, t_ms: 6000 }
    const emitter = new GeoEventEmitter(makeEngine([event]))
    const received: GeoEvent[] = []
    emitter.on('assignment_changed', (ev) => received.push(ev))
    emitter.ingest([UPDATE])
    const ev = received[0]
    assert.equal(ev.kind === 'assignment_changed' ? ev.region : 'wrong-kind', null)
  })

  it('emits multiple events from a single ingest in order', () => {
    const events: GeoEvent[] = [
      { kind: 'enter', id: 'v1', zone: 'z1', t_ms: 1000 },
      { kind: 'assignment_changed', id: 'v1', region: 'r1', t_ms: 1000 },
      { kind: 'approach', id: 'v1', circle: 'c1', t_ms: 1000 },
    ]
    const emitter = new GeoEventEmitter(makeEngine(events))
    const received: string[] = []
    emitter.on('enter',              () => received.push('enter'))
    emitter.on('assignment_changed', () => received.push('assignment_changed'))
    emitter.on('approach',           () => received.push('approach'))
    emitter.ingest([UPDATE])
    assert.deepEqual(received, ['enter', 'assignment_changed', 'approach'])
  })

  it('emits no events when engine returns empty array', () => {
    const emitter = new GeoEventEmitter(makeEngine([]))
    let fired = false
    emitter.on('enter', () => { fired = true })
    emitter.ingest([UPDATE])
    assert.equal(fired, false)
  })
})

describe('GeoEventEmitter — once / off', () => {
  it('once() listener fires exactly once across multiple ingests', () => {
    const event: GeoEvent = { kind: 'enter', id: 'v1', zone: 'z1', t_ms: 1000 }
    const emitter = new GeoEventEmitter(makeEngine([event]))
    let count = 0
    emitter.once('enter', () => { count++ })
    emitter.ingest([UPDATE])
    emitter.ingest([UPDATE])
    assert.equal(count, 1)
  })

  it('off() removes a specific listener', () => {
    const event: GeoEvent = { kind: 'enter', id: 'v1', zone: 'z1', t_ms: 1000 }
    const emitter = new GeoEventEmitter(makeEngine([event]))
    let count = 0
    const listener = () => { count++ }
    emitter.on('enter', listener)
    emitter.ingest([UPDATE])
    emitter.off('enter', listener)
    emitter.ingest([UPDATE])
    assert.equal(count, 1)
  })
})

describe('GeoEventEmitter — chaining and delegation', () => {
  it('ingest() returns this', () => {
    const emitter = new GeoEventEmitter(makeEngine())
    assert.equal(emitter.ingest([UPDATE]), emitter)
  })

  it('registerZone() returns this', () => {
    const engine = makeEngine()
    const emitter = new GeoEventEmitter(engine)
    assert.equal(emitter.registerZone('z', POLYGON), emitter)
  })

  it('registerCatalogRegion() returns this', () => {
    const engine = makeEngine()
    const emitter = new GeoEventEmitter(engine)
    assert.equal(emitter.registerCatalogRegion('r', POLYGON), emitter)
  })

  it('registerCircle() returns this', () => {
    const engine = makeEngine()
    const emitter = new GeoEventEmitter(engine)
    assert.equal(emitter.registerCircle('c', 0, 0, 1), emitter)
  })

  it('delegates ingest updates to the engine', () => {
    const engine = makeEngine()
    const emitter = new GeoEventEmitter(engine)
    const batch = [UPDATE, { id: 'v2', x: 1, y: 1, tMs: 2000 }]
    emitter.ingest(batch)
    assert.deepEqual(engine.calls[0], batch)
  })
})
