import { GeoEngineNode } from "./native.js";
import type { EntityStateJs, EntityWithDistanceJs } from "./native.js";
import { RuleBuilder } from "./rules.js";
import type {
  GeoEvent,
  GeoJsonPolygonInput,
  EntityState,
  EntityWithDistance,
  PointUpdate,
} from "./events.js";
import type { RuleConfig, SequenceOptions } from "./rules.js";

export interface EngineOptions {
  /** Max historical position samples kept per entity. Default: 10. */
  historySize?: number;
}

export interface ZoneOptions {
  dwell?: {
    minInsideMs?: number;
    minOutsideMs?: number;
  };
}

export interface CircleOptions {
  dwell?: {
    minInsideMs?: number;
    minOutsideMs?: number;
  };
}

// Shape of the native node including methods added by new NAPI code.
// This is replaced by the auto-generated index.d.ts after `npm run build`.
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
  getEntityState(id: string): EntityStateJs | null;
  getEntities(): EntityStateJs[];
  entitiesInZone(zoneId: string): EntityStateJs[];
  entitiesInCircle(circleId: string): EntityStateJs[];
  entitiesInRegion(regionId: string): EntityStateJs[];
  entitiesNearPoint(
    x: number,
    y: number,
    radius: number,
  ): EntityWithDistanceJs[];
  nearestToPoint(x: number, y: number, k: number): EntityWithDistanceJs[];
}

// NAPI-RS converts Rust snake_case field names to camelCase in JavaScript, so the native
// module returns `tMs` while our public TypeScript API uses `t_ms`. This mapper normalises
// the field name so callers always receive the documented shape.
function jsToEntityState(js: EntityStateJs): EntityState {
  return {
    id: js.id,
    x: js.x,
    y: js.y,
    t_ms: js.tMs,
    speed: js.speed,
    heading: js.heading,
  };
}

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

  registerCircle(
    id: string,
    cx: number,
    cy: number,
    r: number,
    options?: CircleOptions,
  ): this {
    const dwell = options?.dwell
      ? {
          minInsideMs: options.dwell.minInsideMs,
          minOutsideMs: options.dwell.minOutsideMs,
        }
      : undefined;
    this.node.registerCircle(id, cx, cy, r, dwell);
    return this;
  }

  registerCatalogRegion(id: string, polygon: GeoJsonPolygonInput): this {
    this.node.registerCatalogRegion(id, polygon as unknown);
    return this;
  }

  // --- Rules ---

  defineRule(name: string, fn: (rule: RuleBuilder) => RuleConfig): this {
    const config = fn(new RuleBuilder());
    config.name = name;
    this.node.defineRule({
      name,
      triggers: config.triggers.map((t) => ({
        eventKind: t.eventKind,
        targetId: t.targetId,
      })),
      filters: config.filters.map((f) =>
        f.filterType === "heading_between"
          ? { filterType: f.filterType, value: null, from: f.from, to: f.to }
          : { filterType: f.filterType, value: f.value, from: null, to: null },
      ),
      emit: config.emit,
      data: config.data ?? null,
    });
    return this;
  }

  defineSequence(opts: SequenceOptions): this {
    this.node.defineSequence({
      name: opts.name,
      steps: opts.steps,
      withinMs: opts.withinMs,
    });
    return this;
  }

  // --- Queries ---

  /** Return all entities currently inside the named zone. */
  entitiesInZone(zoneId: string): EntityState[] {
    return this.node.entitiesInZone(zoneId).map(jsToEntityState);
  }

  /** Return all entities currently inside the named circle. */
  entitiesInCircle(circleId: string): EntityState[] {
    return this.node.entitiesInCircle(circleId).map(jsToEntityState);
  }

  /** Return all entities whose current catalog region matches `regionId`. */
  entitiesInRegion(regionId: string): EntityState[] {
    return this.node.entitiesInRegion(regionId).map(jsToEntityState);
  }

  /**
   * Return all entities within `radius` of `(x, y)`, sorted by distance ascending.
   * Each result includes a `distance` field in the same units as coordinates.
   */
  entitiesNearPoint(
    x: number,
    y: number,
    radius: number,
  ): EntityWithDistance[] {
    return this.node
      .entitiesNearPoint(x, y, radius)
      .map((js) => ({ ...jsToEntityState(js), distance: js.distance }));
  }

  /**
   * Return the `k` nearest entities to `(x, y)`, sorted by distance ascending.
   * Each result includes a `distance` field in the same units as coordinates.
   */
  nearestToPoint(x: number, y: number, k: number): EntityWithDistance[] {
    return this.node
      .nearestToPoint(x, y, k)
      .map((js) => ({ ...jsToEntityState(js), distance: js.distance }));
  }

  // --- Ingest ---

  ingest(updates: PointUpdate[]): GeoEvent[] {
    return this.node.ingest(updates) as GeoEvent[];
  }

  // --- Entity state ---

  getEntityState(id: string): EntityState | undefined {
    const state = this.node.getEntityState(id);
    return state ? jsToEntityState(state) : undefined;
  }

  getEntities(): EntityState[] {
    return this.node.getEntities().map(jsToEntityState);
  }
}
