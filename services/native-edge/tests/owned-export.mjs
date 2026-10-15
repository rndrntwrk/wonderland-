import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { readFile, mkdtemp } from "node:fs/promises";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";
const root = await mkdtemp(join(tmpdir(), "wonderland-owned-export-"));
const bundled = await build({
  entryPoints: ["tests/account-driver.ts"],
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
    durableObjects: {
      ACCOUNTS: { className: "TestAccounts", useSQLite: true },
    },
    durableObjectsPersist: join(root, "do"),
  });
  await mf.ready;
}
async function call(op, args = [], realm = "owned-export") {
  const r = await mf.dispatchFetch("https://test/", {
    method: "POST",
    body: JSON.stringify({ op, args, realm }),
  });
  const v = await r.json();
  if (!r.ok) throw Error(v.error);
  return v;
}
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
  authority = "a".repeat(64),
  protocol = "c".repeat(64),
  permission = { revision: "1", sha256: "d".repeat(64) },
  password = "source-portable-test-password",
  origin = "e".repeat(64);
const spec = {
  name: "Source Owner",
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
try {
  await start();
  await call("initializeCatalog", [catalog]);
  await call("initializeAdmissionAuthorityV2", [authority]);
  await assert.rejects(
    call("initializeAdmissionAuthorityV2", ["f".repeat(64)]),
    /already pinned/,
  );
  const owners = [];
  for (let i = 0; i < 2; i++) {
    await call("register", ["source_owner_" + i, password, origin]);
    const session = await call("login", [
      "source_owner_" + i,
      password,
      origin,
    ]);
    const avatar = await call("createAvatar", [
      session.token,
      "source_avatar_create_" + i,
      { ...spec, name: i ? "Other Owner" : "Source Owner" },
    ]);
    const operation = "source_admission_op_" + i;
    const claim = await call("reserveAvatarControl", [
      session.token,
      operation,
      {
        avatar_id: avatar.avatar_id,
        lot_id: "16777472",
        content_release: release,
        protocol_release: protocol,
      },
    ]);
    owners.push({ session, avatar, operation, claim });
  }
  const one = owners[0],
    two = owners[1];
  await assert.rejects(
    call("exportReservedAvatarV2", [
      two.session.token,
      one.operation,
      one.avatar.avatar_id,
      permission,
    ]),
    /session mismatch/,
  );
  const exported = await call("exportReservedAvatarV2", [
    one.session.token,
    one.operation,
    one.avatar.avatar_id,
    permission,
  ]);
  const portable = JSON.parse(exported.portable_json);
  assert.equal(portable.head, spec.head_key);
  assert.equal(portable.body, spec.body_key);
  assert.equal(portable.budget_mirror, "0");
  assert.deepEqual(portable.default_suits, {
    daywear: spec.body_key,
    sleepwear: "5587752452109",
    swimwear: "420906795021",
  });
  assert.deepEqual(portable.motive_data, [12288, ...Array(15).fill(0)]);
  assert.equal(portable.person_data[6], 1);
  assert.equal(portable.person_data[10], 20);
  assert.equal(Object.hasOwn(portable, "permissions"), false);
  assert.equal(Object.hasOwn(portable, "avatar_flags"), false);
  assert.deepEqual(exported.permission_source.roommate_lots, []);
  assert.equal(exported.permission_source.moderation_level, 0);
  assert.equal(exported.portable_json, canonical(portable));
  assert.equal(exported.portable_sha256, sha(exported.portable_json));
  const { export_digest, ...payload } = exported;
  assert.equal(export_digest, sha(canonical(payload)));
  assert.notEqual(exported.session_ref, one.session.token);
  assert.deepEqual(
    (await call("revalidateReservedAvatarV2", [exported])).export,
    exported,
  );
  await assert.rejects(
    call("revalidateReservedAvatarV2", [
      { ...exported, account_id: two.session.account_id },
    ]),
    /changed avatar export/,
  );
  await assert.rejects(
    call("exportReservedAvatarV2", [
      one.session.token,
      one.operation,
      one.avatar.avatar_id,
      { ...permission, revision: "2" },
    ]),
    /binding changed/,
  );
  await mf.dispose();
  await start();
  assert.deepEqual(
    await call("exportReservedAvatarV2", [
      one.session.token,
      one.operation,
      one.avatar.avatar_id,
      permission,
    ]),
    exported,
  );
  assert.deepEqual(
    await call("getReservedAvatarExportV2", [
      one.session.token,
      one.operation,
      one.avatar.avatar_id,
    ]),
    exported,
  );
  await assert.rejects(
    call("getReservedAvatarExportV2", [
      two.session.token,
      one.operation,
      one.avatar.avatar_id,
    ]),
    /session mismatch/,
  );
  await call("testBalance", [one.avatar.avatar_id, "7"]);
  await assert.rejects(
    call("revalidateReservedAvatarV2", [exported]),
    /source revision changed/,
  );
  await call("testBalance", [one.avatar.avatar_id, "0"]);
  await call("testOutfitPrice", [one.avatar.avatar_id, "1"]);
  await assert.rejects(
    call("revalidateReservedAvatarV2", [exported]),
    /source revision changed/,
  );
  await call("testOutfitPrice", [one.avatar.avatar_id, "0"]);
  await call("changeSource", [one.avatar.avatar_id, "moderation_level", 1]);
  await assert.rejects(
    call("revalidateReservedAvatarV2", [exported]),
    /source revision changed/,
  );
  await call("changeSource", [one.avatar.avatar_id, "moderation_level", 0]);
  const idleBefore = (await call("snapshot")).sessions.find(
    (row) => row.token_hash === exported.session_ref,
  ).idle;
  await call("advance", [1000]);
  await call("revalidateReservedAvatarV2", [exported]);
  const idleAfter = (await call("snapshot")).sessions.find(
    (row) => row.token_hash === exported.session_ref,
  ).idle;
  assert.equal(
    idleAfter,
    idleBefore,
    "private revalidation must not refresh account idle activity",
  );
  await call("removeSource", [two.avatar.avatar_id]);
  await assert.rejects(
    call("exportReservedAvatarV2", [
      two.session.token,
      two.operation,
      two.avatar.avatar_id,
      permission,
    ]),
    /source migration required/,
  );
  for (let index = 0; index < 59; index++) {
    await call("advance", [30000]);
    await call("renewAvatarControlV2", [
      one.session.token,
      exported,
      "browser_presence_" + String(index).padStart(3, "0"),
      "presence",
    ]);
  }
  const presenceValidated = await call("revalidateReservedAvatarV2", [
    exported,
  ]);
  assert.equal(
    presenceValidated.lease.expires_at,
    idleBefore,
    "presence lease must stop at original session idle deadline",
  );
  assert.equal(
    (await call("snapshot")).sessions.find(
      (row) => row.token_hash === exported.session_ref,
    ).idle,
    idleBefore,
  );
  await assert.rejects(
    call("renewAvatarControlV2", [
      one.session.token,
      exported,
      "browser_presence_058",
      "activity",
    ]),
    /operation identity reused/,
  );

  await call("advance", [60001]);
  await assert.rejects(
    call("revalidateReservedAvatarV2", [exported]),
    /unauthorized|control fenced/,
  );
  const fences = await call("pendingControlFences");
  assert.ok(
    fences.fences.some(
      (f) =>
        f.claim.avatar_id === one.avatar.avatar_id &&
        f.claim.state === "fence_pending",
    ),
  );
  await assert.rejects(
    call("reserveAvatarControl", [
      one.session.token,
      "new_owned_admission_op",
      {
        avatar_id: one.avatar.avatar_id,
        lot_id: "16777472",
        content_release: release,
        protocol_release: protocol,
      },
    ]),
    /unauthorized|already reserved/,
  );
  console.log(
    "PASS actual workerd SQLite: source-owned portable export, exact decimals/hash, two accounts/session isolation, restart, source revision mutation, missing migration, expiry retains custody and durable fence",
  );
} finally {
  await mf?.dispose();
}
