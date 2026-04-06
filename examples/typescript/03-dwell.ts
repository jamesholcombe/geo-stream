/**
 * 03-dwell.ts
 *
 * Dwell thresholds prevent spurious enter / exit events when an entity
 * briefly crosses or hovers near a zone boundary.
 *
 *   minInsideMs  — entity must be continuously inside for this long before
 *                  an "enter" event fires.
 *   minOutsideMs — entity must be continuously outside for this long before
 *                  an "exit" event fires.
 *
 * This example contrasts two entities:
 *   van-1  makes a brief incursion (< minInsideMs)  → no events
 *   van-2  stays inside long enough                  → enter + exit events
 *
 * Prerequisites: same as 01-basic-zone.ts (napi-build, geo-stream compile, npm install here).
 *
 * Run:
 *   npx ts-node 03-dwell.ts
 */

import { GeoEngine } from '@jamesholcombe/geo-stream'

const engine = new GeoEngine()

engine.registerZone(
  'loading-bay',
  {
    type: 'Polygon',
    coordinates: [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]],
  },
  {
    minInsideMs: 5_000,   // must dwell inside ≥ 5 s before "enter" fires
    minOutsideMs: 3_000,  // must stay outside ≥ 3 s before "exit" fires
  },
)

const t0 = 1_700_000_000_000

// --- Entity 1: brief incursion ---
// van-1 enters and leaves within 2 s — below the 5 s threshold.
// No events should fire.
const briefEvents = engine.ingest([
  { id: 'van-1', x: 1.0, y: 1.0, tMs: t0 },
  { id: 'van-1', x: 5.0, y: 5.0, tMs: t0 + 2_000 }, // 2 s inside — not enough
])

console.log('van-1 brief incursion (expect no events):')
console.log(' ', briefEvents.length === 0 ? '(no events — dwell threshold not met)' : briefEvents)

// --- Entity 2: sustained stay ---
// van-2 remains inside for 10 s (> minInsideMs of 5 s), then exits and stays
// outside for 5 s (> minOutsideMs of 3 s). Both thresholds are met.
const sustainedEvents = engine.ingest([
  { id: 'van-2', x: 1.0, y: 1.0, tMs: t0 },
  { id: 'van-2', x: 1.0, y: 1.0, tMs: t0 + 5_000 },  // still inside at 5 s → enter fires
  { id: 'van-2', x: 5.0, y: 5.0, tMs: t0 + 10_000 }, // moves outside
  { id: 'van-2', x: 5.0, y: 5.0, tMs: t0 + 13_000 }, // still outside at 3 s → exit fires
])

console.log('\nvan-2 sustained stay (expect enter + exit):')
for (const ev of sustainedEvents) {
  console.log(' ', ev)
}
// { kind: 'enter', id: 'van-2', zone: 'loading-bay', t_ms: 1700000005000 }
// { kind: 'exit',  id: 'van-2', zone: 'loading-bay', t_ms: 1700000013000 }
