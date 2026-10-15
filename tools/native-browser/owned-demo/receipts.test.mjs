import assert from "node:assert/strict";
import { test } from "node:test";
import { createHash } from "node:crypto";
import {
  askWithDurableReceipts,
  retireCheckpointReceipts,
} from "./receipts.mjs";

const binding = {
  protocol: 1,
  source_pin: "test",
  binding_sha256: "a".repeat(64),
  lot_incarnation: "7",
};
const cursor = {
  lot_id: "41",
  authority_epoch: "7",
  completed_tick: "32",
  state_hash: "b".repeat(64),
};
const manifest = { binding, cursor, sha256: "c".repeat(64) };
const canonical = (value) => {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  return (
    "{" +
    Object.keys(value)
      .sort()
      .map((key) => JSON.stringify(key) + ":" + canonical(value[key]))
      .join(",") +
    "}"
  );
};
function exported(records, through = "32") {
  const payload = {
    schema_version: 1,
    binding,
    cursor,
    checkpoint_sha256: manifest.sha256,
    through_batch_id: through,
    records,
  };
  return {
    kind: "receipt_export",
    payload,
    digest: createHash("sha256").update(canonical(payload)).digest("hex"),
  };
}
function acknowledgement(value) {
  const { binding, cursor, checkpoint_sha256, through_batch_id } =
    value.payload;
  return {
    schema_version: 1,
    status: "persisted",
    binding,
    cursor,
    checkpoint_sha256,
    through_batch_id,
    receipt_set_sha256: value.digest,
  };
}
const record = {
  batch_id: "32",
  input_sha256: "d".repeat(64),
  digest: "e".repeat(64),
  reply: {
    kind: "rejected",
    batch_id: "32",
    digest: "e".repeat(64),
    cursor,
    receipt_hex: "exact-action-rejection-correlation",
  },
};

test("checkpoint custody transfer acknowledges only after exact durable persistence and drains pages", async () => {
  const first = exported([record]),
    second = exported([{ ...record, batch_id: "33" }], "33");
  let page = 0,
    persisted = 0;
  const calls = [];
  const host = {
    async ask(request) {
      calls.push(request.kind);
      if (request.kind === "export_receipts")
        return [first, second, exported([], "33")][page];
      assert.equal(persisted, page + 1);
      assert.deepEqual(
        request.acknowledgement,
        acknowledgement([first, second][page++]),
      );
      return {
        kind: "receipts_retired",
        acknowledgement: request.acknowledgement,
      };
    },
  };
  await retireCheckpointReceipts(
    host,
    async (method, args) => {
      assert.equal(method, "persistHostReceiptsV1");
      assert.deepEqual(args, [
        [first, second][page].payload,
        [first, second][page].digest,
      ]);
      persisted++;
      return acknowledgement([first, second][page]);
    },
    manifest,
  );
  assert.equal(page, 2);
  assert.deepEqual(calls, [
    "export_receipts",
    "acknowledge_receipts",
    "export_receipts",
    "acknowledge_receipts",
    "export_receipts",
  ]);
});

test("failed persistence or altered acknowledgement never retires host correlations", async () => {
  const page = exported([record]);
  for (const persist of [
    async () => {
      throw Error("R2 readback failed");
    },
    async () => ({
      ...acknowledgement(page),
      receipt_set_sha256: "0".repeat(64),
    }),
  ]) {
    let requests = 0;
    const host = {
      async ask(request) {
        requests++;
        assert.equal(request.kind, "export_receipts");
        return page;
      },
    };
    await assert.rejects(retireCheckpointReceipts(host, persist, manifest));
    assert.equal(requests, 1);
  }
});

test("stale checkpoint and changed receipt bytes cannot authorize custody transfer", async () => {
  for (const page of [
    exported([record]),
    { ...exported([record]), digest: "0".repeat(64) },
  ]) {
    await assert.rejects(
      retireCheckpointReceipts({ ask: async () => page }, () => assert.fail(), {
        ...manifest,
        ...(page.digest[0] === "0"
          ? {}
          : { cursor: { ...cursor, completed_tick: "31" } }),
      }),
    );
  }
});

test("retired same-input action retrieves exact rejected correlation without executing again", async () => {
  const request = { kind: "prepare_player_action", batch_id: "32", binding };
  let asks = 0;
  const result = await askWithDurableReceipts(
    {
      ask: async () => {
        asks++;
        return {
          kind: "retired",
          binding,
          batch_id: "32",
          input_sha256: record.input_sha256,
        };
      },
    },
    async (method, args) => {
      assert.equal(method, "lookupHostReceiptV1");
      assert.deepEqual(args, ["7", "32"]);
      return { status: "recorded", record };
    },
    request,
  );
  assert.deepEqual(result, record.reply);
  assert.equal(asks, 1);
});

test("reused retired input, digest, and missing durable records all fail closed", async () => {
  const host = {
    ask: async (request) => ({
      kind: "retired",
      binding,
      batch_id: "32",
      input_sha256: "f".repeat(64),
      digest: request.digest,
    }),
  };
  const lot = async () => ({ status: "recorded", record });
  await assert.rejects(
    askWithDurableReceipts(host, lot, {
      kind: "prepare_player_tick",
      batch_id: "32",
      binding,
    }),
    /input differs/,
  );
  await assert.rejects(
    askWithDurableReceipts(host, lot, {
      kind: "resolve",
      batch_id: "32",
      digest: "f".repeat(64),
    }),
    /digest differs/,
  );
  await assert.rejects(
    askWithDurableReceipts(host, async () => ({ status: "unknown" }), {
      kind: "lookup",
      batch_id: "32",
    }),
    /refusing to execute again/,
  );
});

test("ordinary host proposal remains unmodified and performs no durable lookup", async () => {
  const proposal = { kind: "proposal", digest: "e".repeat(64) };
  assert.equal(
    await askWithDurableReceipts(
      { ask: async () => proposal },
      () => assert.fail(),
      { kind: "prepare_player_tick" },
    ),
    proposal,
  );
});
