// Bounded cross-language custody witness: the production Rust Host is exercised
// through its existing authored owned-boundary fixture, with actual workerd/R2.
// No original-game asset or playability claim is made.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { once } from "node:events";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { build } from "esbuild";
import { Miniflare } from "miniflare";
import {
  askWithDurableReceipts,
  retireCheckpointReceipts,
} from "../../../tools/native-browser/owned-demo/receipts.mjs";
const root = await mkdtemp(join(tmpdir(), "wonderland-host-receipt-pair-"));
const bundled = await build({
  entryPoints: ["tests/compaction-driver.ts"],
  bundle: true,
  format: "esm",
  platform: "neutral",
  external: ["cloudflare:workers"],
  write: false,
});
const fixture =
  process.env.OWNED_BOUNDARY_FIXTURE ??
  resolve("../../target/debug/examples/owned-boundary-fixture");
const sha = (b) => createHash("sha256").update(b).digest("hex");
let mf, child;
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
const lot = async (op, args = []) => {
  const response = await mf.dispatchFetch("http://test/", {
    method: "POST",
    body: JSON.stringify({ op, args, lot: "host-receipt-pair" }),
  });
  const v = await response.json();
  if (!response.ok) throw Error(v.error);
  return v;
};
async function openHost() {
  child = spawn(fixture, [], { stdio: ["pipe", "pipe", "pipe"] });
  let closed = false,
    stderr = "";
  const queue = [],
    waiters = [];
  child.stderr.on("data", (d) => (stderr += d));
  child.on("error", (e) => {
    closed = true;
    for (const w of waiters.splice(0)) w.reject(e);
  });
  child.on("exit", () => {
    closed = true;
    for (const w of waiters.splice(0)) w.reject(Error(stderr || "host exited"));
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
          if (closed) return reject(Error(stderr || "host exited"));
          const timer = setTimeout(() => reject(Error("host timeout")), 15000);
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
  return {
    initial: await next(),
    ask: (q) => {
      const result = next();
      child.stdin.write(JSON.stringify(q) + "\n");
      return result;
    },
  };
}
const checks = [];
try {
  await start();
  const host = await openHost(),
    initial = host.initial;
  assert.equal(initial.admission_ready, true);
  assert.equal(initial.sealed_release.original_gameplay_qualified, false);
  await lot("initializeControlledV2", [
    initial.binding,
    initial.cursor,
    sha(Buffer.from(initial.checkpoint_hex, "hex")),
    {
      authority_id: "b".repeat(64),
      content_release: initial.binding.binding_sha256,
      protocol_release: "c".repeat(64),
      permission_binding: initial.permission_binding,
      actors: [],
    },
  ]);
  let cursor = initial.cursor,
    first;
  for (let batch = 1; batch <= 64; batch++) {
    const request = {
      kind: "prepare_player_tick",
      batch_id: String(batch),
      binding: initial.binding,
      predecessor: cursor,
      expected_control_revision: "0",
    };
    const proposal = await host.ask(request);
    assert.equal(proposal.kind, "proposal", JSON.stringify(proposal));
    const receipt = await lot("commitControlledBatchV2", [proposal]);
    assert.equal(receipt.status, "committed");
    const resolution = {
      kind: "resolve",
      batch_id: String(batch),
      digest: proposal.digest,
      decision: { status: "committed", cursor: receipt.cursor },
    };
    const reply = await host.ask(resolution);
    assert.equal(reply.kind, "committed");
    assert.equal(reply.receipt_hex, null);
    cursor = reply.cursor;
    if (batch === 1) first = { request, proposal, receipt, resolution, reply };
    if (batch % 32 === 0) {
      const runtime = await host.ask({ kind: "inspect" }),
        stored = await lot("inspectControlledV2");
      assert.deepEqual(runtime.cursor, stored.cursor);
      const control = {
        control_revision: stored.control_revision,
        actor_registry_sha256: stored.actor_registry_sha256,
        actors: stored.actors,
        permission_binding: stored.permission_binding,
      };
      const manifest = await lot("publishControlledCheckpointV2", [
        runtime.binding,
        runtime.cursor,
        sha(Buffer.from(runtime.checkpoint_hex, "hex")),
        [...Buffer.from(runtime.checkpoint_hex, "hex")],
        control,
      ]);
      if (batch === 32) {
        // Persisted custody but a lost acknowledgement must leave host replies live.
        const exported = await host.ask({ kind: "export_receipts" });
        const ack = await lot("persistHostReceiptsV1", [
          exported.payload,
          exported.digest,
        ]);
        assert.equal(
          (await host.ask({ kind: "inspect" })).retained_receipts,
          32,
        );
        await mf.dispose();
        await start();
        assert.deepEqual(
          await lot("persistHostReceiptsV1", [
            exported.payload,
            exported.digest,
          ]),
          ack,
        );
        checks.push(
          "Rust canonical receipt export matches TypeScript hash and complete nullable replies; a durable ack lost before host retirement survives real workerd restart without dropping the host cache",
        );
      }
      await retireCheckpointReceipts(host, lot, manifest);
      assert.equal((await host.ask({ kind: "inspect" })).retained_receipts, 0);
      assert.equal((await lot("inspect")).journal.count, 0);
    }
  }
  assert.equal(
    (await host.ask({ kind: "lookup", batch_id: "1" })).kind,
    "retired",
  );
  for (const request of [
    first.request,
    first.resolution,
    { kind: "lookup", batch_id: "1" },
  ])
    assert.deepEqual(
      await askWithDurableReceipts(host, lot, request),
      first.reply,
    );
  await assert.rejects(
    askWithDurableReceipts(host, lot, {
      ...first.request,
      expected_control_revision: "99",
    }),
    /Retired batch input differs/,
  );
  await assert.rejects(
    askWithDurableReceipts(host, lot, {
      ...first.resolution,
      digest: "0".repeat(64),
    }),
    /Retired batch digest differs/,
  );
  assert.deepEqual(
    await lot("commitControlledBatchV2", [first.proposal]),
    first.receipt,
  );
  assert.deepEqual(await lot("readBatch", ["1", "1"]), first.proposal.payload);
  const before = await host.ask({ kind: "inspect" });
  assert.deepEqual(before.cursor, cursor);
  assert.equal(before.retired_through_batch_id, "64");
  await mf.dispose();
  await start();
  assert.deepEqual(
    await askWithDurableReceipts(host, lot, { kind: "lookup", batch_id: "1" }),
    first.reply,
  );
  assert.deepEqual((await host.ask({ kind: "inspect" })).cursor, before.cursor);
  checks.push(
    "64 actual controlled Rust Host ticks cross two production export/persist/ack boundaries; retired prepare/resolve/lookup return byte-equivalent historical replies through real workerd archives, changed identities refuse and no tick reexecutes",
  );
  await mkdir("../../tests/output/journal-compaction", { recursive: true });
  const report = {
    passed: true,
    scope:
      "Production Rust Host through existing authored fixture + actual workerd SQLite/R2 + production private receipt supervisor;64 controlled ticks, no original asset/gameplay claim",
    checks,
  };
  await writeFile(
    "../../tests/output/journal-compaction/host-integration-report.json",
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report, null, 2));
} finally {
  await mf?.dispose();
  if (child && child.exitCode === null) {
    const closed = once(child, "exit");
    child.kill("SIGTERM");
    await closed;
  }
}
