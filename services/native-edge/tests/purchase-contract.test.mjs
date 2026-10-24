import assert from "node:assert/strict";
import test from "node:test";
import { build } from "esbuild";
import { dispatchSupervisorRpc } from "../src/hosted/supervisor-rpc.ts";

const bundle = await build({
  entryPoints: ["src/account/purchases.ts"],
  bundle: true,
  write: false,
  format: "esm",
  platform: "browser",
});
const { decodePurchaseEvents, purchaseReference, configuredPurchaseReader } =
  await import(
    "data:text/javascript;base64," +
      Buffer.from(bundle.outputFiles[0].text).toString("base64")
  );
function event(tick = 2, ordinal = 0, tag = 0) {
  const parts = [];
  const u64 = (v) => {
    const b = Buffer.alloc(8);
    b.writeBigUInt64LE(BigInt(v));
    parts.push(b);
  };
  const u32 = (v) => {
    const b = Buffer.alloc(4);
    b.writeUInt32LE(v);
    parts.push(b);
  };
  const entity = (id) => {
    const b = Buffer.alloc(2);
    b.writeInt16LE(id);
    parts.push(b);
    u32(1);
  };
  u64(tick);
  u32(ordinal);
  u32(tag);
  u64(91);
  entity(1);
  u32(1);
  parts.push(Buffer.alloc(32, 17));
  u64(25);
  u64(1);
  u64(1);
  u64(7);
  u64(1);
  entity(2);
  u32(44);
  u32(1);
  return Buffer.concat(parts);
}
function lane(events = [event()]) {
  const count = Buffer.alloc(8);
  count.writeBigUInt64LE(BigInt(events.length));
  return Buffer.concat([count, ...events]);
}
const reference = {
  lot_id: "41",
  batch_key: "1:2",
  digest: "a".repeat(64),
  tick: "2",
  ordinal: 0,
};
test("reference accepts journal identity only with canonical exact integers", () => {
  assert.deepEqual(purchaseReference(reference), reference);
  for (const patch of [
    { cost: "0" },
    { lot_id: "041" },
    { batch_key: "01:2" },
    { batch_key: "1:0" },
    { tick: "0" },
    { ordinal: 0x100000000 },
    { ordinal: -1 },
    { digest: "x".repeat(64) },
  ])
    assert.throws(() => purchaseReference({ ...reference, ...patch }));
});
test("source-format single-object lane preserves exact request and entity identities", () => {
  const decoded = decodePurchaseEvents(lane().toString("hex"));
  assert.deepEqual(decoded, [
    {
      tick: "2",
      ordinal: 0,
      request: {
        operation: "91",
        actor: { object_id: 1, generation: 1 },
        owner: "1",
        preview_hash: "11".repeat(32),
        cost: "25",
        permissions_revision: "1",
        account_revision: "1",
        catalog_revision: "7",
        purchases: [
          {
            entity: { object_id: 2, generation: 1 },
            catalog_id: 44,
            owner: "1",
          },
        ],
      },
    },
  ]);
});
test("every truncated prefix and trailing byte rejects instead of partial consumption", () => {
  const bytes = lane();
  for (let n = 0; n < bytes.length; n++)
    assert.throws(() =>
      decodePurchaseEvents(bytes.subarray(0, n).toString("hex")),
    );
  assert.throws(() =>
    decodePurchaseEvents(
      Buffer.concat([bytes, Buffer.from([0])]).toString("hex"),
    ),
  );
});
test("unknown/mixed lanes, duplicate/out-of-order events and capacity excess fail closed", () => {
  for (const events of [
    [event(2, 0, 1)],
    [event(), event(3, 0, 1)],
    [event(), event()],
    [event(3), event(2)],
  ])
    assert.throws(() => decodePurchaseEvents(lane(events).toString("hex")));
  const tooMany = Buffer.alloc(8);
  tooMany.writeBigUInt64LE(129n);
  assert.throws(() => decodePurchaseEvents(tooMany.toString("hex")));
  assert.throws(() => decodePurchaseEvents("00".repeat(256 * 1024 + 1)));
  assert.equal(
    decodePurchaseEvents(lane([event(), event(2, 1)]).toString("hex")).length,
    2,
  );
});
test("journal access has no caller-provided payload or absent-binding fallback", async () => {
  await assert.rejects(
    configuredPurchaseReader({})(reference),
    /journal unavailable/,
  );
});
test("both new binding methods remain unavailable through the supervisor RPC boundary", async () => {
  let calls = 0;
  for (const method of [
    "initializeEmptyPurchaseInventoryV1",
    "applyCommittedPurchaseV1",
  ]) {
    const response = await dispatchSupervisorRpc(
      { target: "account", method, args: [{}] },
      async () => {
        calls++;
        return {};
      },
    );
    assert.equal(response.status, 400);
  }
  assert.equal(calls, 0);
});
