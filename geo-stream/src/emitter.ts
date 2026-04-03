/**
 * EventEmitter wrapper for GeoEngine.
 *
 * Wraps the core `GeoEngine` so that spatial events are emitted via Node's
 * EventEmitter API instead of returned from `ingest()`. Fully typed: each
 * event kind has a strongly-typed listener signature.
 *
 * @example
 * ```ts
 * import { GeoEventEmitter } from '@jamesholcombe/geo-stream/emitter'
 *
 * const engine = new GeoEventEmitter()
 *
 * engine.registerZone('warehouse', { type: 'Polygon', coordinates: [...] })
 *
 * engine.on('enter', (ev) => console.log(ev.id, 'entered', ev.zone, 'at', ev.speed, 'm/s'))
 * engine.on('rule',  (ev) => console.log('rule fired:', ev.name))
 *
 * engine.ingest([{ id: 'truck-1', x: 1, y: 1, tMs: Date.now() }])
 * ```
 */

import { EventEmitter } from "node:events";
import type {
  GeoEngine,
  GeoEvent,
  GeoJsonPolygonInput,
  PointUpdate,
  ZoneOptions,
  EngineOptions,
  SequenceOptions,
  RuleBuilder,
  RuleConfig,
} from "./types.js";

type EnterEvent = Extract<GeoEvent, { kind: "enter" }>;
type ExitEvent = Extract<GeoEvent, { kind: "exit" }>;
type ApproachEvent = Extract<GeoEvent, { kind: "approach" }>;
type RecedeEvent = Extract<GeoEvent, { kind: "recede" }>;
type AssignmentChangedEvent = Extract<GeoEvent, { kind: "assignment_changed" }>;
type RuleEvent = Extract<GeoEvent, { kind: "rule" }>;
type SequenceCompleteEvent = Extract<GeoEvent, { kind: "sequence_complete" }>;

export class GeoEventEmitter extends EventEmitter {
  private _engine: GeoEngine;

  constructor(engine?: GeoEngine) {
    super();
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    this._engine =
      engine ??
      new (
        require("./types.js") as {
          GeoEngine: new (opts?: EngineOptions) => GeoEngine;
        }
      ).GeoEngine();
  }

  // --- Registration (chainable, mirrors GeoEngine) ---

  registerZone(
    id: string,
    polygon: GeoJsonPolygonInput,
    options?: ZoneOptions,
  ): this {
    this._engine.registerZone(id, polygon, options);
    return this;
  }

  registerCatalogRegion(id: string, polygon: GeoJsonPolygonInput): this {
    this._engine.registerCatalogRegion(id, polygon);
    return this;
  }

  registerCircle(id: string, cx: number, cy: number, r: number): this {
    this._engine.registerCircle(id, cx, cy, r);
    return this;
  }

  defineRule(name: string, fn: (rule: RuleBuilder) => RuleConfig): this {
    this._engine.defineRule(name, fn);
    return this;
  }

  defineSequence(opts: SequenceOptions): this {
    this._engine.defineSequence(opts);
    return this;
  }

  /** Process a batch of location updates. Resulting events are emitted on this object. */
  ingest(updates: PointUpdate[]): this {
    for (const ev of this._engine.ingest(updates)) {
      this.emit(ev.kind, ev);
    }
    return this;
  }

  // ---------------------------------------------------------------------------
  // Typed overloads — on / once / off
  // ---------------------------------------------------------------------------

  on(event: "enter", listener: (ev: EnterEvent) => void): this;
  on(event: "exit", listener: (ev: ExitEvent) => void): this;
  on(event: "approach", listener: (ev: ApproachEvent) => void): this;
  on(event: "recede", listener: (ev: RecedeEvent) => void): this;
  on(
    event: "assignment_changed",
    listener: (ev: AssignmentChangedEvent) => void,
  ): this;
  on(event: "rule", listener: (ev: RuleEvent) => void): this;
  on(
    event: "sequence_complete",
    listener: (ev: SequenceCompleteEvent) => void,
  ): this;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  on(event: string | symbol, listener: (...args: any[]) => void): this {
    return super.on(event, listener);
  }

  once(event: "enter", listener: (ev: EnterEvent) => void): this;
  once(event: "exit", listener: (ev: ExitEvent) => void): this;
  once(event: "approach", listener: (ev: ApproachEvent) => void): this;
  once(event: "recede", listener: (ev: RecedeEvent) => void): this;
  once(
    event: "assignment_changed",
    listener: (ev: AssignmentChangedEvent) => void,
  ): this;
  once(event: "rule", listener: (ev: RuleEvent) => void): this;
  once(
    event: "sequence_complete",
    listener: (ev: SequenceCompleteEvent) => void,
  ): this;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  once(event: string | symbol, listener: (...args: any[]) => void): this {
    return super.once(event, listener);
  }

  off(event: "enter", listener: (ev: EnterEvent) => void): this;
  off(event: "exit", listener: (ev: ExitEvent) => void): this;
  off(event: "approach", listener: (ev: ApproachEvent) => void): this;
  off(event: "recede", listener: (ev: RecedeEvent) => void): this;
  off(
    event: "assignment_changed",
    listener: (ev: AssignmentChangedEvent) => void,
  ): this;
  off(event: "rule", listener: (ev: RuleEvent) => void): this;
  off(
    event: "sequence_complete",
    listener: (ev: SequenceCompleteEvent) => void,
  ): this;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  off(event: string | symbol, listener: (...args: any[]) => void): this {
    return super.off(event, listener);
  }
}
