import type { Polygon, MultiPolygon, Feature } from "geojson";
import type {
  PointUpdateJs as PointUpdate,
  DwellOptionsJs as DwellOptions,
} from "../index.js";

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

export interface EntityState {
  id: string;
  x: number;
  y: number;
  t_ms: number;
  speed?: number; // units/s
  heading?: number; // degrees 0–360
}
