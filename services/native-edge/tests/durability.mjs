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
  const races = await Promise.allSettled([
    call("commitBatch", [nextProposal]),
    call("commitBatch", [competing]),
  ]);
  assert.equal(races.filter((r) => r.status === "fulfilled").length, 1);
  assert.equal((await call("inspect")).journal.count, 4);
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
  const winner = races.find((r) => r.status === "fulfilled").value;
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

  await mkdir(join(repo, "tests/output/native-durability"), {
    recursive: true,
  });
  await writeFile(
    join(repo, "tests/output/native-durability/report.json"),
    JSON.stringify(
      {
        passed: true,
        scope:
          "Local real workerd SQLite Durable Object and R2 plus actual native host; no deployment or takeover acceptance",
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
