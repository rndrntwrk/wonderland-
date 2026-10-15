import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { readFile, mkdtemp } from "node:fs/promises";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";
const root = await mkdtemp(join(tmpdir(), "wonderland-owned-control-"));
const bundled = await build({
  entryPoints: ["tests/owned-control-driver.ts"],
  bundle: true,
  format: "esm",
  platform: "neutral",
  external: ["cloudflare:workers"],
  write: false,
  plugins: [
    {
      name: "wasm",
      setup(b) {
        b.onResolve({ filter: /account_crypto_bg\.wasm$/ }, () => ({
          path: "./account_crypto_bg.wasm",
          external: true,
        }));
      },
    },
  ],
});
const wasm = await readFile("generated/account_crypto_bg.wasm");
let mf;
async function start() {
  mf = new Miniflare({
    modulesRoot: root,
    modules: [
      {
        type: "ESModule",
        path: join(root, "worker.mjs"),
        contents: bundled.outputFiles[0].text,
      },
      {
        type: "CompiledWasm",
        path: join(root, "account_crypto_bg.wasm"),
        contents: wasm,
      },
    ],
    compatibilityDate: "2026-07-01",
    bindings: { ACCOUNT_REALM: "owned" },
    durableObjects: {
      ACCOUNTS: { className: "TestAccounts", useSQLite: true },
      COORD: { className: "TestOwnedLot", useSQLite: true },
    },
    durableObjectsPersist: join(root, "do"),
    r2Buckets: ["CHECKPOINTS"],
    r2Persist: join(root, "r2"),
  });
  await mf.ready;
}
async function call(target, op, args = [], name) {
  const r = await mf.dispatchFetch("https://test/", {
    method: "POST",
    body: JSON.stringify({ target, op, args, name }),
  });
  const v = await r.json();
  if (!r.ok) throw Error(v.error);
  return v;
}
const a = (op, args = []) => call("account", op, args),
  l = (op, args = []) => call("lot", op, args);
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
const release = "b".repeat(64),
  protocol = "c".repeat(64),
  authority = "a".repeat(64),
  permission = { revision: "1", sha256: "d".repeat(64) },
  password = "private-owned-control-password",
  origin = "e".repeat(64);
const spec = {
  name: "First Owner",
  description: "",
  gender: "female",
  skin: "dark",
  head_key: "9007199254740993",
  body_key: "18446744073709551615",
  release,
};
const catalog = {
  release,
  entries: [
    { key: spec.head_key, part: "head", gender: "female", skins: ["dark"] },
    { key: spec.body_key, part: "body", gender: "female", skins: ["dark"] },
  ],
};
const binding = {
    protocol: 1,
    source_pin: "4c6b3e8f5835b228723caea3c9f683c62f244f73",
    binding_sha256: release,
    lot_incarnation: "1",
  },
  cursor = {
    lot_id: "16777472",
    authority_epoch: "1",
    completed_tick: "0",
    state_hash: "f".repeat(64),
  },
  setup = {
    authority_id: authority,
    content_release: release,
    protocol_release: protocol,
    permission_binding: permission,
    actors: [],
  };
try {
  await start();
  await a("initializeCatalog", [catalog]);
  await a("initializeAdmissionAuthorityV2", [authority]);
  await l("initializeControlledV2", [binding, cursor, "0".repeat(64), setup]);
  const exports = [],
    sessions = [];
  for (let i = 0; i < 2; i++) {
    await a("register", ["owned_control_" + i, password, origin]);
    const login = await a("login", ["owned_control_" + i, password, origin]);
    sessions.push(login);
    const avatar = await a("createAvatar", [
      login.token,
      "owned_avatar_creation_" + i,
      { ...spec, name: i ? "Second Owner" : "First Owner" },
    ]);
    const op = "owned_control_operation_" + i;
    await a("reserveAvatarControl", [
      login.token,
      op,
      {
        avatar_id: avatar.avatar_id,
        lot_id: cursor.lot_id,
        content_release: release,
        protocol_release: protocol,
      },
    ]);
    exports.push(
      await a("exportReservedAvatarV2", [
        login.token,
        op,
        avatar.avatar_id,
        permission,
      ]),
    );
  }
  const installs = [];
  for (const e of exports)
    installs.push(await l("installReservedAvatarV2", [e]));
  assert.equal(installs[0].principal, "1");
  assert.equal(installs[1].principal, "2");
  assert.equal(installs[1].control_revision, "2");
  assert.equal(
    (await l("installReservedAvatarV2", [exports[0]])).principal,
    "1",
  );
  await assert.rejects(
    l("installReservedAvatarV2", [{ ...exports[0], account_id: "3" }]),
    /changed avatar export/,
  );
  assert.equal((await l("getAdmissionV2", [exports[0]])).status, "installed");
  assert.equal((await l("getAdmissionV2", [exports[0]])).receipt, null);
  await mf.dispose();
  await start();
  assert.equal((await l("inspectControlledV2")).controls.length, 2);
  assert.equal(
    (await l("installReservedAvatarV2", [exports[0]])).principal,
    "1",
  );
  await a("advance", [30000]);
  await l("advance", [30000]);
  const renewals = [];
  for (let i = 0; i < 2; i++) {
    const renewed = await a("renewAvatarControlV2", [
      sessions[i].token,
      exports[i],
      "renew_owned_control_" + i,
    ]);
    renewals.push(renewed);
    const applied = await l("renewControlLeaseV2", [renewed]);
    assert.equal(applied.status, "renewed");
    assert.equal(applied.lease_revision, "2");
    assert.deepEqual(
      await a("renewAvatarControlV2", [
        sessions[i].token,
        exports[i],
        "renew_owned_control_" + i,
      ]),
      renewed,
    );
    assert.deepEqual(await l("renewControlLeaseV2", [renewed]), applied);
  }
  assert.equal((await l("inspectControlledV2")).control_revision, "4");
  await a("advance", [35000]);
  await l("advance", [35000]);
  for (const e of exports) {
    assert.equal(
      (await a("revalidateReservedAvatarV2", [e])).lease.revision,
      "2",
    );
    assert.equal((await l("getAdmissionV2", [e])).status, "installed");
  }
  await assert.rejects(
    a("renewAvatarControlV2", [
      sessions[1].token,
      exports[0],
      "wrong_session_renewal",
    ]),
    /lease owner/,
  );
  const racingRenewal = await a("renewAvatarControlV2", [
    sessions[0].token,
    exports[0],
    "renewal_racing_fence",
  ]);
  await l("recordControlFence", [
    {
      version: 1,
      authority,
      avatar_id: exports[0].avatar_id,
      generation: exports[0].control_generation,
      lot_id: cursor.lot_id,
      request_digest: exports[0].reservation_digest,
    },
  ]);
  assert.equal(
    (await l("renewControlLeaseV2", [racingRenewal])).reason,
    "control_fenced",
  );
  // Deliberately invalid simulation bytes are used ONLY for a denied candidate.
  // This suite never manufactures an admitted actor or a successful host batch.
  const payload = {
    binding,
    batch_id: "1",
    predecessor: cursor,
    result: { ...cursor, completed_tick: "1" },
    ticks_hex: "",
    checkpoint_sha256: "0".repeat(64),
    effects_hex: "0000000000000000",
    events_hex: "0000000000000000",
    control_v2: {
      schema_version: 2,
      revision_before: "4",
      revision_after: "4",
      actor_registry_before_sha256: sha("[]"),
      actor_registry_after_sha256: sha("[]"),
      actors: [],
      permission_before: permission,
      permission_after: permission,
    },
  };
  const proposal = {
    kind: "proposal",
    status: "pending",
    digest: sha(canonical(payload)),
    payload,
  };
  await assert.rejects(l("commitBatch", [proposal]), /requires V2/);
  await l("advance", [60001]);
  const denied = await l("commitControlledBatchV2", [proposal]);
  assert.equal(denied.status, "rejected");
  assert.equal(denied.reason, "lease_expired");
  const state = await l("inspectControlledV2");
  assert.equal(state.control_revision, "6");
  assert.equal(state.journal.count, 0);
  assert.deepEqual(state.cursor, cursor);
  assert.equal(state.pending_outbox, 0);
  assert.ok(state.controls.every((c) => c.state === "fence_pending"));
  assert.deepEqual(await l("commitControlledBatchV2", [proposal]), denied);
  assert.deepEqual(await l("getBatch", ["1", "1"]), denied);
  await mf.dispose();
  await start();
  assert.deepEqual(await l("commitControlledBatchV2", [proposal]), denied);
  assert.deepEqual(await l("getBatch", ["1", "1"]), denied);
  assert.equal((await l("inspectControlledV2")).control_revision, "6");
  await a("advance", [60001]);
  await assert.rejects(
    a("renewAvatarControlV2", [
      sessions[0].token,
      exports[0],
      "too_late_renewal_op",
    ]),
    /control fenced/,
  );
  console.log(
    "PASS actual workerd SQLite: direct account revalidation, bounded durable installation, distinct monotonic principals, retry/restart, no fabricated admission, V1 guard, authenticated monotonic renewal past original expiry, no late revival, expiry denial/tombstones persist atomically without journal/outbox/cursor mutation",
  );
} finally {
  await mf?.dispose();
}
