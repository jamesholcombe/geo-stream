import type { Polygon, MultiPolygon, Feature } from "geojson";
import type {
  PointUpdateJs as PointUpdate,
  DwellOptionsJs as DwellOptions,
} from "./native.js";

export type { PointUpdate, DwellOptions };

export type GeoJsonPolygonInput =
  | Polygon
  | MultiPolygon
  | Feature<Polygon>
  | Feature<MultiPolygon>;

/** Metadata present on spatial events once two positions have been processed. */
export type EventMeta = {
  speed?: number; // units/s
  heading?: number; // degrees 0–360, north-up clockwise
};

interface BaseEvent {
  id: string;
  t_ms: number;
  kind: string;
}

export type EnterEvent = BaseEvent & {
  kind: "enter";
  zone: string;
} & EventMeta;

export type ExitEvent = BaseEvent & {
  kind: "exit";
  zone: string;
} & EventMeta;

export type ApproachEvent = BaseEvent & {
  kind: "approach";
  circle: string;
} & EventMeta;

export type RecedeEvent = BaseEvent & {
  kind: "recede";
  circle: string;
} & EventMeta;

export type AssignmentChangedEvent = BaseEvent & {
  kind: "assignment_changed";
  region: string | null;
};

export type RuleEvent = {
  kind: "rule";
  name: string;
  [key: string]: unknown;
} & EventMeta;

export type SequenceCompleteEvent = BaseEvent & {
  kind: "sequence_complete";
  sequence: string;
};

export type GeoEvent =
  | EnterEvent
  | ExitEvent
  | ApproachEvent
  | RecedeEvent
  | AssignmentChangedEvent
  | RuleEvent
  | SequenceCompleteEvent;

export interface EntityState {
  id: string;
  x: number;
  y: number;
  t_ms: number;
  speed?: number; // units/s
  heading?: number; // degrees 0–360
}
