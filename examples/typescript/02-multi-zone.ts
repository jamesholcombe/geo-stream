/**
 * 02-multi-zone.ts
 *
 * Demonstrates all three zone types in a single engine:
 *
 *   Geofence       → enter / exit
 *   Catalog region → assignment_changed  (tracks "which named area am I in?")
 *   Radius zone    → approach / recede
 *
 * A single entity ("truck-1") moves through each zone in sequence.
 *
 * Run:
 *   npx ts-node 02-multi-zone.ts
 */

import { GeoEngine, GeoEvent } from '../../crates/adapters/napi/types'

const engine = new GeoEngine()

// --- Zone registration ---

// A geofence: rectangular polygon (x: 0–2, y: 0–2)
engine.registerGeofence('warehouse', {
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

// A radius zone: circular area — centre (7, 7), radius 1.5 units
engine.registerRadiusZone('depot-beacon', 7, 7, 1.5)

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
      return `ENTER geofence "${ev.geofence}"`
    case 'exit':
      return `EXIT  geofence "${ev.geofence}"`
    case 'approach':
      return `APPROACH radius zone "${ev.zone}"`
    case 'recede':
      return `RECEDE   radius zone "${ev.zone}"`
    case 'assignment_changed':
      return `ASSIGNED → catalog region "${ev.region ?? 'none (unassigned)'}"`
  }
}
