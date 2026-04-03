/**
 * Typed overlay for the NAPI-RS native bindings.
 *
 * Provides:
 *   - `GeoEvent`      — discriminated union covering all event variants
 *   - `RuleBuilder`   — fluent builder that serialises rule config for Rust
 *   - `GeoEngine`     — typed wrapper with zone/rule/entity-state API
 */

import type { Polygon, MultiPolygon, Feature } from "geojson";
import { GeoEngineNode } from "../index.js";
import type {
  PointUpdateJs as PointUpdate,
  DwellOptionsJs as DwellOptions,
} from "../index.js";

export type { PointUpdate, DwellOptions };

// ---------------------------------------------------------------------------
// GeoJSON polygon input
// ---------------------------------------------------------------------------

export type GeoJsonPolygonInput =
  | Polygon
  | MultiPolygon
  | Feature<Polygon>
  | Feature<MultiPolygon>;

// ---------------------------------------------------------------------------
// Event discriminated union
// ---------------------------------------------------------------------------

/** Metadata present on spatial events once two positions have been processed. */
export type EventMeta = {
  speed?: number; // units/s
  heading?: number; // degrees 0–360, north-up clockwise
};

export type GeoEvent =
  | ({ kind: "enter"; id: string; zone: string; t_ms: number } & EventMeta)
  | ({ kind: "exit"; id: string; zone: string; t_ms: number } & EventMeta)
  | ({ kind: "approach"; id: string; circle: string; t_ms: number } & EventMeta)
  | ({ kind: "recede"; id: string; circle: string; t_ms: number } & EventMeta)
  | {
      kind: "assignment_changed";
      id: string;
      region: string | null;
      t_ms: number;
    }
  | ({
      kind: "rule";
      id: string;
      name: string;
      t_ms: number;
      [key: string]: unknown;
    } & EventMeta)
  | { kind: "sequence_complete"; id: string; sequence: string; t_ms: number };

// ---------------------------------------------------------------------------
// Entity state
// ---------------------------------------------------------------------------

export interface EntityState {
  id: string;
  x: number;
  y: number;
  t_ms: number;
  speed?: number; // units/s
  heading?: number; // degrees 0–360
}

// ---------------------------------------------------------------------------
// Rule config types (plain objects passed to Rust)
// ---------------------------------------------------------------------------

export type TriggerKind = "enter" | "exit" | "approach" | "recede";

export interface RuleTrigger {
  eventKind: TriggerKind;
  targetId: string;
}

export type RuleFilterSpec =
  | { filterType: "speed_above"; value: number }
  | { filterType: "speed_below"; value: number }
  | { filterType: "heading_between"; from: number; to: number };

export interface RuleConfig {
  name: string;
  triggers: RuleTrigger[];
  filters: RuleFilterSpec[];
  emit: string;
  data?: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Fluent RuleBuilder — builds a RuleConfig, no runtime logic
// ---------------------------------------------------------------------------

export class RuleBuilder {
  private _triggers: RuleTrigger[] = [];
  private _filters: RuleFilterSpec[] = [];

  whenEnters(zoneId: string): this {
    this._triggers.push({ eventKind: "enter", targetId: zoneId });
    return this;
  }

  whenExits(zoneId: string): this {
    this._triggers.push({ eventKind: "exit", targetId: zoneId });
    return this;
  }

  whenApproaches(circleId: string): this {
    this._triggers.push({ eventKind: "approach", targetId: circleId });
    return this;
  }

  whenRecedes(circleId: string): this {
    this._triggers.push({ eventKind: "recede", targetId: circleId });
    return this;
  }

  speedAbove(mps: number): this {
    this._filters.push({ filterType: "speed_above", value: mps });
    return this;
  }

  speedBelow(mps: number): this {
    this._filters.push({ filterType: "speed_below", value: mps });
    return this;
  }

  headingBetween(from: number, to: number): this {
    this._filters.push({ filterType: "heading_between", from, to });
    return this;
  }

  /** Terminal — returns the serialisable config object. */
  emit(name: string, extra?: Record<string, unknown>): RuleConfig {
    return {
      name: "", // overwritten by GeoEngine.defineRule(name, ...)
      triggers: this._triggers,
      filters: this._filters,
      emit: name,
      data: extra,
    };
  }
}

// ---------------------------------------------------------------------------
// Sequence options
// ---------------------------------------------------------------------------

export interface SequenceOptions {
  name: string;
  /** Zone/circle ids to match in order (enter for zones, approach for circles). */
  steps: string[];
  /** If set, the sequence resets if not completed within this many ms. */
  withinMs?: number;
}

// ---------------------------------------------------------------------------
// Zone registration options
// ---------------------------------------------------------------------------

export interface ZoneOptions {
  dwell?: {
    minInsideMs?: number;
    minOutsideMs?: number;
  };
}

// ---------------------------------------------------------------------------
// Engine options
// ---------------------------------------------------------------------------

export interface EngineOptions {
  /** Max historical position samples kept per entity. Default: 10. */
  historySize?: number;
}

// ---------------------------------------------------------------------------
// Internal shape of the native node (extended by new NAPI methods).
// Once `index.d.ts` is regenerated after `napi build`, this matches exactly.
// ---------------------------------------------------------------------------

interface NativeNode extends InstanceType<typeof GeoEngineNode> {
  defineRule(config: {
    name: string;
    triggers: Array<{ eventKind: string; targetId: string }>;
    filters: Array<{
      filterType: string;
      value?: number | null;
      from?: number | null;
      to?: number | null;
    }>;
    emit: string;
    data?: unknown;
  }): void;
  defineSequence(config: {
    name: string;
    steps: string[];
    withinMs?: number | null;
  }): void;
  getEntityState(id: string): EntityState | undefined;
  getEntities(): EntityState[];
}

// ---------------------------------------------------------------------------
// GeoEngine — typed wrapper
// ---------------------------------------------------------------------------

export class GeoEngine {
  private node: NativeNode;

  constructor(options?: EngineOptions) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    this.node = new (GeoEngineNode as any)(
      options ? { historySize: options.historySize ?? undefined } : undefined,
    ) as NativeNode;
  }

  // --- Registration (chainable) ---

  registerZone(
    id: string,
    polygon: GeoJsonPolygonInput,
    options?: ZoneOptions,
  ): this {
    const dwell = options?.dwell
      ? {
          minInsideMs: options.dwell.minInsideMs,
          minOutsideMs: options.dwell.minOutsideMs,
        }
      : undefined;
    this.node.registerZone(id, polygon as unknown, dwell);
    return this;
  }

  registerCircle(id: string, cx: number, cy: number, r: number): this {
    this.node.registerCircle(id, cx, cy, r);
    return this;
  }

  registerCatalogRegion(id: string, polygon: GeoJsonPolygonInput): this {
    this.node.registerCatalogRegion(id, polygon as unknown);
    return this;
  }

  // --- Rules ---

  /**
   * Define a named rule. The builder callback configures triggers and filters;
   * call `.emit(eventName)` at the end to produce the config.
   *
   * @example
   * engine.defineRule('fast-entry', rule =>
   *   rule.whenEnters('danger-zone').speedAbove(10).emit('high-speed-entry', { severity: 'high' })
   * )
   */
  defineRule(name: string, fn: (rule: RuleBuilder) => RuleConfig): this {
    const config = fn(new RuleBuilder());
    config.name = name;

    this.node.defineRule({
      name,
      triggers: config.triggers.map((t) => ({
        eventKind: t.eventKind,
        targetId: t.targetId,
      })),
      filters: config.filters.map((f) => {
        if (f.filterType === "heading_between") {
          return {
            filterType: f.filterType,
            value: null,
            from: f.from,
            to: f.to,
          };
        }
        return {
          filterType: f.filterType,
          value: f.value,
          from: null,
          to: null,
        };
      }),
      emit: config.emit,
      data: config.data ?? null,
    });
    return this;
  }

  /**
   * Define a sequence rule that fires `sequence_complete` when all steps are
   * triggered (enter/approach) in order.
   *
   * @example
   * engine.defineSequence({ name: 'gate-to-lot', steps: ['gate', 'lot-a'], withinMs: 60_000 })
   */
  defineSequence(opts: SequenceOptions): this {
    this.node.defineSequence({
      name: opts.name,
      steps: opts.steps,
      withinMs: opts.withinMs,
    });
    return this;
  }

  // --- Ingest ---

  ingest(updates: PointUpdate[]): GeoEvent[] {
    return this.node.ingest(updates) as GeoEvent[];
  }

  // --- Entity state ---

  getEntityState(id: string): EntityState | undefined {
    return this.node.getEntityState(id);
  }

  getEntities(): EntityState[] {
    return this.node.getEntities();
  }
}
