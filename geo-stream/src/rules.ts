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

export interface SequenceOptions {
  name: string;
  /** Zone/circle ids to match in order (enter for zones, approach for circles). */
  steps: string[];
  /** If set, the sequence resets if not completed within this many ms. */
  withinMs?: number;
}

/**
 * Fluent builder for configurable rules. Collects triggers and filters, then
 * call `.emit(eventName)` to produce the config object passed to Rust.
 *
 * @example
 * engine.defineRule('fast-entry', rule =>
 *   rule.whenEnters('danger-zone').speedAbove(10).emit('high-speed-entry')
 * )
 */
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

  /** Terminal — returns the serialisable config passed to `GeoEngine.defineRule`. */
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
