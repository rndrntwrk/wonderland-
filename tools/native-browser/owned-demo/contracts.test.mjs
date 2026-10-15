import assert from "node:assert/strict";
import { test } from "node:test";
import { createRequire } from "node:module";
import { readFile, mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash, randomBytes } from "node:crypto";
import { drainControlFences } from "./fences.mjs";
import { loadOrCreateExport } from "./admission.mjs";
const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const require = createRequire(join(repo, "services/native-edge/package.json"));
const { Miniflare } = require("miniflare"),
  { build } = require("esbuild");
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
async function service() {
  const root = await mkdtemp(
      join(tmpdir(), "wonderland-owned-adapter-contract-"),
    ),
    capability = randomBytes(32).toString("hex");
  const bundle = await build({
    entryPoints: [join(repo, "tools/native-browser/owned-demo/driver.ts")],
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
  const mf = new Miniflare({
    modulesRoot: root,
    modules: [
      {
        type: "ESModule",
        path: join(root, "worker.mjs"),
        contents: bundle.outputFiles[0].text,
      },
      {
        type: "CompiledWasm",
        path: join(root, "account_crypto_bg.wasm"),
        contents: await readFile(
          join(repo, "services/native-edge/generated/account_crypto_bg.wasm"),
        ),
      },
    ],
    compatibilityDate: "2026-07-01",
    durableObjects: {
      ACCOUNTS: { className: "AccountAuthority", useSQLite: true },
      COORD: { className: "LotCoordinator", useSQLite: true },
    },
    durableObjectsPersist: join(root, "sqlite"),
    r2Buckets: ["CHECKPOINTS"],
    r2Persist: join(root, "r2"),
    bindings: {
      PRIVATE_CAPABILITY: capability,
      ACCOUNT_REALM: "adapter-contract",
      LOT_NAME: "16777472",
    },
  });
  await mf.ready;
  async function rpc(target, method, args = []) {
    const r = await mf.dispatchFetch("http://owned-private.invalid/rpc", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-owned-private": capability,
      },
      body: JSON.stringify({ target, method, args }),
    });
    const value = await r.json();
    if (!r.ok) throw Object.assign(Error(value.error), { code: value.code });
    return value.value;
  }
  return {
    mf,
    capability,
    account: (method, args) => rpc("account", method, args),
    lot: (method, args) => rpc("lot", method, args),
  };
}
test("private workerd RPC denies unauthenticated, browser-origin and form-compatible requests", async () => {
  const s = await service();
  try {
    const body = JSON.stringify({
      target: "account",
      method: "initializeAdmissionAuthorityV2",
      args: ["a".repeat(64)],
    });
    for (const headers of [
      {},
      { "content-type": "text/plain" },
      { "content-type": "application/json" },
      {
        "content-type": "application/json",
        "x-owned-private": s.capability,
        origin: "http://attacker.invalid",
      },
      { "content-type": "text/plain", "x-owned-private": s.capability },
    ])
      assert.equal(
        (
          await s.mf.dispatchFetch("http://owned-private.invalid/rpc", {
            method: "POST",
            headers,
            body,
          })
        ).status,
        403,
      );
    await s.account("initializeAdmissionAuthorityV2", ["a".repeat(64)]);
  } finally {
    await s.mf.dispose();
  }
});
test("actual account logout drains wrapped claims into durable lot fences before a proposal can commit", async () => {
  const s = await service(),
    { account, lot } = s;
  const authority = "a".repeat(64),
    release = "b".repeat(64),
    protocol = "c".repeat(64),
    permission = { revision: "1", sha256: "d".repeat(64) };
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
      state_hash: "e".repeat(64),
    };
  const spec = {
    name: "Fence Traveller",
    description: "",
    gender: "female",
    skin: "dark",
    head_key: "9007199254740993",
    body_key: "18446744073709551615",
    release,
  };
  try {
    await account("initializeCatalog", [
      {
        release,
        entries: [
          {
            key: spec.head_key,
            part: "head",
            gender: "female",
            skins: ["dark"],
          },
          {
            key: spec.body_key,
            part: "body",
            gender: "female",
            skins: ["dark"],
          },
        ],
      },
    ]);
    await account("initializeAdmissionAuthorityV2", [authority]);
    await lot("initializeControlledV2", [
      binding,
      cursor,
      "0".repeat(64),
      {
        authority_id: authority,
        content_release: release,
        protocol_release: protocol,
        permission_binding: permission,
        actors: [],
      },
    ]);
    await account("register", [
      "adapter_contract_owner",
      "private-local-contract-password",
      "f".repeat(64),
    ]);
    const session = await account("login", [
        "adapter_contract_owner",
        "private-local-contract-password",
        "f".repeat(64),
      ]),
      avatar = await account("createAvatar", [
        session.token,
        "adapter_avatar_create_1",
        spec,
      ]);
    const operation = "adapter_control_operation_1";
    await account("reserveAvatarControl", [
      session.token,
      operation,
      {
        avatar_id: avatar.avatar_id,
        lot_id: cursor.lot_id,
        content_release: release,
        protocol_release: protocol,
      },
    ]);
    await assert.rejects(
      account("getReservedAvatarExportV2", [
        session.token,
        operation,
        avatar.avatar_id,
      ]),
      (error) => error.code === "owned_export_unavailable",
    );
    const exported = await loadOrCreateExport({
      account,
      token: session.token,
      operation,
      avatarId: avatar.avatar_id,
      permissionBinding: permission,
    });
    // A fresh ingress has no cached permission binding. An already frozen export
    // wins over changed current lot policy, without re-exporting portable state.
    assert.deepEqual(
      await loadOrCreateExport({
        account,
        token: session.token,
        operation,
        avatarId: avatar.avatar_id,
        permissionBinding: { revision: "2", sha256: "1".repeat(64) },
      }),
      exported,
    );
    await assert.rejects(
      loadOrCreateExport({
        account,
        token: "0".repeat(64),
        operation,
        avatarId: avatar.avatar_id,
        permissionBinding: permission,
      }),
      (error) => error.code !== "owned_export_unavailable",
    );
    await lot("installReservedAvatarV2", [exported]);
    await account("logout", [session.token]);
    await drainControlFences({ account, lot, authority, lotId: cursor.lot_id });
    const before = await lot("inspectControlledV2");
    assert.equal(before.controls[0].state, "fence_pending");
    assert.equal(before.control_revision, "2");
    // This intentionally invalid candidate must be denied before simulation lanes.
    // It is never used as an accepted actor, successful batch or playable artifact.
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
        revision_before: "2",
        revision_after: "2",
        actor_registry_before_sha256: sha("[]"),
        actor_registry_after_sha256: sha("[]"),
        actors: [],
        permission_before: permission,
        permission_after: permission,
      },
    };
    const denied = await lot("commitControlledBatchV2", [
      {
        kind: "proposal",
        status: "pending",
        digest: sha(canonical(payload)),
        payload,
      },
    ]);
    assert.equal(denied.status, "rejected");
    assert.equal(denied.reason, "fence_drain_required");
    const after = await lot("inspectControlledV2");
    assert.deepEqual(after.cursor, cursor);
    assert.equal(after.journal.count, 0);
    assert.equal(after.pending_outbox, 0);
    assert.equal(after.actors.length, 0);
    await drainControlFences({ account, lot, authority, lotId: cursor.lot_id });
    assert.equal((await lot("inspectControlledV2")).control_revision, "2");
  } finally {
    await s.mf.dispose();
  }
});
