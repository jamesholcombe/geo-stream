/**
 * Kafka adapter for GeoEngine.
 *
 * Consumes `PointUpdate` records from a Kafka input topic, processes them
 * through a `GeoEngine`, and publishes resulting `GeoEvent` records to an
 * output topic.
 *
 * Uses structural typing — no hard dependency on `kafkajs`. Any Kafka client
 * that satisfies the `KafkaConsumer` / `KafkaProducer` interfaces works.
 *
 * @example
 * ```ts
 * import { Kafka } from 'kafkajs'
 * import { GeoEngine } from '@jamesholcombe/geo-stream/types'
 * import { GeoStreamKafka } from '@jamesholcombe/geo-stream/kafka'
 *
 * const kafka = new Kafka({ brokers: ['localhost:9092'] })
 * const engine = new GeoEngine()
 * engine.registerZone('site', { type: 'Polygon', coordinates: [...] })
 *
 * const adapter = new GeoStreamKafka(engine, {
 *   consumer: kafka.consumer({ groupId: 'geo-stream' }),
 *   producer: kafka.producer(),
 *   inputTopic:  'location-updates',
 *   outputTopic: 'geo-events',
 * })
 *
 * await adapter.connect()
 * await adapter.start()
 * // ...later...
 * await adapter.stop()
 * ```
 *
 * Message format:
 *   Input  — JSON-encoded `PointUpdate`: `{ "id": "...", "x": 0, "y": 0, "tMs": 0 }`
 *   Output — JSON-encoded `GeoEvent`:    `{ "kind": "enter", "id": "...", ... }`
 */

import { GeoEngine } from "./types.js";
import type { PointUpdate, GeoEvent } from "./types.js";

// ---------------------------------------------------------------------------
// Minimal structural interfaces — no kafkajs import required
// ---------------------------------------------------------------------------

export interface KafkaProducer {
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  send(record: {
    topic: string;
    messages: Array<{ value: string }>;
  }): Promise<void>;
}

export interface KafkaMessage {
  value: Buffer | null;
}

export interface KafkaConsumer {
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  subscribe(opts: { topic: string; fromBeginning?: boolean }): Promise<void>;
  run(opts: {
    eachMessage: (payload: { message: KafkaMessage }) => Promise<void>;
  }): Promise<void>;
}

export interface GeoStreamKafkaOptions {
  consumer: KafkaConsumer;
  producer: KafkaProducer;
  inputTopic: string;
  outputTopic: string;
  /** Called when a message cannot be parsed as a PointUpdate. Defaults to a no-op. */
  onParseError?: (raw: string, err: unknown) => void;
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

export class GeoStreamKafka {
  private engine: GeoEngine;
  private opts: GeoStreamKafkaOptions;

  constructor(engine: GeoEngine, opts: GeoStreamKafkaOptions) {
    this.engine = engine;
    this.opts = opts;
  }

  /** Connect the underlying consumer and producer. Call before `start()`. */
  async connect(): Promise<void> {
    await this.opts.consumer.connect();
    await this.opts.producer.connect();
  }

  /**
   * Subscribe to the input topic and begin processing messages.
   * This method delegates to `consumer.run()` which runs until `stop()` is called.
   */
  async start(): Promise<void> {
    const { consumer, producer, inputTopic, outputTopic, onParseError } =
      this.opts;

    await consumer.subscribe({ topic: inputTopic, fromBeginning: false });

    await consumer.run({
      eachMessage: async ({ message }) => {
        if (!message.value) return;

        const raw = message.value.toString();
        let update: PointUpdate;
        try {
          update = JSON.parse(raw) as PointUpdate;
        } catch (err) {
          onParseError?.(raw, err);
          return;
        }

        const events = this.engine.ingest([update]);
        if (events.length === 0) return;

        await producer.send({
          topic: outputTopic,
          messages: events.map((ev: GeoEvent) => ({
            value: JSON.stringify(ev),
          })),
        });
      },
    });
  }

  /** Disconnect the consumer and producer. */
  async stop(): Promise<void> {
    await this.opts.consumer.disconnect();
    await this.opts.producer.disconnect();
  }
}
