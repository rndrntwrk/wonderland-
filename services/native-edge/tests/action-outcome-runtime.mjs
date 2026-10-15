// SIGKILL after durable action commit and before host resolve/export.
// Actual account/workerd SQLite + production Host; authored test content only.
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
import { reconcileAuthenticatedPlayerAction } from "../../../tools/native-browser/owned-demo/action-results.mjs";
import { drainControlFences } from "../../../tools/native-browser/owned-demo/fences.mjs";
const root = await mkdtemp(
  join(tmpdir(), "wonderland-action-outcome-runtime-"),
);
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
  for (let i = 0; i < 1; i++) {
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
  const invoke = await host.ask({
    kind: "fixture_player_action",
    principal: "1",
    command_sequence: "1",
    action: "invoke",
  });
  assert.equal(invoke.kind, "fixture_player_action", JSON.stringify(invoke));
  await commit(
    await prepare("prepare_player_action", {
      control: invoke.control,
      action_hex: invoke.action_hex,
    }),
  );
  // A real authored queue item now exists, so cancellation is admitted by the
  // unchanged production source validator rather than a fabricated command.
  const beforeCancel = await host.ask({ kind: "inspect" });
  await l("publishControlledCheckpointV2", [
    beforeCancel.binding,
    beforeCancel.cursor,
    sha(Buffer.from(beforeCancel.checkpoint_hex, "hex")),
    Array.from(Buffer.from(beforeCancel.checkpoint_hex, "hex")),
    control(await l("inspectControlledV2")),
  ]);
  const cancel = await host.ask({
    kind: "fixture_player_action",
    principal: "1",
    command_sequence: "2",
    action: "cancel",
  });
  assert.equal(cancel.kind, "fixture_player_action", JSON.stringify(cancel));
  const candidate = await prepare("prepare_player_action", {
    control: cancel.control,
    action_hex: cancel.action_hex,
  });
  assert.equal(candidate.kind, "proposal", JSON.stringify(candidate));
  const evidence = candidate.payload.action_v1;
  assert.equal(evidence.request_hex, cancel.action_hex);
  const accepted = await l("commitControlledBatchV2", [candidate]);
  assert.equal(accepted.status, "committed");
  // The host has NOT resolved this action and has NOT exported its reply.
  assert.deepEqual(
    (await host.ask({ kind: "inspect" })).cursor,
    beforeCancel.cursor,
  );
  assert.equal(
    (await host.ask({ kind: "lookup", batch_id: candidate.payload.batch_id }))
      .kind,
    "proposal",
  );
  const durable = await l("inspectControlledV2");
  await host.kill();
  host = null;
  await mf.dispose();
  mf = null;
  await start();
  const query = {
    schema_version: 1,
    lot_incarnation: beforeCancel.binding.lot_incarnation,
    control: evidence.control,
    request_hex: cancel.action_hex,
  };
  const found = await l("lookupPlayerActionV1", [query]);
  assert.equal(found.status, "committed");
  assert.equal(found.receipt_hex, evidence.accepted_receipt_hex);
  assert.equal(found.request_hex, cancel.action_hex);
  assert.equal(found.digest, candidate.digest);
  assert.deepEqual(found.cursor, accepted.cursor);
  assert.deepEqual(await l("commitControlledBatchV2", [candidate]), accepted);
  const afterRetry = await l("inspectControlledV2");
  assert.deepEqual(afterRetry.cursor, durable.cursor);
  assert.equal(afterRetry.journal.count, durable.journal.count);
  assert.equal(afterRetry.pending_outbox, durable.pending_outbox);
  await l("beginControlledRecoveryV2", [beforeCancel.binding.lot_incarnation]);
  const bundle = await l("controlledRecoveryBundleV2", [
    beforeCancel.binding.lot_incarnation,
  ]);
  assert.equal(bundle.batches.length, 1);
  assert.equal(bundle.batches[0].batch_id, candidate.payload.batch_id);
  const recoveryFile = join(root, "action-recovery.json");
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
  const marker = replacement.takeover_marker;
  await l("finishControlledRecoveryV2", [
    { schema_version: 2, marker, control: bundle.control },
    Array.from(Buffer.from(replacement.checkpoint_hex, "hex")),
  ]);
  await host.ask({ kind: "activate_recovery", marker });
  assert.deepEqual(await l("lookupPlayerActionV1", [query]), found);
  assert.equal(
    (
      await l("lookupPlayerActionV1", [
        { ...query, lot_incarnation: replacement.binding.lot_incarnation },
      ])
    ).status,
    "unknown",
  );
  const entry = {
    token: sessions[0].token,
    export: exports[0],
    control: cancel.control,
  };
  const emitted = [],
    calls = [];
  const dependencies = {
    sweepFences: async () => {
      calls.push("fences");
      await drainControlFences({
        account: a,
        lot: l,
        authority,
        lotId: beforeCancel.cursor.lot_id,
      });
    },
    revalidate: async (entry) => {
      calls.push("revalidate");
      await a("revalidateReservedAvatarV2", [entry.export]);
      const current = await host.ask({ kind: "inspect" }),
        stored = await l("inspectControlledV2");
      assert.deepEqual(current.cursor, stored.cursor);
      const held = stored.controls.find(
        (c) =>
          c.avatar_id === entry.export.avatar_id &&
          c.control_generation === entry.export.control_generation &&
          c.session_ref === entry.export.session_ref,
      );
      assert.ok(
        held && held.state === "active" && held.expires_at > Date.now(),
      );
      assert.ok(
        current.actors.some(
          (actor) =>
            actor.principal === entry.control.principal &&
            actor.account_id === entry.export.account_id &&
            actor.avatar_id === entry.export.avatar_id &&
            actor.control_generation === entry.control.control_generation &&
            actor.session_ref === entry.control.session_ref,
        ),
      );
      return current;
    },
    lookup: async (request) => {
      calls.push("lookup");
      return l("lookupPlayerActionV1", [request]);
    },
    receipt: (bytes) => {
      calls.push("receipt");
      emitted.push(bytes);
    },
  };
  const browserQuery = {
    type: "native_action_result",
    lot_incarnation: query.lot_incarnation,
    action_hex: query.request_hex,
  };
  const beforeLookup = await l("inspectControlledV2");
  await reconcileAuthenticatedPlayerAction(browserQuery, entry, dependencies);
  assert.deepEqual(calls, ["fences", "revalidate", "lookup", "receipt"]);
  assert.deepEqual(emitted, [evidence.accepted_receipt_hex]);
  const afterLookup = await l("inspectControlledV2");
  assert.deepEqual(afterLookup.cursor, beforeLookup.cursor);
  assert.deepEqual(afterLookup.controls, beforeLookup.controls);
  assert.equal(afterLookup.control_revision, beforeLookup.control_revision);
  assert.equal(afterLookup.pending_outbox, beforeLookup.pending_outbox);
  // An explicit test replay is refused by restored accepted sequence state.
  // Production reconnect never sends this WLC1 again.
  const replay = await prepare("prepare_player_action", {
    control: cancel.control,
    action_hex: cancel.action_hex,
  });
  assert.equal(replay.kind, "denied");
  assert.match(replay.reason, /sequence was already accepted/);
  assert.deepEqual(
    (await host.ask({ kind: "inspect" })).cursor,
    afterLookup.cursor,
  );

  // Cover an actual durable fence rejection and another crash before resolve.
  const next = await host.ask({
    kind: "fixture_player_action",
    principal: "1",
    command_sequence: "3",
    action: "invoke",
  });
  assert.equal(next.kind, "fixture_player_action", JSON.stringify(next));
  const deniedProposal = await prepare("prepare_player_action", {
    control: next.control,
    action_hex: next.action_hex,
  });
  assert.equal(deniedProposal.kind, "proposal", JSON.stringify(deniedProposal));
  await a("logout", [sessions[0].token]);
  await drainControlFences({
    account: a,
    lot: l,
    authority,
    lotId: beforeCancel.cursor.lot_id,
  });
  const denied = await l("commitControlledBatchV2", [deniedProposal]);
  assert.equal(denied.status, "rejected");
  assert.equal(denied.reason, "fence_drain_required");
  const deniedQuery = {
    schema_version: 1,
    lot_incarnation: deniedProposal.payload.binding.lot_incarnation,
    control: deniedProposal.payload.action_v1.control,
    request_hex: next.action_hex,
  };
  const terminalDenied = await l("lookupPlayerActionV1", [deniedQuery]);
  assert.equal(terminalDenied.status, "rejected");
  assert.equal(
    terminalDenied.receipt_hex,
    deniedProposal.payload.action_v1.rejected_receipt_hex,
  );
  calls.length = 0;
  emitted.length = 0;
  await assert.rejects(
    reconcileAuthenticatedPlayerAction(browserQuery, entry, dependencies),
  );
  assert.deepEqual(calls, ["fences", "revalidate"]);
  assert.deepEqual(emitted, []);
  const beforeRejectedCrash = await l("inspectControlledV2");
  await host.kill();
  host = null;
  await mf.dispose();
  mf = null;
  await start();
  assert.deepEqual(
    await l("lookupPlayerActionV1", [deniedQuery]),
    terminalDenied,
  );
  assert.deepEqual(await l("lookupPlayerActionV1", [query]), found);
  const afterRejectedCrash = await l("inspectControlledV2");
  assert.deepEqual(afterRejectedCrash.cursor, beforeRejectedCrash.cursor);
  assert.equal(
    afterRejectedCrash.pending_outbox,
    beforeRejectedCrash.pending_outbox,
  );
  await writeFile(
    join(root, "verification.json"),
    JSON.stringify(
      {
        original_gameplay_qualified: false,
        committed: found,
        rejected: terminalDenied,
        old_incarnation: beforeCancel.binding.lot_incarnation,
        recovered_incarnation: replacement.binding.lot_incarnation,
        accepted_replay_refused: true,
        revoked_reconciliation_blocked: true,
      },
      null,
      2,
    ),
  );
  console.log(
    "PASS actual Rust Host + persisted workerd: admitted Invoke/Cancel evidence, SIGKILL before resolve/export after committed and rejected decisions, exact original WLA1 retained across restart/takeover, unchanged VM/outbox on retries, authenticated recovered delivery and revoked-session denial. Authored fixture only; original gameplay NOT qualified.",
  );
  console.log("Evidence directory: " + root);
} finally {
  await host?.kill();
  await mf?.dispose();
}
