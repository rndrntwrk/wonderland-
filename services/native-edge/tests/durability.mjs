import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { spawn, execFileSync } from "node:child_process";
import { mkdtemp, readFile, writeFile, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { createInterface } from "node:readline";
import { createHash } from "node:crypto";
import assert from "node:assert/strict";
const repo = resolve("../..");
const root = await mkdtemp(join(tmpdir(), "wonderland-durable-"));
const release = join(root, "release");
const run = (file, args) =>
  execFileSync(join(repo, "target/debug", file), args, {
    encoding: "utf8",
    timeout: 60000,
  });
run("examples/host-cooked-fixture", [release]);
run("runtime-bridge", [
  "release-prepare",
  join(release, "draft.json"),
  "--release-dir",
  release,
  "--output",
  join(release, "binding.json"),
]);
run("examples/host-boundary-fixture", ["prepare", release]);
const host = spawn(
  join(repo, "target/debug/wonderland-native-lot-host"),
  [join(release, "host.json")],
  { stdio: ["pipe", "pipe", "pipe"] },
);
let stderr = "";
host.stderr.on("data", (d) => (stderr += d));
const queue = [],
  waiters = [];
createInterface({ input: host.stdout }).on("line", (line) => {
  const v = JSON.parse(line);
  if (waiters.length) waiters.shift()(v);
  else queue.push(v);
});
const next = () =>
  queue.length
    ? Promise.resolve(queue.shift())
    : new Promise((res, rej) => {
        const timer = setTimeout(
          () => rej(Error("host timeout " + stderr)),
          10000,
        );
        waiters.push((v) => {
          clearTimeout(timer);
          res(v);
        });
      });
const ask = async (q) => {
  host.stdin.write(JSON.stringify(q) + "\n");
  return next();
};
const bundled = await build({
  entryPoints: ["tests/driver.ts"],
  bundle: true,
  format: "esm",
  platform: "neutral",
  external: ["cloudflare:workers"],
  write: false,
});
let mf;
const start = async () => {
  mf = new Miniflare({
    modules: true,
    script: bundled.outputFiles[0].text,
    compatibilityDate: "2026-07-01",
    durableObjects: {
      COORD: { className: "TestCoordinator", useSQLite: true },
    },
    durableObjectsPersist: join(root, "do"),
    r2Buckets: ["CHECKPOINTS"],
    r2Persist: join(root, "r2"),
  });
  await mf.ready;
};
const call = async (op, args = [], lot = "declared-test-lot") => {
  const r = await mf.dispatchFetch("http://test/", {
    method: "POST",
    body: JSON.stringify({ op, args, lot }),
  });
  const v = await r.json();
  if (!r.ok) throw Error(v.error);
  return v;
};
const sha = (b) => createHash("sha256").update(b).digest("hex");
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
const checks = [];
try {
  await start();
  const initial = await next();
  const initialBytes = Buffer.from(initial.checkpoint_hex, "hex");
  await call("initialize", [
    initial.binding,
    initial.cursor,
    sha(initialBytes),
  ]);
  const fence = {
    version: 1,
    authority: "a".repeat(64),
    avatar_id: "1",
    generation: "1",
    lot_id: initial.cursor.lot_id,
    request_digest: "b".repeat(64),
  };
  const fenceReceipt = await call("recordControlFence", [fence]);
  assert.equal(fenceReceipt.status, "fenced");
  assert.deepEqual(await call("recordControlFence", [fence]), fenceReceipt);
  await assert.rejects(
    call("recordControlFence", [{ ...fence, request_digest: "c".repeat(64) }]),
    /identity reused/,
  );
  await assert.rejects(
    call("recordControlFence", [
      { ...fence, lot_id: fence.lot_id === "999" ? "998" : "999" },
    ]),
    /lot mismatch/,
  );
  await assert.rejects(
    call("recordControlFence", [{ ...fence, generation: "0" }]),
    /invalid control/,
  );
  await assert.rejects(
    call("recordControlFence", [{ ...fence, public_token: "not-a-grant" }]),
    /unexpected record/,
  );
  const commands_hex = (
    await readFile(join(release, "commands.txt"), "utf8")
  ).trim();
  const request = {
    kind: "prepare",
    batch_id: "1",
    binding: initial.binding,
    predecessor: initial.cursor,
    commands_hex,
  };
  const proposal = await ask(request);
  assert.equal(proposal.kind, "proposal");
  assert.deepEqual((await call("inspect")).cursor, initial.cursor);
  const controlledLot = "controlled-mode-negative-only";
  const initialArgs = [initial.binding, initial.cursor, sha(initialBytes)];
  const controlledInitial = await call(
    "initializeControlledV2",
    initialArgs,
    controlledLot,
  );
  assert.deepEqual(
    await call("initializeControlledV2", initialArgs, controlledLot),
    controlledInitial,
  );
  assert.deepEqual(await call("controlStateV2", [], controlledLot), {
    schema_version: 2,
    mode: "controlled_v2",
    revision: "0",
  });
  await assert.rejects(
    call("initialize", initialArgs, controlledLot),
    /mode mismatch/,
  );
  await assert.rejects(
    call("initializeControlledV2", initialArgs),
    /mode mismatch/,
  );
  const beforeControlled = await call("inspect", [], controlledLot);
  for (const [op, args] of [
    ["commitBatch", [proposal]],
    ["beginRecovery", [initial.binding.lot_incarnation]],
    ["recoveryBundle", [initial.binding.lot_incarnation]],
    ["finishRecovery", [{}, []]],
    ["publishCheckpoint", [...initialArgs, Array.from(initialBytes)]],
  ])
    await assert.rejects(
      call(op, args, controlledLot),
      /controlled lot requires V2/,
    );
  assert.deepEqual(await call("inspect", [], controlledLot), beforeControlled);
  assert.deepEqual(await call("getBatch", ["1", "1"], controlledLot), {
    status: "unknown",
  });
  const controlledFence = await call(
    "recordControlFence",
    [fence],
    controlledLot,
  );
  await Promise.all([
    call("recordControlFence", [fence], controlledLot),
    call("recordControlFence", [{ ...fence, avatar_id: "2" }], controlledLot),
  ]);
  assert.equal((await call("controlStateV2", [], controlledLot)).revision, "2");
  await assert.rejects(
    call(
      "recordControlFence",
      [{ ...fence, request_digest: "f".repeat(64) }],
      controlledLot,
    ),
    /identity reused/,
  );
  assert.equal((await call("controlStateV2", [], controlledLot)).revision, "2");
  const checkpointModeRace = await call(
    "raceControlledInitialization",
    [...initialArgs, Array.from(initialBytes)],
    "checkpoint-mode-race",
  );
  assert.deepEqual(checkpointModeRace, { published: false });
  assert.deepEqual(
    await call("listCheckpoints", [], "checkpoint-mode-race"),
    [],
  );
  const racedModeLot = "mode-initialization-race";
  const modeRace = await Promise.allSettled([
    call("initialize", initialArgs, racedModeLot),
    call("initializeControlledV2", initialArgs, racedModeLot),
  ]);
  assert.equal(modeRace.filter((r) => r.status === "fulfilled").length, 1);
  assert.equal(modeRace.filter((r) => r.status === "rejected").length, 1);
  await call("faultControlRevision", ["18446744073709551615"], controlledLot);
  await assert.rejects(
    call("recordControlFence", [{ ...fence, avatar_id: "3" }], controlledLot),
    /invalid decimal identity/,
  );
  assert.equal(
    (await call("controlStateV2", [], controlledLot)).revision,
    "18446744073709551615",
  );
  await call("faultControlRevision", ["2"], controlledLot);
  await call(
    "recordControlFence",
    [{ ...fence, avatar_id: "3" }],
    controlledLot,
  );
  assert.equal((await call("controlStateV2", [], controlledLot)).revision, "3");
  checks.push(
    "Controlled V2 initialization cannot upgrade/downgrade a lot; V1 native proposals, checkpoint and recovery mutations are denied; concurrent tombstones advance exact revisions once and overflow rolls back atomically (no actor admission enabled)",
  );
  for (const mode of ["count", "bytes"]) {
    const faultLot = `capacity-${mode}`;
    await call(
      "initialize",
      [initial.binding, initial.cursor, sha(initialBytes)],
      faultLot,
    );
    const before = await call("faultCapacity", [mode], faultLot);
    await assert.rejects(
      call("commitBatch", [proposal], faultLot),
      /journal capacity/,
    );
    assert.deepEqual(await call("inspect", [], faultLot), before);
    assert.deepEqual(await call("getBatch", ["1", "1"], faultLot), {
      status: "unknown",
    });
  }
  checks.push(
    "Fault-injected count and byte capacity boundaries reject a genuine proposal without changing state",
  );
  const receipt = await call("commitBatch", [proposal]);
  assert.equal(receipt.status, "committed");
  // Simulate a caller discarding its received reply: restart persisted workerd,
  // then obtain exactly the previously committed receipt without executing again.
  await mf.dispose();
  await start();
  assert.deepEqual(await call("recordControlFence", [fence]), fenceReceipt);

  assert.deepEqual(
    await call("recordControlFence", [fence], controlledLot),
    controlledFence,
  );
  assert.equal((await call("controlStateV2", [], controlledLot)).revision, "3");
  await assert.rejects(
    call("initialize", initialArgs, controlledLot),
    /mode mismatch/,
  );
  await assert.rejects(
    call("commitBatch", [proposal], controlledLot),
    /controlled lot requires V2/,
  );
  checks.push(
    "Controlled mode and revision survive persisted SQLite/workerd restart; generic write remains denied",
  );

  assert.deepEqual(await call("getBatch", ["1", "1"]), receipt);
  assert.deepEqual(await call("commitBatch", [proposal]), receipt);
  assert.equal(
    (await ask({ kind: "inspect" })).checkpoint_hex,
    initial.checkpoint_hex,
  );
  const committed = await ask({
    kind: "resolve",
    batch_id: "1",
    digest: receipt.digest,
    decision: { status: "committed", cursor: receipt.cursor },
  });
  assert.equal(committed.kind, "committed");
  let state = await ask({ kind: "inspect" });
  assert.deepEqual(state.cursor, receipt.cursor);
  assert.deepEqual(await call("readBatch", ["1", "1"]), proposal.payload);
  checks.push(
    "Actual native host proposal survives workerd restart and caller retry with exact durable receipt and journal",
  );
  const altered = structuredClone(proposal);
  altered.payload.events_hex += "00";
  await assert.rejects(call("commitBatch", [altered]), /digest mismatch/);
  altered.digest = sha(canonical(altered.payload));
  await assert.rejects(call("commitBatch", [altered]), /identity reused/);

  const outboxProposals = [];
  for (const [id, file] of [
    ["2", "build-commands.txt"],
    ["3", "effect-commands.txt"],
  ]) {
    const p = await ask({
      ...request,
      batch_id: id,
      predecessor: state.cursor,
      commands_hex: (await readFile(join(release, file), "utf8")).trim(),
    });
    assert.equal(p.kind, "proposal");
    outboxProposals.push(p);
    const r = await call("commitBatch", [p]);
    assert.equal(
      (
        await ask({
          kind: "resolve",
          batch_id: id,
          digest: r.digest,
          decision: { status: "committed", cursor: r.cursor },
        })
      ).kind,
      "committed",
    );
    state = await ask({ kind: "inspect" });
  }
  assert.equal((await call("inspect")).pending_outbox, 2);
  await mf.dispose();
  await start();
  assert.equal((await call("inspect")).pending_outbox, 2);
  checks.push(
    "Nonempty durable build and dialog effect outbox references survive actual workerd restart",
  );
  const checkpointBytes = Buffer.from(state.checkpoint_hex, "hex");
  const key = `lots/${state.cursor.lot_id}/${state.cursor.authority_epoch}/${state.cursor.completed_tick}/${sha(checkpointBytes)}`;
  const corruptBucket = await mf.getR2Bucket("CHECKPOINTS");
  await corruptBucket.put(key, Buffer.from("corrupt"));
  await assert.rejects(
    call("publishCheckpoint", [
      initial.binding,
      state.cursor,
      sha(checkpointBytes),
      [...checkpointBytes],
    ]),
    /size mismatch/,
  );
  assert.deepEqual(await call("listCheckpoints"), []);
  await corruptBucket.put(key, Buffer.alloc(checkpointBytes.length, 9));
  await assert.rejects(
    call("publishCheckpoint", [
      initial.binding,
      state.cursor,
      sha(checkpointBytes),
      [...checkpointBytes],
    ]),
    /corrupted/,
  );
  assert.deepEqual(await call("listCheckpoints"), []);
  await corruptBucket.put(key, checkpointBytes);
  const manifest = await call("publishCheckpoint", [
    initial.binding,
    state.cursor,
    sha(checkpointBytes),
    [...checkpointBytes],
  ]);
  assert.equal(manifest.sha256, sha(checkpointBytes));
  await mf.dispose();
  await start();
  assert.equal((await call("listCheckpoints"))[0].sha256, manifest.sha256);
  assert.equal((await call("inspect")).pending_outbox, 2);
  for (const p of outboxProposals)
    assert.deepEqual(
      await call("readBatch", ["1", p.payload.batch_id]),
      p.payload,
    );
  const bucket = await mf.getR2Bucket("CHECKPOINTS");
  assert.equal(
    sha(
      Buffer.from(await (await bucket.get(manifest.object_key)).arrayBuffer()),
    ),
    manifest.sha256,
  );
  await assert.rejects(
    call("publishCheckpoint", [
      initial.binding,
      state.cursor,
      "0".repeat(64),
      [...checkpointBytes],
    ]),
    /digest mismatch/,
  );
  const nextProposal = await ask({
    ...request,
    batch_id: "4",
    predecessor: state.cursor,
    commands_hex: (
      await readFile(join(release, "large-commands.txt"), "utf8")
    ).trim(),
  });
  assert.equal(nextProposal.kind, "proposal");
  assert.ok(canonical(nextProposal.payload).length > 256 * 1024);
  const raceLot = "checkpoint-io-race";
  await call(
    "initialize",
    [initial.binding, state.cursor, sha(checkpointBytes)],
    raceLot,
  );
  await call("pauseCheckpoint", [], raceLot);
  const publication = call(
    "publishCheckpoint",
    [initial.binding, state.cursor, sha(checkpointBytes), [...checkpointBytes]],
    raceLot,
  ).then(
    (value) => ({ value }),
    (error) => ({ error: String(error) }),
  );
  for (let i = 0; !(await call("checkpointPaused", [], raceLot)); i++) {
    if (i >= 100) throw Error("checkpoint did not reach paused R2 read");
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await call("commitBatch", [nextProposal], raceLot);
  await call("resumeCheckpoint", [], raceLot);
  assert.match((await publication).error, /superseded before publication/);
  assert.deepEqual(await call("listCheckpoints", [], raceLot), []);
  checks.push(
    "A genuine successor committed during paused R2 I/O prevents obsolete checkpoint publication",
  );
  // Adversarial competing metadata uses the same genuine runtime transition.
  // Either batch may win; no fabricated receipt is fed back to the host.
  const competing = structuredClone(nextProposal);
  competing.payload.batch_id = "5";
  competing.digest = sha(canonical(competing.payload));
  const competingLot = "competing-successors";
  await call(
    "initialize",
    [initial.binding, state.cursor, sha(checkpointBytes)],
    competingLot,
  );
  const races = await Promise.allSettled([
    call("commitBatch", [nextProposal], competingLot),
    call("commitBatch", [competing], competingLot),
  ]);
  const winner = await call("commitBatch", [nextProposal]);
  assert.equal(
    (
      await ask({
        kind: "resolve",
        batch_id: "4",
        digest: winner.digest,
        decision: { status: "committed", cursor: winner.cursor },
      })
    ).kind,
    "committed",
  );
  assert.equal(races.filter((r) => r.status === "fulfilled").length, 1);
  assert.equal((await call("inspect")).journal.count, 1);
  assert.equal((await call("inspect")).retained_receipts, 4);
  checks.push(
    "Digest alteration and changed-ID reuse denied; competing successors atomically admit one",
  );
  await assert.rejects(
    call("publishCheckpoint", [
      initial.binding,
      state.cursor,
      sha(checkpointBytes),
      [...checkpointBytes],
    ]),
    /not current/,
  );
  checks.push(
    "R2 checkpoint readback and SQLite manifest survive restart; stale/bad checkpoints refused",
  );

  const durableTail = await call("readBatch", ["1", winner.batch_id]);
  assert.equal(durableTail.ticks_hex, nextProposal.payload.ticks_hex);
  await writeFile(join(release, "checkpoint.bin"), checkpointBytes);
  const tailBytes = Buffer.from(durableTail.ticks_hex, "hex");
  await writeFile(join(release, "tail.bin"), tailBytes);
  const config = JSON.parse(await readFile(join(release, "host.json"), "utf8"));
  config.recovery = {
    checkpoint_file: join(release, "checkpoint.bin"),
    checkpoint_sha256: sha(checkpointBytes),
    tail_file: join(release, "tail.bin"),
    tail_sha256: sha(tailBytes),
    expected_cursor: winner.cursor,
  };
  await writeFile(join(release, "recovery.json"), JSON.stringify(config));
  const restored = JSON.parse(
    execFileSync(
      join(repo, "target/debug/wonderland-native-lot-host"),
      [join(release, "recovery.json")],
      { input: "", encoding: "utf8", timeout: 10000 },
    ),
  );
  assert.deepEqual(restored.cursor, winner.cursor);
  config.recovery.expected_cursor = {
    ...winner.cursor,
    state_hash: "0".repeat(64),
  };
  await writeFile(
    join(release, "recovery-invalid.json"),
    JSON.stringify(config),
  );
  assert.throws(
    () =>
      execFileSync(
        join(repo, "target/debug/wonderland-native-lot-host"),
        [join(release, "recovery-invalid.json")],
        {
          input: "",
          encoding: "utf8",
          timeout: 10000,
          stdio: ["pipe", "pipe", "pipe"],
        },
      ),
    /committed cursor/,
  );
  checks.push(
    "Replacement Rust process restores exact R2 checkpoint plus actual accepted multi-chunk journal tail; wrong terminal hash prevents ready output",
  );

  const beforeEffect = await ask({ kind: "inspect" });
  const pendingEffectProposal = await ask({
    ...request,
    batch_id: "6",
    predecessor: beforeEffect.cursor,
    commands_hex: (
      await readFile(join(release, "effect-commands.txt"), "utf8")
    ).trim(),
  });
  assert.equal(pendingEffectProposal.kind, "proposal");
  const effectReceipt = await call("commitBatch", [pendingEffectProposal]);
  assert.equal(
    (
      await ask({
        kind: "resolve",
        batch_id: "6",
        digest: effectReceipt.digest,
        decision: { status: "committed", cursor: effectReceipt.cursor },
      })
    ).kind,
    "committed",
  );
  const persistedEffect = await call("readBatch", ["1", "6"]);
  const current = await ask({ kind: "inspect" });
  const staleProposal = await ask({
    ...request,
    batch_id: "99",
    predecessor: current.cursor,
    commands_hex: "01000000000000000000000000000000",
  });
  assert.equal(staleProposal.kind, "proposal");
  const manifestsBeforeFence = await call("listCheckpoints");
  await call("pauseCheckpoint");
  const oldCheckpointPublish = call("publishCheckpoint", [
    initial.binding,
    current.cursor,
    sha(Buffer.from(current.checkpoint_hex, "hex")),
    [...Buffer.from(current.checkpoint_hex, "hex")],
  ]).then(
    (value) => ({ value }),
    (error) => ({ error: String(error) }),
  );
  for (let i = 0; !(await call("checkpointPaused")); i++) {
    if (i >= 100) throw Error("old checkpoint did not reach R2 fence window");
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  const grant = await call("beginRecovery", ["1"]);
  await call("resumeCheckpoint");
  assert.match((await oldCheckpointPublish).error, /recovery in progress/);
  assert.deepEqual(await call("listCheckpoints"), manifestsBeforeFence);
  await assert.rejects(call("beginRecovery", ["2"]), /recovery in progress/);
  await assert.rejects(
    call("commitBatch", [staleProposal]),
    /recovery in progress/,
  );
  await mf.dispose();
  await start();
  assert.deepEqual(await call("beginRecovery", ["1"]), grant);
  const bundle = await call("recoveryBundle", ["1"]);
  const recoveryBucket = await mf.getR2Bucket("CHECKPOINTS");
  const checkpointObject = Buffer.from(bundle.checkpoint_hex, "hex");
  await recoveryBucket.delete(bundle.checkpoint.object_key);
  await assert.rejects(call("recoveryBundle", ["1"]), /missing or wrong size/);
  await recoveryBucket.put(
    bundle.checkpoint.object_key,
    Buffer.alloc(8 * 1024 * 1024 + 1),
  );
  await assert.rejects(call("recoveryBundle", ["1"]), /missing or wrong size/);
  await recoveryBucket.put(
    bundle.checkpoint.object_key,
    Buffer.alloc(checkpointObject.length, 3),
  );
  await assert.rejects(
    call("recoveryBundle", ["1"]),
    /checkpoint digest mismatch/,
  );
  await recoveryBucket.put(bundle.checkpoint.object_key, checkpointObject);
  const savedJournal = await call("damageJournal", ["1:4"]);
  await assert.rejects(
    call("recoveryBundle", ["1"]),
    /journal integrity mismatch/,
  );
  await call("restoreJournal", ["1:4", savedJournal]);
  await call("oversizeJournal", ["1:4"]);
  await assert.rejects(call("readBatch", ["1", "4"]), /chunk bounds/);
  await assert.rejects(
    call("recoveryBundle", ["1"]),
    /journal integrity mismatch/,
  );
  await call("restoreJournal", ["1:4", savedJournal]);
  await call("nulJournal", ["1:4"]);
  await assert.rejects(call("readBatch", ["1", "4"]), /chunk bounds/);
  await assert.rejects(
    call("recoveryBundle", ["1"]),
    /journal integrity mismatch/,
  );
  await call("restoreJournal", ["1:4", savedJournal]);
  assert.deepEqual(await call("recoveryBundle", ["1"]), bundle);
  assert.deepEqual(bundle.grant, grant);
  assert.deepEqual(
    bundle.batches.map((p) => p.batch_id),
    ["4", "6"],
  );
  await writeFile(
    join(release, "bundle-checkpoint.bin"),
    Buffer.from(bundle.checkpoint_hex, "hex"),
  );
  const pinnedParts = [];
  for (const part of bundle.batches) {
    const file = join(release, `bundle-tail-${part.batch_id}.bin`),
      bytes = Buffer.from(part.ticks_hex, "hex");
    await writeFile(file, bytes);
    pinnedParts.push({ file, sha256: sha(bytes) });
  }
  config.recovery.checkpoint_file = join(release, "bundle-checkpoint.bin");
  config.recovery.checkpoint_sha256 = bundle.checkpoint.sha256;
  config.recovery.tail_file = pinnedParts[0].file;
  config.recovery.tail_sha256 = pinnedParts[0].sha256;
  config.recovery.additional_tail_files = pinnedParts.slice(1);
  config.recovery.expected_cursor = current.cursor;
  await writeFile(
    join(release, "recovery-original.json"),
    JSON.stringify(config),
  );
  config.recovery.takeover = grant;
  await writeFile(join(release, "takeover.json"), JSON.stringify(config));
  const malformedGrantConfig = structuredClone(config);
  malformedGrantConfig.recovery.takeover.new_authority_epoch =
    "9007199254740995";
  await writeFile(
    join(release, "bad-grant.json"),
    JSON.stringify(malformedGrantConfig),
  );
  assert.throws(
    () =>
      execFileSync(
        join(repo, "target/debug/wonderland-native-lot-host"),
        [join(release, "bad-grant.json")],
        {
          input: "",
          encoding: "utf8",
          timeout: 10000,
          stdio: ["pipe", "pipe", "pipe"],
        },
      ),
    /recovery grant/,
  );
  const replacement = JSON.parse(
    execFileSync(
      join(repo, "target/debug/wonderland-native-lot-host"),
      [join(release, "takeover.json")],
      { input: "", encoding: "utf8", timeout: 10000 },
    ),
  );
  assert.equal(replacement.kind, "recovery_candidate");
  assert.equal(replacement.recovery_pending, true);
  assert.equal(replacement.binding.lot_incarnation, "2");
  assert.equal(replacement.cursor.authority_epoch, "9007199254740994");
  run("examples/host-boundary-fixture", ["takeover", release]);
  const marker = replacement.takeover_marker;
  const anchor = Buffer.from(replacement.checkpoint_hex, "hex");
  await assert.rejects(
    call("finishRecovery", [marker, [...Buffer.alloc(anchor.length)]]),
    /digest/,
  );
  const changedGrantMarker = structuredClone(marker);
  changedGrantMarker.grant.old_checkpoint_sha256 = "0".repeat(64);
  await assert.rejects(
    call("finishRecovery", [changedGrantMarker, [...anchor]]),
    /changed recovery grant/,
  );
  await call("pauseCheckpoint");
  const racingBundle = call("recoveryBundle", ["1"]).then(
    (value) => ({ value }),
    (error) => ({ error: String(error) }),
  );
  for (let i = 0; !(await call("checkpointPaused")); i++) {
    if (i >= 100) throw Error("bundle did not reach paused R2 read");
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  const finished = await call("finishRecovery", [marker, [...anchor]]);
  await call("resumeCheckpoint");
  assert.match(
    (await racingBundle).error,
    /recovery changed during bundle read/,
  );
  const retainedCheckpoints = await call("listCheckpoints");
  assert.equal(retainedCheckpoints.length, 2);
  assert.equal(
    retainedCheckpoints[0].cursor.authority_epoch,
    "9007199254740994",
  );
  assert.equal(
    retainedCheckpoints[1].cursor.authority_epoch,
    "9007199254740993",
  );
  assert.equal((await call("inspect")).pending_outbox, 3);
  await mf.dispose();
  await start();
  assert.deepEqual(
    await call("finishRecovery", [marker, [...anchor]]),
    finished,
  );
  assert.deepEqual(await call("beginRecovery", ["1"]), grant);
  await assert.rejects(call("commitBatch", [staleProposal]), /stale binding/);
  const changedCompletion = structuredClone(marker);
  changedCompletion.new_cursor.state_hash = "0".repeat(64);
  await assert.rejects(
    call("finishRecovery", [changedCompletion, [...anchor]]),
    /changed recovery completion/,
  );
  for (const field of ["incarnation", "epoch"]) {
    const lot = `overflow-${field}`;
    const b = { ...initial.binding },
      c = { ...initial.cursor };
    if (field === "incarnation") b.lot_incarnation = "18446744073709551615";
    else c.authority_epoch = "18446744073709551615";
    await call("initialize", [b, c, sha(initialBytes)], lot);
    const before = await call("inspect", [], lot);
    await assert.rejects(
      call("beginRecovery", [b.lot_incarnation], lot),
      /decimal identity/,
    );
    assert.deepEqual(await call("inspect", [], lot), before);
  }
  const nextRequest = {
    kind: "prepare",
    batch_id: "1",
    binding: replacement.binding,
    predecessor: replacement.cursor,
    commands_hex: "01000000000000000000000000000000",
  };
  const replacementOutput = execFileSync(
    join(repo, "target/debug/wonderland-native-lot-host"),
    [join(release, "takeover.json")],
    {
      input:
        JSON.stringify(nextRequest) +
        "\n" +
        JSON.stringify({ kind: "activate_recovery", marker: finished }) +
        "\n" +
        JSON.stringify(nextRequest) +
        "\n",
      encoding: "utf8",
      timeout: 10000,
    },
  )
    .trim()
    .split("\n")
    .map(JSON.parse);
  assert.equal(replacementOutput[1].kind, "denied");
  assert.match(
    replacementOutput[1].reason,
    /awaits durable recovery activation/,
  );
  assert.equal(replacementOutput[2].kind, "committed_state");
  assert.equal(replacementOutput[3].kind, "proposal");
  const acceptedReplacement = await call("commitBatch", [replacementOutput[3]]);
  assert.equal(acceptedReplacement.cursor.authority_epoch, "9007199254740994");
  const replayInput =
    [
      { kind: "activate_recovery", marker: finished },
      nextRequest,
      {
        kind: "resolve",
        batch_id: "1",
        digest: acceptedReplacement.digest,
        decision: { status: "committed", cursor: acceptedReplacement.cursor },
      },
      { kind: "inspect" },
    ]
      .map((v) => JSON.stringify(v))
      .join("\n") + "\n";
  const replayOutput = execFileSync(
    join(repo, "target/debug/wonderland-native-lot-host"),
    [join(release, "takeover.json")],
    { input: replayInput, encoding: "utf8", timeout: 10000 },
  )
    .trim()
    .split("\n")
    .map(JSON.parse);
  const currentReplacement = replayOutput.at(-1);
  assert.deepEqual(currentReplacement.cursor, acceptedReplacement.cursor);
  const currentBytes = Buffer.from(currentReplacement.checkpoint_hex, "hex");
  await call("publishCheckpoint", [
    currentReplacement.binding,
    currentReplacement.cursor,
    sha(currentBytes),
    [...currentBytes],
  ]);
  const nextGrant = await call("beginRecovery", ["2"]);
  const emptyBundle = await call("recoveryBundle", ["2"]);
  assert.deepEqual(emptyBundle.batches, []);
  const emptyConfig = JSON.parse(
    await readFile(join(release, "host.json"), "utf8"),
  );
  emptyConfig.authority_epoch = currentReplacement.cursor.authority_epoch;
  emptyConfig.lot_incarnation = currentReplacement.binding.lot_incarnation;
  await writeFile(
    join(release, "empty-tail-checkpoint.bin"),
    Buffer.from(emptyBundle.checkpoint_hex, "hex"),
  );
  emptyConfig.recovery = {
    checkpoint_file: join(release, "empty-tail-checkpoint.bin"),
    checkpoint_sha256: emptyBundle.checkpoint.sha256,
    expected_cursor: nextGrant.old_cursor,
    takeover: nextGrant,
  };
  await writeFile(
    join(release, "empty-tail-recovery.json"),
    JSON.stringify(emptyConfig),
  );
  const emptyRestored = JSON.parse(
    execFileSync(
      join(repo, "target/debug/wonderland-native-lot-host"),
      [join(release, "empty-tail-recovery.json")],
      { input: "", encoding: "utf8", timeout: 10000 },
    ),
  );
  assert.equal(emptyRestored.binding.lot_incarnation, "3");
  assert.equal(emptyRestored.kind, "recovery_candidate");
  await call("finishRecovery", [
    emptyRestored.takeover_marker,
    [...Buffer.from(emptyRestored.checkpoint_hex, "hex")],
  ]);
  await assert.rejects(call("recoveryBundle", ["2"]), /no pending recovery/);
  checks.push(
    "Exact-current checkpoint recovery needs no fabricated tail and supports the next fenced incarnation",
  );
  checks.push(
    "Durable recovery grant survives restart, assembles exact multi-batch bundle, fences live old host, preserves pending logical effects, gates activation and accepts actual replacement successor",
  );

  assert.deepEqual(await call("recordControlFence", [fence]), fenceReceipt);
  checks.push(
    "Control fence tombstones are exact-idempotent, reject altered identity, survive workerd restart and two actual host replacements; receipt does not release custody or admit an actor",
  );

  await mkdir(join(repo, "tests/output/native-durability"), {
    recursive: true,
  });
  await writeFile(
    join(repo, "tests/output/native-durability/report.json"),
    JSON.stringify(
      {
        passed: true,
        scope:
          "Local real workerd SQLite Durable Object and R2 plus actual native host; local fenced takeover only; no deployment",
        checks,
      },
      null,
      2,
    ) + "\n",
  );
  console.log(JSON.stringify({ passed: true, checks }, null, 2));
} finally {
  host.kill();
  await mf?.dispose();
}
