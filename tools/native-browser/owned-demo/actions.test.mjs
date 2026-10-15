import assert from "node:assert/strict";
import { test } from "node:test";
import { settlePlayerAction } from "./actions.mjs";
test("ordinary native denied action returns its rejection receipt without committing or disconnecting", async () => {
  const emitted = [];
  const denied = { kind: "denied", receipt_hex: "native-rejection-bytes" };
  assert.equal(
    await settlePlayerAction(denied, {
      commit: () => {
        throw Error("must not commit denial");
      },
      receipt: (hex) => emitted.push(hex),
    }),
    denied,
  );
  assert.deepEqual(emitted, ["native-rejection-bytes"]);
});
test("invalid undecodable native action closes without fabricating a receipt", async () => {
  await assert.rejects(
    settlePlayerAction(
      { kind: "denied" },
      { commit: () => assert.fail(), receipt: () => assert.fail() },
    ),
    /decoded/,
  );
});
test("proposal receipt is published only after its durable decision returns", async () => {
  let resolve;
  const decision = new Promise((r) => (resolve = r)),
    emitted = [];
  const pending = settlePlayerAction(
    { kind: "proposal" },
    { commit: () => decision, receipt: (v) => emitted.push(v) },
  );
  assert.deepEqual(emitted, []);
  resolve({ kind: "rejected", receipt_hex: "durable-rejection-bytes" });
  await pending;
  assert.deepEqual(emitted, ["durable-rejection-bytes"]);
});

test("exact historical terminal action reply redelivers correlation without recommitting", async () => {
  for (const kind of ["committed", "rejected"]) {
    const emitted = [],
      reply = { kind, receipt_hex: "persisted-exact-action-receipt" };
    assert.equal(
      await settlePlayerAction(reply, {
        commit: () => assert.fail(),
        receipt: (hex) => emitted.push(hex),
      }),
      reply,
    );
    assert.deepEqual(emitted, [reply.receipt_hex]);
  }
});
