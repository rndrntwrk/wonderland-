import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { fork } from "node:child_process";
import { createServer } from "node:http";
import { once } from "node:events";
import { mkdtemp, writeFile, statfs } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
const root = await mkdtemp(join(tmpdir(), "wonderland-purchase-"));
const fs = await statfs(root);
assert.ok(
  fs.bavail * fs.bsize > 128 * 1024 * 1024,
  "focused test needs 128 MiB available; no build/install",
);
let worker, url, barrierReached;
const sockets = new Set();
const barrier = createServer(() => barrierReached?.());
barrier.on("connection", (s) => {
  sockets.add(s);
  s.on("close", () => sockets.delete(s));
});
barrier.listen(0, "127.0.0.1");
await once(barrier, "listening");
const deadline = (p) =>
  Promise.race([
    p,
    new Promise((_, reject) => {
      const t = setTimeout(() => reject(Error("test deadline")), 15000);
      t.unref();
    }),
  ]);
async function start() {
  worker = fork(
    "tests/purchase-worker.mjs",
    [root, `http://127.0.0.1:${barrier.address().port}/`],
    { detached: true, stdio: ["ignore", "ignore", "pipe", "ipc"] },
  );
  let error = "";
  worker.stderr.on("data", (x) => (error += x));
  const r = await deadline(
    new Promise((resolve, reject) => {
      worker.once("message", resolve);
      worker.once("exit", (c) =>
        reject(Error("worker exit " + c + " " + error)),
      );
    }),
  );
  url = r.url;
}
async function call(target, op, args = [], crash) {
  const r = await fetch(url, {
    method: "POST",
    body: JSON.stringify({ target, op, args, crash }),
    signal: AbortSignal.timeout(15000),
  });
  const value = await r.json();
  if (!r.ok) throw Error(value.error);
  return value;
}
const a = (op, args = []) => call("account", op, args),
  l = (op, args = []) => call("lot", op, args);
async function stop(crash = false) {
  if (!worker) return;
  const p = worker;
  worker = null;
  const exit = once(p, "exit");
  if (crash) process.kill(-p.pid, "SIGKILL");
  else p.send("close");
  await deadline(exit);
  for (const s of sockets) s.destroy();
}
const release = "b".repeat(64),
  permission = { revision: "1", sha256: "d".repeat(64) },
  binding = {
    protocol: 1,
    source_pin: "4c6b3e8f5835b228723caea3c9f683c62f244f73",
    binding_sha256: release,
    lot_incarnation: "1",
  };
const canonical = (v) =>
  v === null || typeof v !== "object"
    ? JSON.stringify(v)
    : Array.isArray(v)
      ? "[" + v.map(canonical).join(",") + "]"
      : "{" +
        Object.entries(v)
          .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
          .map(([k, v]) => JSON.stringify(k) + ":" + canonical(v))
          .join(",") +
        "}";
const sha = (v) => createHash("sha256").update(v).digest("hex");
let batch = 0;
const owners = [],
  checks = [];
let result = { passed: false, checks };
// Fixed-width bincode layout from native-lot-host/events.rs and sim-core DurableBuildRequest.
// Test-authored wire input, not an actual native producer or rendered purchase.
function wire(owner, op, cost, patch = {}) {
  const v = {
    operation: op,
    actor: { object_id: Number(owner.e.avatar_id) * 2 - 1, generation: 1 },
    owner: Number(owner.e.avatar_id),
    preview_hash: "11".repeat(32),
    cost,
    permissions_revision: "1",
    account_revision: owner.e.account_revision,
    catalog_revision: "7",
    entity: { object_id: Number(owner.e.avatar_id) * 2, generation: 1 },
    catalog_id: 44,
    ...patch,
  };
  const parts = [],
    u64 = (x) => {
      const b = Buffer.alloc(8);
      b.writeBigUInt64LE(BigInt(x));
      parts.push(b);
    },
    u32 = (x) => {
      const b = Buffer.alloc(4);
      b.writeUInt32LE(Number(x));
      parts.push(b);
    },
    entity = (x) => {
      const b = Buffer.alloc(2);
      b.writeInt16LE(x.object_id);
      parts.push(b);
      u32(x.generation);
    };
  u64(1);
  u64(batch + 2);
  u32(0);
  u32(0);
  u64(v.operation);
  entity(v.actor);
  u32(v.owner);
  parts.push(Buffer.from(v.preview_hash, "hex"));
  const c = Buffer.alloc(8);
  c.writeBigInt64LE(BigInt(v.cost));
  parts.push(c);
  u64(v.permissions_revision);
  u64(v.account_revision);
  u64(v.catalog_revision);
  u64(1);
  entity(v.entity);
  u32(v.catalog_id);
  u32(v.owner);
  return Buffer.concat(parts).toString("hex");
}
async function journal(owner, op, cost, patch = {}, incarnation = "1") {
  const hex = wire(owner, op, cost, patch);
  batch++;
  const e = owner.e,
    actor = {
      principal: e.avatar_id,
      account_id: e.account_id,
      avatar_id: e.avatar_id,
      actor: { object_id: Number(e.avatar_id) * 2 - 1, generation: 1 },
      control_generation: e.control_generation,
      session_ref: e.session_ref,
      admission_operation: e.admission_operation,
      reservation_digest: e.reservation_digest,
      export_digest: e.export_digest,
    };
  const cur = (t) => ({
    lot_id: "41",
    authority_epoch: incarnation === "2" ? "2" : "1",
    completed_tick: String(t),
    state_hash: "a".repeat(64),
  });
  return l("fixtureJournal", [
    {
      binding: { ...binding, lot_incarnation: incarnation },
      batch_id: String(batch),
      predecessor: cur(batch),
      result: cur(batch + 1),
      ticks_hex: "00",
      checkpoint_sha256: "0".repeat(64),
      effects_hex: "",
      events_hex: hex,
      control_v2: {
        schema_version: 2,
        revision_before: "1",
        revision_after: "1",
        actor_registry_before_sha256: "a".repeat(64),
        actor_registry_after_sha256: sha(canonical([actor])),
        actors: [actor],
        permission_before: permission,
        permission_after: permission,
      },
    },
  ]);
}
async function owner(index, budget = "100") {
  const username = "purchase_owner_" + index,
    password = "local-purchase-only-passphrase";
  await a("register", [username, password, "e".repeat(64)]);
  const login = await a("login", [username, password, "e".repeat(64)]);
  const spec = {
    name: "Purchase Owner " + ["One", "Two", "Three", "Four", "Five"][index],
    description: "",
    gender: "female",
    skin: "dark",
    head_key: "1",
    body_key: "2",
    release,
  };
  const avatar = await a("createAvatar", [
    login.token,
    "purchase_create_op_" + index,
    spec,
  ]);
  await a("testBalance", [avatar.avatar_id, budget]);
  const operation = "purchase_control_op_" + index;
  await a("reserveAvatarControl", [
    login.token,
    operation,
    {
      avatar_id: avatar.avatar_id,
      lot_id: "41",
      content_release: release,
      protocol_release: "c".repeat(64),
    },
  ]);
  const e = await a("exportReservedAvatarV2", [
    login.token,
    operation,
    avatar.avatar_id,
    permission,
  ]);
  return { login, e, operation };
}
try {
  await start();
  await a("initializeCatalog", [
    {
      release,
      entries: [
        { key: "1", part: "head", gender: "female", skins: ["dark"] },
        { key: "2", part: "body", gender: "female", skins: ["dark"] },
      ],
    },
  ]);
  await a("initializeAdmissionAuthorityV2", ["f".repeat(64)]);
  owners.push(await owner(0));
  const ref = await journal(owners[0], "91", "25");
  await assert.rejects(
    a("applyCommittedPurchaseV1", [ref]),
    /allocator intake required/,
  );
  const intake = {
    schema_version: 1,
    allocator_scope: "9".repeat(64),
    source_high_water: "16777215",
    source_receipt_sha256: "8".repeat(64),
    existing_owned_objects: 0,
  };
  await a("initializeEmptyPurchaseInventoryV1", [intake]);
  await assert.rejects(
    a("initializeEmptyPurchaseInventoryV1", [
      { ...intake, source_high_water: "16777216" },
    ]),
    /already pinned/,
  );
  const receipt = await a("applyCommittedPurchaseV1", [ref]);
  assert.equal(receipt.status, "account_committed_pending_geometry");
  assert.equal(receipt.balance_after, "75");
  assert.equal(
    receipt.confirmation.created_objects[0].persistent_id,
    "16777216",
  );
  checks.push(
    "committed source-format journal request atomically debits existing balance, creates one owned object identity and retains pending-geometry receipt",
  );
  const retries = await Promise.all(
    Array.from({ length: 8 }, () => a("applyCommittedPurchaseV1", [ref])),
  );
  assert.ok(
    retries.every((r) => JSON.stringify(r) === JSON.stringify(receipt)),
  );
  let snap = await a("purchaseSnapshot");
  assert.equal(snap.objects.length, 1);
  assert.equal(snap.receipts.length, 1);
  assert.equal(snap.balances[0].balance, "75");
  checks.push(
    "parallel exact retries return the same receipt and identity with one debit",
  );
  assert.deepEqual(
    await a("applyCommittedPurchaseV1", [
      await journal(owners[0], "91", "25", {}, "2"),
    ]),
    receipt,
  );
  checks.push(
    "same source operation re-emitted under a later lot incarnation/epoch retains the original financial decision",
  );
  await assert.rejects(a("applyCommittedPurchaseV1", [{ ...ref, cost: "1" }]));
  await assert.rejects(
    a("applyCommittedPurchaseV1", [{ ...ref, digest: "0".repeat(64) }]),
  );
  await assert.rejects(
    a("applyCommittedPurchaseV1", [await journal(owners[0], "91", "26")]),
    /identity reused/,
  );
  await assert.rejects(
    a("exportReservedAvatarV2", [
      owners[0].login.token,
      owners[0].operation,
      owners[0].e.avatar_id,
      permission,
    ]),
    /source revision changed/,
  );
  checks.push(
    "caller price injection, changed journal and changed same-operation bytes reject; old frozen admission export cannot silently continue after debit",
  );
  owners.push(await owner(1));
  await a("testBalance", [owners[1].e.avatar_id, "10"]);
  const low = await journal(owners[1], "92", "25");
  const rejected = await a("applyCommittedPurchaseV1", [low]);
  assert.equal(rejected.status, "rejected");
  assert.equal(rejected.confirmation.charged_cost, "0");
  assert.deepEqual(rejected.confirmation.created_objects, []);
  await a("testBalance", [owners[1].e.avatar_id, "100"]);
  assert.deepEqual(await a("applyCommittedPurchaseV1", [low]), rejected);
  checks.push(
    "insufficient saved balance rejects with zero charge/object; exact rejection persists after later fixture funding",
  );
  owners.push(await owner(2));
  const unchanged = await a("purchaseSnapshot");
  for (const patch of [
    { owner: 0 },
    { actor: { object_id: 100, generation: 1 } },
    { cost: "-1" },
    { catalog_id: 0 },
    { permissions_revision: "2" },
    { account_revision: "999" },
  ])
    await assert.rejects(
      a("applyCommittedPurchaseV1", [
        await journal(owners[2], "200", "1", patch),
      ]),
    );
  await assert.rejects(
    a("applyCommittedPurchaseV1", [{ ...ref, batch_key: "1:9999" }]),
  );
  assert.deepEqual(await a("purchaseSnapshot"), unchanged);
  checks.push(
    "uncommitted source, wrong owner/actor/revisions, negative charge and invalid catalog identity fail without debit, inventory or receipt",
  );
  const rollback = await journal(owners[2], "93", "30");
  const before = await a("purchaseSnapshot");
  await a("purchaseFault", [true]);
  await assert.rejects(
    a("applyCommittedPurchaseV1", [rollback]),
    /test purchase receipt failure/,
  );
  await a("purchaseFault", [false]);
  assert.deepEqual(await a("purchaseSnapshot"), before);
  await a("applyCommittedPurchaseV1", [rollback]);
  checks.push(
    "late receipt SQL failure rolls back debit, owned row, identity allocation and journal receipt together",
  );
  for (const [index, point] of [
    [3, "before"],
    [4, "after"],
  ]) {
    owners.push(await owner(index));
    const r = await journal(owners[index], String(91 + index), "40");
    let reached;
    const barrierWait = new Promise((resolve) => (reached = resolve));
    barrierReached = reached;
    const pending = call(
      "account",
      "applyCommittedPurchaseV1",
      [r],
      point,
    ).then(
      () => {
        throw Error("unexpected response before crash");
      },
      () => null,
    );
    await deadline(barrierWait);
    await stop(true);
    await pending;
    barrierReached = null;
    await start();
    const recovered = await a("applyCommittedPurchaseV1", [r]);
    assert.equal(recovered.balance_after, "60");
    assert.deepEqual(await a("applyCommittedPurchaseV1", [r]), recovered);
  }
  checks.push(
    "real test-owned supervisor/workerd SIGKILL before commit and after commit/before reply recovers exact operation with one debit/object; no process inspection",
  );
  snap = await a("purchaseSnapshot");
  assert.equal(snap.objects.length, 4);
  assert.equal(snap.receipts.length, 5);
  assert.equal(new Set(snap.objects.map((o) => o.persistent_id)).size, 4);
  await a("logout", [owners[0].login.token]);
  assert.deepEqual(await a("applyCommittedPurchaseV1", [ref]), receipt);
  await assert.rejects(
    a("applyCommittedPurchaseV1", [await journal(owners[0], "999", "1")]),
  );
  checks.push(
    "revocation preserves exact prior financial delivery but blocks a new operation",
  );
  const publicDenied = await fetch(url + "native-account/purchase", {
    method: "POST",
    headers: { origin: "https://test", "content-type": "application/json" },
    body: "{}",
  });
  assert.equal(publicDenied.status, 403);
  result.passed = true;
  result.objects = snap.objects.length;
  result.receipts = snap.receipts.length;
} catch (e) {
  result.failure = String(e.stack ?? e);
  process.exitCode = 1;
} finally {
  await stop().catch(() => {});
  for (const s of sockets) s.destroy();
  barrier.close();
  await writeFile(join(root, "result.json"), JSON.stringify(result, null, 2));
  console.log(JSON.stringify({ ...result, evidenceRoot: root }));
}
