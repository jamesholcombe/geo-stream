/**
 * 02-multi-zone.ts
 *
 * Demonstrates all three zone types in a single engine:
 *
 *   Zone           → enter / exit
 *   Catalog region → assignment_changed  (tracks "which named area am I in?")
 *   Circle         → approach / recede
 *
 * A single entity ("truck-1") moves through each zone in sequence.
 *
 * Run:
 *   npx ts-node 02-multi-zone.ts
 */

import { GeoEngine, GeoEvent } from '../../geo-stream/types'

const engine = new GeoEngine()

// --- Zone registration ---

// A zone: rectangular polygon (x: 0–2, y: 0–2)
engine.registerZone('warehouse', {
  type: 'Polygon',
  coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]],
})

// Catalog regions: mutually exclusive named areas.
// The engine emits assignment_changed when the entity moves between them.
engine.registerCatalogRegion('district-north', {
  type: 'Polygon',
  coordinates: [[[0, 5], [10, 5], [10, 10], [0, 10], [0, 5]]],
})
engine.registerCatalogRegion('district-south', {
  type: 'Polygon',
  coordinates: [[[0, 0], [10, 0], [10, 5], [0, 5], [0, 0]]],
})

// A circle: centre (7, 7), radius 1.5 units
engine.registerCircle('depot-beacon', 7, 7, 1.5)

// --- Simulate entity movement ---

const t0 = 1_700_000_000_000

const waypoints: Array<{ x: number; y: number; label: string }> = [
  { x: 1.0,  y: 1.0,  label: 'inside warehouse (district-south)' },
  { x: 4.0,  y: 0.25, label: 'open road (district-south)'        },
  { x: 6.0,  y: 6.0,  label: 'in district-north'                 },
  { x: 7.0,  y: 7.0,  label: 'inside depot-beacon (district-north)'},
  { x: 20.0, y: 20.0, label: 'open space (no zones)'             },
]

for (let i = 0; i < waypoints.length; i++) {
  const { x, y, label } = waypoints[i]
  const tMs = t0 + i * 30_000

  const events = engine.ingest([{ id: 'truck-1', x, y, tMs }])

  console.log(`\n[t+${i * 30}s]  pos=(${x}, ${y})  — ${label}`)
  if (events.length === 0) {
    console.log('  (no events)')
  }
  for (const ev of events) {
    console.log(' ', formatEvent(ev))
  }
}

function formatEvent(ev: GeoEvent): string {
  switch (ev.kind) {
    case 'enter':
      return `ENTER zone "${ev.zone}"`
    case 'exit':
      return `EXIT  zone "${ev.zone}"`
    case 'approach':
      return `APPROACH circle "${ev.circle}"`
    case 'recede':
      return `RECEDE   circle "${ev.circle}"`
    case 'assignment_changed':
      return `ASSIGNED → catalog region "${ev.region ?? 'none (unassigned)'}"`
  }
}
