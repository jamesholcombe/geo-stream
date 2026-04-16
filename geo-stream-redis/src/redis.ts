/**
 * Redis Streams adapter for GeoEngine.
 *
 * Reads `PointUpdate` entries from a Redis input stream via `XREAD BLOCK`,
 * processes them through a `GeoEngine`, and appends resulting `GeoEvent`
 * entries to an output stream via `XADD`.
 *
 * Uses structural typing — no hard dependency on `ioredis`. Any Redis client
 * that satisfies the `RedisStreamClient` interface works (ioredis, node-redis ≥4,
 * etc.) provided you adapt the return types accordingly.
 *
 * @example
 * ```ts
 * import Redis from 'ioredis'
 * import { GeoEngine } from '@jamesholcombe/geo-stream'
 * import { GeoStreamRedis } from '@jamesholcombe/geo-stream/redis'
 *
 * const redis = new Redis()
 * const engine = new GeoEngine()
 * engine.registerZone('depot', { type: 'Polygon', coordinates: [...] })
 *
 * const adapter = new GeoStreamRedis(engine, {
 *   client:       redis,
 *   inputStream:  'location-updates',
 *   outputStream: 'geo-events',
 * })
 *
 * adapter.start()   // non-blocking — runs poll loop in background
 * // ...later...
 * adapter.stop()
 * ```
 *
 * Stream entry format:
 *   Input  — field-value pairs: `id <entityId> x <lon> y <lat> t_ms <epoch_ms>`
 *   Output — field-value pairs: one field per `GeoEvent` key
 *             e.g. `kind enter id vehicle-1 zone city-centre t_ms 1700000000000`
 */

import { GeoEngine } from "./types.js";
import type { PointUpdate } from "./types.js";

// ---------------------------------------------------------------------------
// Minimal structural interface for the Redis client
// ---------------------------------------------------------------------------

/**
 * Subset of the Redis client API used by this adapter.
 * Compatible with ioredis and node-redis v4 (with minor wrapping).
 */
export interface RedisStreamClient {
  /**
   * XREAD COUNT <count> BLOCK <blockMs> STREAMS <stream> <lastId>
   * Returns null when the block timeout elapses with no new entries.
   */
  xread(
    countArg: "COUNT",
    count: number,
    blockArg: "BLOCK",
    blockMs: number,
    streamsArg: "STREAMS",
    stream: string,
    lastId: string,
  ): Promise<Array<[string, Array<[string, string[]]>]> | null>;

  /**
   * XADD <stream> * <field> <value> [<field> <value> ...]
   */
  xadd(
    stream: string,
    id: "*",
    ...fieldValues: string[]
  ): Promise<string | null>;
}

export interface GeoStreamRedisOptions {
  client: RedisStreamClient;
  inputStream: string;
  outputStream: string;
  /**
   * Maximum entries to read per XREAD call. Default: 100.
   */
  batchSize?: number;
  /**
   * How long (ms) to block waiting for new entries. Default: 1000.
   */
  blockMs?: number;
  /**
   * Stream ID to begin reading from.
   * Use `'$'` (default) to receive only new entries written after start.
   * Use `'0'` to replay the full stream from the beginning.
   */
  startId?: string;
  /**
   * Called when an entry cannot be parsed into a valid PointUpdate.
   * Defaults to a no-op.
   */
  onParseError?: (fields: Record<string, string>, err: unknown) => void;
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

export class GeoStreamRedis {
  private engine: GeoEngine;
  private client: RedisStreamClient;
  private inputStream: string;
  private outputStream: string;
  private batchSize: number;
  private blockMs: number;
  private lastId: string;
  private onParseError: (fields: Record<string, string>, err: unknown) => void;
  private running = false;

  constructor(engine: GeoEngine, opts: GeoStreamRedisOptions) {
    this.engine = engine;
    this.client = opts.client;
    this.inputStream = opts.inputStream;
    this.outputStream = opts.outputStream;
    this.batchSize = opts.batchSize ?? 100;
    this.blockMs = opts.blockMs ?? 1000;
    this.lastId = opts.startId ?? "$";
    this.onParseError = opts.onParseError ?? (() => undefined);
  }

  /**
   * Start the XREAD poll loop. Returns immediately; processing runs in the
   * background. Rejects if an unrecoverable error occurs.
   */
  start(): Promise<void> {
    this.running = true;
    return this._loop();
  }

  /** Stop the poll loop after the current XREAD call returns. */
  stop(): void {
    this.running = false;
  }

  private async _loop(): Promise<void> {
    while (this.running) {
      const reply = await this.client.xread(
        "COUNT",
        this.batchSize,
        "BLOCK",
        this.blockMs,
        "STREAMS",
        this.inputStream,
        this.lastId,
      );

      if (!reply) continue; // block timeout — no entries

      for (const [, entries] of reply) {
        for (const [entryId, rawFields] of entries) {
          this.lastId = entryId;

          // Flatten alternating field/value pairs into a plain object.
          const fields: Record<string, string> = {};
          for (let i = 0; i + 1 < rawFields.length; i += 2) {
            fields[rawFields[i]] = rawFields[i + 1];
          }

          let update: PointUpdate;
          try {
            update = parseUpdate(fields);
          } catch (err) {
            this.onParseError(fields, err);
            continue;
          }

          const events = this.engine.ingest([update]);
          for (const ev of events) {
            const kvPairs = Object.entries(ev).flatMap(([k, v]) => [
              k,
              String(v ?? ""),
            ]);
            await this.client.xadd(this.outputStream, "*", ...kvPairs);
          }
        }
      }
    }
  }
}

function parseUpdate(fields: Record<string, string>): PointUpdate {
  const { id, x, y, t_ms } = fields;
  if (!id || x === undefined || y === undefined || t_ms === undefined) {
    throw new Error(`Missing required fields. Got: ${JSON.stringify(fields)}`);
  }
  const xn = Number(x);
  const yn = Number(y);
  const tMs = Number(t_ms);
  if (!isFinite(xn) || !isFinite(yn) || !isFinite(tMs)) {
    throw new Error(
      `Non-numeric coordinate or timestamp in: ${JSON.stringify(fields)}`,
    );
  }
  return { id, x: xn, y: yn, tMs };
}
