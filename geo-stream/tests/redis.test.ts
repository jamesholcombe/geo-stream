import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { GeoStreamRedis } from "../dist/redis.js";
import type { RedisStreamClient } from "../dist/redis.js";
import type { GeoEngine, GeoEvent, PointUpdate } from "../dist/types.js";

// ---------------------------------------------------------------------------
// Mock helpers
// ---------------------------------------------------------------------------

type XreadReply = Array<[string, Array<[string, string[]]>]> | null;

/**
 * Creates a mock Redis client that returns one reply per call.
 * `replies` is consumed in order; when exhausted the client stops the adapter.
 */
interface MockRedisClient extends RedisStreamClient {
  xreadArgs: Array<[number, number, string, string]>;
  xaddCalls: Array<{ stream: string; fields: Record<string, string> }>;
}

function makeClient(
  replies: XreadReply[],
  stopOnExhaust?: GeoStreamRedis,
): MockRedisClient {
  const xreadArgs: Array<[number, number, string, string]> = [];
  const xaddCalls: Array<{ stream: string; fields: Record<string, string> }> =
    [];
  let callIndex = 0;

  return {
    xreadArgs,
    xaddCalls,
    async xread(
      _countArg,
      count,
      _blockArg,
      blockMs,
      _streamsArg,
      stream,
      lastId,
    ) {
      xreadArgs.push([count, blockMs, stream, lastId]);
      const reply = replies[callIndex++] ?? null;
      if (callIndex >= replies.length && stopOnExhaust) {
        stopOnExhaust.stop();
      }
      return reply;
    },
    async xadd(stream, _id, ...fieldValues) {
      const fields: Record<string, string> = {};
      for (let i = 0; i + 1 < fieldValues.length; i += 2) {
        fields[fieldValues[i]] = fieldValues[i + 1];
      }
      xaddCalls.push({ stream, fields });
      return "1-0";
    },
  };
}

function makeEngine(
  events: GeoEvent[] = [],
): GeoEngine & { ingestCalls: PointUpdate[][] } {
  const ingestCalls: PointUpdate[][] = [];
  return {
    ingestCalls,
    registerZone() {},
    registerCatalogRegion() {},
    registerCircle() {},
    ingest(updates: PointUpdate[]) {
      ingestCalls.push(updates);
      return events;
    },
  } as unknown as GeoEngine & { ingestCalls: PointUpdate[][] };
}

/** Build a well-formed XREAD reply with one entry. */
function reply(
  entryId: string,
  fields: Record<string, string>,
  streamKey = "location-updates",
): XreadReply {
  const flatFields = Object.entries(fields).flatMap(([k, v]) => [k, v]);
  return [[streamKey, [[entryId, flatFields]]]];
}

const VALID_FIELDS = { id: "v1", x: "1.5", y: "2.5", t_ms: "1700000000000" };
const INPUT_STREAM = "location-updates";
const OUTPUT_STREAM = "geo-events";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("GeoStreamRedis — XREAD arguments", () => {
  it("calls xread with correct COUNT / BLOCK / STREAMS / lastId", async () => {
    let adapter!: GeoStreamRedis;
    const client = makeClient([null], undefined);
    adapter = new GeoStreamRedis(makeEngine(), {
      client,
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
    });
    const stopClient = makeClient([null], adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    const [count, blockMs, stream, lastId] = stopClient.xreadArgs[0];
    assert.equal(count, 100); // default batchSize
    assert.equal(blockMs, 1000); // default blockMs
    assert.equal(stream, INPUT_STREAM);
    assert.equal(lastId, "$"); // default startId
  });

  it("respects batchSize and blockMs options", async () => {
    let adapter!: GeoStreamRedis;
    const client = makeClient([null]);
    adapter = new GeoStreamRedis(makeEngine(), {
      client,
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
      batchSize: 50,
      blockMs: 500,
    });
    const stopClient = makeClient([null], adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    const [count, blockMs] = stopClient.xreadArgs[0];
    assert.equal(count, 50);
    assert.equal(blockMs, 500);
  });

  it("respects startId option", async () => {
    let adapter!: GeoStreamRedis;
    const client = makeClient([null]);
    adapter = new GeoStreamRedis(makeEngine(), {
      client,
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
      startId: "0",
    });
    const stopClient = makeClient([null], adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    const [, , , lastId] = stopClient.xreadArgs[0];
    assert.equal(lastId, "0");
  });
});

describe("GeoStreamRedis — message processing", () => {
  it("null xread reply (timeout) → loop continues without calling xadd", async () => {
    const event: GeoEvent = { kind: "enter", id: "v1", zone: "z1", t_ms: 1000 };
    let adapter!: GeoStreamRedis;
    // First reply null, second reply has an entry — stop after that
    const replies: XreadReply[] = [null, reply("1-1", VALID_FIELDS)];
    adapter = new GeoStreamRedis(makeEngine([event]), {
      client: makeClient([]),
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
    });
    const stopClient = makeClient(replies, adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    // xadd should have been called once (from the second reply, not the null)
    assert.equal(stopClient.xaddCalls.length, 1);
  });

  it("valid entry → engine.ingest called with correct PointUpdate", async () => {
    let adapter!: GeoStreamRedis;
    const engine = makeEngine([]);
    adapter = new GeoStreamRedis(engine, {
      client: makeClient([]),
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
    });
    const stopClient = makeClient([reply("1-1", VALID_FIELDS)], adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    assert.equal(engine.ingestCalls.length, 1);
    assert.deepEqual(engine.ingestCalls[0][0], {
      id: "v1",
      x: 1.5,
      y: 2.5,
      tMs: 1_700_000_000_000,
    });
  });

  it("valid entry with events → xadd called with serialised event fields", async () => {
    const event: GeoEvent = { kind: "enter", id: "v1", zone: "z1", t_ms: 1000 };
    let adapter!: GeoStreamRedis;
    adapter = new GeoStreamRedis(makeEngine([event]), {
      client: makeClient([]),
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
    });
    const stopClient = makeClient([reply("1-1", VALID_FIELDS)], adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    assert.equal(stopClient.xaddCalls.length, 1);
    assert.equal(stopClient.xaddCalls[0].stream, OUTPUT_STREAM);
    assert.equal(stopClient.xaddCalls[0].fields.kind, "enter");
    assert.equal(stopClient.xaddCalls[0].fields.zone, "z1");
  });

  it("engine returns no events → xadd not called", async () => {
    let adapter!: GeoStreamRedis;
    adapter = new GeoStreamRedis(makeEngine([]), {
      client: makeClient([]),
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
    });
    const stopClient = makeClient([reply("1-1", VALID_FIELDS)], adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    assert.equal(stopClient.xaddCalls.length, 0);
  });

  it("lastId advances to entry id after processing", async () => {
    const event: GeoEvent = { kind: "enter", id: "v1", zone: "z1", t_ms: 1000 };
    let adapter!: GeoStreamRedis;
    // Two entries in sequence; stop after both
    const replies: XreadReply[] = [
      reply("2-1", VALID_FIELDS),
      reply("3-5", VALID_FIELDS),
    ];
    adapter = new GeoStreamRedis(makeEngine([event]), {
      client: makeClient([]),
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
    });
    const stopClient = makeClient(replies, adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    // Second xread call should use the id from the first entry
    assert.equal(stopClient.xreadArgs[1][3], "2-1");
  });

  it("multiple entries in one batch all processed", async () => {
    const event: GeoEvent = { kind: "enter", id: "v1", zone: "z1", t_ms: 1000 };
    let adapter!: GeoStreamRedis;
    const flatFields = Object.entries(VALID_FIELDS).flatMap(([k, v]) => [k, v]);
    const batchReply: XreadReply = [
      [
        INPUT_STREAM,
        [
          ["4-1", flatFields],
          ["4-2", flatFields],
          ["4-3", flatFields],
        ],
      ],
    ];
    adapter = new GeoStreamRedis(makeEngine([event]), {
      client: makeClient([]),
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
    });
    const stopClient = makeClient([batchReply], adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    assert.equal(stopClient.xaddCalls.length, 3);
  });
});

describe("GeoStreamRedis — parse error handling", () => {
  it("entry missing required id field → onParseError called, no xadd", async () => {
    let adapter!: GeoStreamRedis;
    const errors: Array<Record<string, string>> = [];
    const badFields = { x: "1", y: "2", t_ms: "1000" }; // missing id
    adapter = new GeoStreamRedis(makeEngine(), {
      client: makeClient([]),
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
      onParseError: (fields) => errors.push(fields),
    });
    const stopClient = makeClient([reply("5-1", badFields)], adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    assert.equal(errors.length, 1);
    assert.deepEqual(errors[0], badFields);
  });

  it("entry with non-numeric x coordinate → onParseError called", async () => {
    let adapter!: GeoStreamRedis;
    const errors: Array<Record<string, string>> = [];
    const badFields = { id: "v1", x: "not-a-number", y: "2", t_ms: "1000" };
    adapter = new GeoStreamRedis(makeEngine(), {
      client: makeClient([]),
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
      onParseError: (fields) => errors.push(fields),
    });
    const stopClient = makeClient([reply("6-1", badFields)], adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    assert.equal(errors.length, 1);
  });

  it("no onParseError provided → parse error silently skipped", async () => {
    let adapter!: GeoStreamRedis;
    const badFields = { x: "1", y: "2", t_ms: "1000" }; // missing id
    adapter = new GeoStreamRedis(makeEngine(), {
      client: makeClient([]),
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
    });
    const stopClient = makeClient([reply("7-1", badFields)], adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    // Should not throw
    await assert.doesNotReject(() => adapter.start());
  });
});

describe("GeoStreamRedis — stop", () => {
  it("stop() halts the loop after the current xread returns", async () => {
    let adapter!: GeoStreamRedis;
    adapter = new GeoStreamRedis(makeEngine(), {
      client: makeClient([]),
      inputStream: INPUT_STREAM,
      outputStream: OUTPUT_STREAM,
    });
    // stop immediately after the first null reply
    const stopClient = makeClient([null], adapter);
    (adapter as unknown as { client: RedisStreamClient }).client = stopClient;
    await adapter.start();
    // Only one xread call should have been made
    assert.equal(stopClient.xreadArgs.length, 1);
  });
});
