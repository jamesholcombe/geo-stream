import { GeoEngineNode } from "../index.js";
import { RuleBuilder } from "./rules.js";
import type {
  GeoEvent,
  GeoJsonPolygonInput,
  EntityState,
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
  getEntityState(id: string): EntityState | undefined;
  getEntities(): EntityState[];
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

  registerCircle(id: string, cx: number, cy: number, r: number): this {
    this.node.registerCircle(id, cx, cy, r);
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
