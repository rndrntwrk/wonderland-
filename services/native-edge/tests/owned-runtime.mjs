// Actual account/workerd-SQLite/production-Rust-Host integration. The Rust
// content is explicitly authored test material, never original-game evidence.
import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { readFile, writeFile, mkdtemp } from "node:fs/promises";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createInterface } from "node:readline";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import assert from "node:assert/strict";
const root = await mkdtemp(join(tmpdir(), "wonderland-owned-runtime-"));
const fixture =
  process.env.OWNED_BOUNDARY_FIXTURE ??
  resolve("../../target/debug/examples/owned-boundary-fixture");
const sha = (b) => createHash("sha256").update(b).digest("hex");
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
let mf, host;
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
async function call(target, op, args = []) {
  const r = await mf.dispatchFetch("https://test/", {
    method: "POST",
    body: JSON.stringify({ target, op, args }),
  });
  const v = await r.json();
  if (!r.ok) throw Error(v.error);
  return v;
}
const a = (op, args = []) => call("account", op, args),
  l = (op, args = []) => call("lot", op, args);
async function openHost(recovery) {
  const child = spawn(fixture, recovery ? [recovery] : [], {
    stdio: ["pipe", "pipe", "pipe"],
  });
  let error = "",
    closed = false;
  const queue = [],
    waiters = [];
  child.stderr.on("data", (b) => (error += b.toString()));
  child.on("error", (e) => {
    closed = true;
    for (const w of waiters.splice(0)) w.reject(e);
  });
  child.on("exit", () => {
    closed = true;
    for (const w of waiters.splice(0)) w.reject(Error(error || "host exited"));
  });
  createInterface({ input: child.stdout }).on("line", (line) => {
    const v = JSON.parse(line);
    if (waiters.length) waiters.shift().resolve(v);
    else queue.push(v);
  });
  const next = () =>
    queue.length
      ? Promise.resolve(queue.shift())
      : new Promise((resolve, reject) => {
          if (closed) return reject(Error(error || "host exited"));
          const timer = setTimeout(
            () => reject(Error("host timeout " + error)),
            15000,
          );
          waiters.push({
            resolve: (v) => {
              clearTimeout(timer);
              resolve(v);
            },
            reject: (e) => {
              clearTimeout(timer);
              reject(e);
            },
          });
        });
  const initial = await next();
  return {
    initial,
    ask(q) {
      const p = next();
      child.stdin.write(JSON.stringify(q) + "\n");
      return p;
    },
    async kill() {
      if (closed) return;
      const exited = once(child, "exit");
      child.kill("SIGKILL");
      const [, signal] = await exited;
      assert.equal(signal, "SIGKILL");
    },
  };
}
const control = (s) => ({
  control_revision: s.control_revision,
  actor_registry_sha256: s.actor_registry_sha256,
  actors: s.actors,
  permission_binding: s.permission_binding,
});
let batch = 0;
async function prepare(kind, args = {}) {
  const current = await host.ask({ kind: "inspect" }),
    stored = await l("inspectControlledV2");
  assert.deepEqual(current.cursor, stored.cursor);
  return host.ask({
    kind,
    batch_id: String(++batch),
    binding: current.binding,
    predecessor: current.cursor,
    expected_control_revision: stored.control_revision,
    ...args,
  });
}
async function commit(p) {
  assert.equal(p.kind, "proposal", JSON.stringify(p));
  const receipt = await l("commitControlledBatchV2", [p]);
  assert.equal(receipt.status, "committed", JSON.stringify(receipt));
  const accepted = await host.ask({
    kind: "resolve",
    batch_id: p.payload.batch_id,
    digest: p.digest,
    decision: { status: "committed", cursor: receipt.cursor },
  });
  assert.equal(accepted.kind, "committed");
  return receipt;
}
const authority = "b".repeat(64),
  protocol = "c".repeat(64),
  password = "owned-runtime-contract-password",
  origin = "e".repeat(64),
  sessions = [],
  exports = [],
  receipts = [];
try {
  await start();
  host = await openHost();
  const initial = host.initial;
  assert.equal(initial.admission_ready, true);
  assert.equal(
    initial.binding.source_pin,
    "4c6b3e8f5835b228723caea3c9f683c62f244f73",
  );
  assert.equal(initial.sealed_release.original_gameplay_qualified, false);
  const release = initial.binding.binding_sha256,
    spec = {
      name: "First Runtime Owner",
      description: "",
      gender: "female",
      skin: "dark",
      head_key: "9007199254740993",
      body_key: "18446744073709551615",
      release,
    };
  await a("initializeAdmissionAuthorityV2", [authority]);
  await a("initializeCatalog", [
    {
      release,
      entries: [
        { key: spec.head_key, part: "head", gender: "female", skins: ["dark"] },
        { key: spec.body_key, part: "body", gender: "female", skins: ["dark"] },
      ],
    },
  ]);
  await a("seedAvatarSequence", ["900"]);
  await l("initializeControlledV2", [
    initial.binding,
    initial.cursor,
    sha(Buffer.from(initial.checkpoint_hex, "hex")),
    {
      authority_id: authority,
      content_release: release,
      protocol_release: protocol,
      permission_binding: initial.permission_binding,
      actors: [],
    },
  ]);
  await l("publishControlledCheckpointV2", [
    initial.binding,
    initial.cursor,
    sha(Buffer.from(initial.checkpoint_hex, "hex")),
    Array.from(Buffer.from(initial.checkpoint_hex, "hex")),
    control(await l("inspectControlledV2")),
  ]);
  for (let i = 0; i < 2; i++) {
    await a("register", ["owned_runtime_" + i, password, origin]);
    const session = await a("login", ["owned_runtime_" + i, password, origin]);
    sessions.push(session);
    const avatar = await a("createAvatar", [
        session.token,
        "runtime_avatar_create_" + i,
        { ...spec, name: i ? "Second Runtime Owner" : "First Runtime Owner" },
      ]),
      operation = "runtime_avatar_admit_" + i;
    await a("reserveAvatarControl", [
      session.token,
      operation,
      {
        avatar_id: avatar.avatar_id,
        lot_id: initial.cursor.lot_id,
        content_release: release,
        protocol_release: protocol,
      },
    ]);
    const e = await a("exportReservedAvatarV2", [
      session.token,
      operation,
      avatar.avatar_id,
      (await host.ask({ kind: "inspect" })).permission_binding,
    ]);
    exports.push(e);
    const installed = await l("installReservedAvatarV2", [e]);
    assert.equal(installed.principal, String(i + 1));
    assert.notEqual(installed.principal, avatar.avatar_id);
    const p = await prepare("prepare_owned_admission", {
      principal: installed.principal,
      export: e,
    });
    const receipt = await commit(p);
    receipts.push({ proposal: p, receipt });
    assert.deepEqual(await l("commitControlledBatchV2", [p]), receipt);
    assert.equal((await l("getAdmissionV2", [e])).status, "active");
  }
  let persisted = await l("inspectControlledV2"),
    live = await host.ask({ kind: "inspect" });
  assert.equal(persisted.actors.length, 2);
  assert.notDeepEqual(persisted.actors[0].actor, persisted.actors[1].actor);
  assert.deepEqual(persisted.actors, live.actors);
  assert.equal(persisted.journal.count, 2);
  // A renewal after prepare invalidates even a no-input autonomous candidate.
  const stale = await prepare("prepare_player_tick");
  const renewed = await a("renewAvatarControlV2", [
    sessions[0].token,
    exports[0],
    "runtime_lease_renewal",
    "presence",
  ]);
  await l("renewControlLeaseV2", [renewed]);
  await assert.rejects(
    l("commitControlledBatchV2", [stale]),
    /stale control revision/,
  );
  await host.ask({
    kind: "resolve",
    batch_id: stale.payload.batch_id,
    digest: stale.digest,
    decision: { status: "rejected" },
  });
  await commit(await prepare("prepare_player_tick"));
  persisted = await l("inspectControlledV2");
  live = await host.ask({ kind: "inspect" });
  // Lose both process state and workerd instance; recover only durable SQLite/R2.
  await host.kill();
  host = null;
  await mf.dispose();
  await start();
  const recoveredExport = await a("getReservedAvatarExportV2", [
    sessions[0].token,
    exports[0].admission_operation,
    exports[0].avatar_id,
  ]);
  assert.deepEqual(recoveredExport, exports[0]);
  const begun = await l("beginControlledRecoveryV2", [
    live.binding.lot_incarnation,
  ]);
  const bundle = await l("controlledRecoveryBundleV2", [
    live.binding.lot_incarnation,
  ]);
  assert.equal(bundle.batches.length, 3);
  assert.deepEqual(bundle.control.actors, persisted.actors);
  const recoveryFile = join(root, "recovery.json");
  await writeFile(
    recoveryFile,
    JSON.stringify({
      checkpoint_hex: bundle.checkpoint_hex,
      ticks_hex: bundle.batches.map((b) => b.ticks_hex),
      grant: bundle.grant,
    }),
  );
  host = await openHost(recoveryFile);
  const replacement = host.initial;
  assert.equal(replacement.kind, "recovery_candidate");
  assert.deepEqual(replacement.actors, persisted.actors);
  assert.equal(
    replacement.actor_registry_sha256,
    persisted.actor_registry_sha256,
  );
  assert.deepEqual(
    replacement.permission_binding,
    persisted.permission_binding,
  );
  assert.equal(replacement.binding.lot_incarnation, "2");
  const marker = replacement.takeover_marker,
    wrapper = { schema_version: 2, marker, control: bundle.control };
  const completed = await l("finishControlledRecoveryV2", [
    wrapper,
    Array.from(Buffer.from(replacement.checkpoint_hex, "hex")),
  ]);
  assert.deepEqual(completed.marker, marker);
  assert.deepEqual(
    await l("finishControlledRecoveryV2", [
      wrapper,
      Array.from(Buffer.from(replacement.checkpoint_hex, "hex")),
    ]),
    completed,
  );
  await host.ask({ kind: "activate_recovery", marker });
  assert.deepEqual(
    (await host.ask({ kind: "inspect" })).actors,
    persisted.actors,
  );
  assert.deepEqual(
    await l("getBatch", ["1", receipts[0].proposal.payload.batch_id]),
    receipts[0].receipt,
  );
  await commit(await prepare("prepare_player_tick"));
  // Tombstone after preparation rejects ALL ordinary progression and preserves
  // the last accepted actor state. A receipt lookup is never renewed authority.
  const beforeFence = await l("inspectControlledV2"),
    fenced = await prepare("prepare_player_tick");
  await l("recordControlFence", [
    {
      version: 1,
      authority,
      avatar_id: exports[0].avatar_id,
      generation: exports[0].control_generation,
      lot_id: initial.cursor.lot_id,
      request_digest: exports[0].reservation_digest,
    },
  ]);
  const denied = await l("commitControlledBatchV2", [fenced]);
  assert.equal(denied.reason, "fence_drain_required");
  await host.ask({
    kind: "resolve",
    batch_id: fenced.payload.batch_id,
    digest: fenced.digest,
    decision: { status: "rejected" },
  });
  assert.deepEqual((await l("inspectControlledV2")).cursor, beforeFence.cursor);
  assert.equal(
    (await l("getAdmissionV2", [exports[0]])).status,
    "fence_pending",
  );
  await assert.rejects(
    l("finishControlledRecoveryV2", [
      { ...wrapper, control: control(await l("inspectControlledV2")) },
      Array.from(Buffer.from(replacement.checkpoint_hex, "hex")),
    ]),
    /changed controlled recovery completion/,
  );
  console.log(
    "PASS real AccountAuthority + workerd SQLite/R2 + production Rust Host: two owned source exports, distinct allocated principals/actors, atomic admission receipts/journal, stale autonomous candidate rejected after renewal, SIGKILL plus workerd restart, replay from initial checkpoint with three real accepted batches, takeover, exact historical receipt, post-recovery tick, fence-after-prepare rejection; authored content only, original gameplay NOT qualified.",
  );
  await writeFile(
    join(root, "verification.json"),
    JSON.stringify(
      {
        original_gameplay_qualified: false,
        actors: persisted.actors,
        old_cursor: live.cursor,
        new_binding: replacement.binding,
        journal_tail_batches: bundle.batches.length,
      },
      null,
      2,
    ),
  );
  console.log("Evidence directory: " + root);
} finally {
  await host?.kill();
  await mf?.dispose();
}
