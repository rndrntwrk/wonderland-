import assert from "node:assert/strict";
import { test } from "node:test";
import { createHash } from "node:crypto";
import { recoverPlayerAction } from "./action-results.mjs";
const action = "574c43310d0a1a0a0100"; // Boundary bytes only; Rust decodes real WLC1.
const entry = {
  control: {
    principal: "7",
    avatar_id: "9",
    control_generation: "3",
    session_ref: "a".repeat(64),
  },
  export: { account_id: "5" },
};
const current = {
  binding: {
    protocol: 1,
    source_pin: "p",
    binding_sha256: "b".repeat(64),
    lot_incarnation: "2",
  },
  cursor: { lot_id: "41", completed_tick: "100" },
};
const query = {
  type: "native_action_result",
  lot_incarnation: "1",
  action_hex: action,
};
const control = { ...entry.control, account_id: "5" };
const evidence = {
  schema_version: 1,
  status: "committed",
  control,
  request_hex: action,
  request_sha256: createHash("sha256")
    .update(Buffer.from(action, "hex"))
    .digest("hex"),
  receipt_hex: "574c41310d0a1a0a0100",
  binding: { ...current.binding, lot_incarnation: "1" },
  batch_id: "4",
  digest: "c".repeat(64),
  cursor: { lot_id: "41", completed_tick: "42" },
};
test("read-only recovery derives authority from authenticated entry and returns exact durable receipt", async () => {
  const emitted = [];
  let lookups = 0;
  await recoverPlayerAction(query, entry, current, {
    lookup: async (request) => {
      lookups++;
      assert.deepEqual(request, {
        schema_version: 1,
        control,
        lot_incarnation: "1",
        request_hex: action,
      });
      return evidence;
    },
    receipt: (value) => emitted.push(value),
  });
  assert.equal(lookups, 1);
  assert.deepEqual(emitted, [evidence.receipt_hex]);
});
test("missing decision leaves unknown without emitting rejection or replay", async () => {
  await recoverPlayerAction(query, entry, current, {
    lookup: async () => ({ schema_version: 1, status: "unknown" }),
    receipt: () => assert.fail("no fabricated result"),
  });
});
test("browser cannot supply another owner, future incarnation, malformed or oversized action", async () => {
  for (const bad of [
    { ...query, control },
    { ...query, lot_incarnation: "3" },
    { ...query, lot_incarnation: "01" },
    { ...query, action_hex: action.toUpperCase() },
    { ...query, action_hex: "00" },
    { ...query, action_hex: "57".repeat(16385) },
  ]) {
    await assert.rejects(
      recoverPlayerAction(bad, entry, current, {
        lookup: () => assert.fail("no lookup"),
        receipt: () => assert.fail(),
      }),
    );
  }
});
test("mismatched owner, action, old binding or future accepted state cannot be shown", async () => {
  for (const bad of [
    { ...evidence, control: { ...control, account_id: "6" } },
    { ...evidence, request_hex: action + "00" },
    { ...evidence, request_sha256: "d".repeat(64) },
    { ...evidence, binding: { ...evidence.binding, lot_incarnation: "2" } },
    { ...evidence, cursor: { ...evidence.cursor, lot_id: "42" } },
    { ...evidence, cursor: { ...evidence.cursor, completed_tick: "101" } },
    { ...evidence, receipt_hex: "00" },
  ]) {
    await assert.rejects(
      recoverPlayerAction(query, entry, current, {
        lookup: async () => bad,
        receipt: () => assert.fail("no unverified result"),
      }),
    );
  }
});
test("durable rejection uses its exact receipt and never renews activity", async () => {
  let emitted;
  await recoverPlayerAction(query, entry, current, {
    lookup: async () => ({ ...evidence, status: "rejected" }),
    receipt: (x) => (emitted = x),
  });
  assert.equal(emitted, evidence.receipt_hex);
});

test("authenticated socket route enforces security fences and authorization before lookup without renewal", async () => {
  const { reconcileAuthenticatedPlayerAction } = await import(
    "./action-results.mjs"
  );
  const order = [];
  await reconcileAuthenticatedPlayerAction(query, entry, {
    sweepFences: async () => order.push("security-fences"),
    revalidate: async (value) => {
      assert.equal(value, entry);
      order.push("authorize-no-touch");
      return current;
    },
    lookup: async () => {
      order.push("lookup");
      return evidence;
    },
    receipt: () => order.push("receipt"),
  });
  assert.deepEqual(order, [
    "security-fences",
    "authorize-no-touch",
    "lookup",
    "receipt",
  ]);
});
test("expired or revoked authenticated socket cannot look up historical receipts", async () => {
  const { reconcileAuthenticatedPlayerAction } = await import(
    "./action-results.mjs"
  );
  for (const deniedAt of ["fence", "authorization"]) {
    await assert.rejects(
      reconcileAuthenticatedPlayerAction(query, entry, {
        sweepFences: async () => {
          if (deniedAt === "fence") throw Error("fenced");
        },
        revalidate: async () => {
          throw Error("session expired");
        },
        lookup: () => assert.fail("must not disclose"),
        receipt: () => assert.fail("must not disclose"),
      }),
    );
  }
});
