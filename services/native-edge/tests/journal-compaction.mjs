// Actual workerd SQLite/R2 storage-contract vectors. Synthetic bounded envelopes
// deliberately test the coordinator, not native VM or playable-game fidelity.
import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import assert from "node:assert/strict";
const root = await mkdtemp(join(tmpdir(), "wonderland-compaction-"));
const bundled = await build({
  entryPoints: ["tests/compaction-driver.ts"],
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
const call = async (op, args = [], lot = "compaction") => {
  const r = await mf.dispatchFetch("http://test/", {
    method: "POST",
    body: JSON.stringify({ op, args, lot }),
  });
  const result = await r.json();
  if (!r.ok) throw Error(result.error);
  return result;
};
export const canonical = (v) =>
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
export const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const b = {
  protocol: 1,
  source_pin: "4c6b3e8f5835b228723caea3c9f683c62f244f73",
  binding_sha256: "b".repeat(64),
  lot_incarnation: "1",
};
const snapshot = (n) => Buffer.from(`storage-contract-checkpoint-${n}`);
const cur = (n, epoch = "9007199254740993") => ({
  lot_id: "16777472",
  authority_epoch: epoch,
  completed_tick: String(n),
  state_hash: sha(`state-${epoch}-${n}`),
});
const permission = { revision: "1", sha256: "d".repeat(64) },
  registry = sha("[]");
const control = {
  control_revision: "0",
  actor_registry_sha256: registry,
  actors: [],
  permission_binding: permission,
};
const setup = {
  authority_id: "a".repeat(64),
  content_release: "b".repeat(64),
  protocol_release: "c".repeat(64),
  permission_binding: permission,
  actors: [],
};
const make = (
  n,
  {
    controlled = false,
    binding = b,
    epoch = "9007199254740993",
    effects = false,
  } = {},
) => {
  const payload = {
    binding,
    batch_id: String(n),
    predecessor: cur(n - 1, epoch),
    result: cur(n, epoch),
    ticks_hex: "01020304",
    checkpoint_sha256: sha(snapshot(n)),
    effects_hex: effects ? "0100000000000000ab" : "0000000000000000",
    events_hex: effects ? "0100000000000000cd" : "0000000000000000",
  };
  if (controlled)
    payload.control_v2 = {
      schema_version: 2,
      revision_before: "0",
      revision_after: "0",
      actor_registry_before_sha256: registry,
      actor_registry_after_sha256: registry,
      actors: [],
      permission_before: permission,
      permission_after: permission,
    };
  return {
    kind: "proposal",
    status: "pending",
    digest: sha(canonical(payload)),
    payload,
  };
};
const init = (lot, controlled = false) =>
  call(
    controlled ? "initializeControlledV2" : "initialize",
    [b, cur(0), sha(snapshot(0)), ...(controlled ? [setup] : [])],
    lot,
  );
const publish = (n, lot, controlled = false, ctl = control) =>
  call(
    controlled ? "publishControlledCheckpointV2" : "publishCheckpoint",
    [
      b,
      cur(n),
      sha(snapshot(n)),
      [...snapshot(n)],
      ...(controlled ? [ctl] : []),
    ],
    lot,
  );
const checks = [];
try {
  await start();
  for (const controlled of [false, true]) {
    const lot = controlled ? "long-controlled" : "long-legacy";
    await init(lot, controlled);
    for (let start = 1; start <= 4224; start += 32) {
      const proposals = Array.from({ length: 32 }, (_, i) =>
        make(start + i, { controlled, effects: start + i === 1 }),
      );
      await call("commitMany", [proposals, controlled], lot);
      await publish(start + 31, lot, controlled);
      const state = await call("inspect", [], lot);
      assert.equal(state.journal.count, 0);
      assert.equal(state.journal.bytes, 0);
      assert.equal(state.retained_receipts, start + 31);
    }
    const state = await call("inspect", [], lot);
    assert.equal(state.archived_batches, 4224);
    assert.equal(state.pending_outbox, 2);
    assert.deepEqual(
      await call("readBatch", ["1", "1"], lot),
      make(1, { controlled, effects: true }).payload,
    );
    const first = await call("getBatch", ["1", "1"], lot);
    assert.deepEqual(
      await call(
        controlled ? "commitControlledBatchV2" : "commitBatch",
        [make(1, { controlled, effects: true })],
        lot,
      ),
      first,
    );
    const reused = make(1, { controlled, effects: true });
    reused.payload.ticks_hex = "ff";
    reused.digest = sha(canonical(reused.payload));
    await assert.rejects(
      call(
        controlled ? "commitControlledBatchV2" : "commitBatch",
        [reused],
        lot,
      ),
      /identity reused/,
    );
    if (controlled)
      assert.deepEqual(
        (await call("inspectControlledV2", [], lot)).permission_binding,
        permission,
      );
  }
  checks.push(
    "4224 committed batches in each legacy and controlled V2 mode cross the old 4096 limit; repeated verified checkpoints bound hot count/bytes while every exact receipt and pending outbox payload remains available",
  );
  await mf.dispose();
  await start();
  for (const controlled of [false, true]) {
    const lot = controlled ? "long-controlled" : "long-legacy";
    assert.deepEqual(
      await call("readBatch", ["1", "1"], lot),
      make(1, { controlled, effects: true }).payload,
    );
    await call(
      "commitMany",
      [[make(4225, { controlled }), make(4226, { controlled })], controlled],
      lot,
    );
    await call(
      controlled ? "beginControlledRecoveryV2" : "beginRecovery",
      ["1"],
      lot,
    );
    const bundle = await call(
      controlled ? "controlledRecoveryBundleV2" : "recoveryBundle",
      ["1"],
      lot,
    );
    assert.deepEqual(
      bundle.batches.map((p) => p.batch_id),
      ["4225", "4226"],
    );
    assert.equal(bundle.checkpoint.sha256, sha(snapshot(4224)));
    if (controlled) {
      assert.deepEqual(bundle.control, control);
      assert.deepEqual(bundle.checkpoint.control_v2, control);
    }
  }
  checks.push(
    "Restart preserves historical receipt/payload lookup, unresolved outboxes and checksum-pinned checkpoint plus exact two-batch recovery tail with V2 registry/permission binding",
  );
  // Crossing the old lifetime byte limit must not require dropping history.
  await init("byte-retention");
  for (let first = 1; first <= 72; first += 8) {
    const proposals = Array.from({ length: 8 }, (_, i) => {
      const p = make(first + i);
      p.payload.ticks_hex = "ab".repeat(512 * 1024);
      p.digest = sha(canonical(p.payload));
      return p;
    });
    await call("commitMany", [proposals, false], "byte-retention");
    await publish(first + 7, "byte-retention");
  }
  assert.equal((await call("inspect", [], "byte-retention")).journal.bytes, 0);
  assert.equal(
    (await call("readBatch", ["1", "1"], "byte-retention")).ticks_hex.length,
    1024 * 1024,
  );
  checks.push(
    "More than 72 MiB of cumulative canonical journal bytes are retained across bounded archive segments while hot storage returns to zero",
  );
  // Full host replies are separate durable records, never reconstructed guesses.
  await init("host-receipts-workerd");
  const hostProposals = [make(1, { effects: true }), make(2)];
  await call("commitMany", [hostProposals, false], "host-receipts-workerd");
  await publish(2, "host-receipts-workerd");
  const envelope = {
    schema_version: 1,
    binding: b,
    cursor: cur(2),
    checkpoint_sha256: sha(snapshot(2)),
    through_batch_id: "2",
    records: hostProposals.map((p) => ({
      batch_id: p.payload.batch_id,
      input_sha256: sha("original-input-" + p.payload.batch_id),
      digest: p.digest,
      reply: {
        kind: "committed",
        batch_id: p.payload.batch_id,
        digest: p.digest,
        cursor: p.payload.result,
        ticks_hex: p.payload.ticks_hex,
        effects_hex: p.payload.effects_hex,
        events_hex: p.payload.events_hex,
        control_v2: null,
        receipt_hex: null,
      },
    })),
  };
  const receiptSha = sha(canonical(envelope));
  const ack = await call(
    "persistHostReceiptsV1",
    [envelope, receiptSha],
    "host-receipts-workerd",
  );
  assert.equal(ack.status, "persisted");
  assert.equal(ack.receipt_set_sha256, receiptSha);
  assert.deepEqual(
    await call(
      "persistHostReceiptsV1",
      [envelope, receiptSha],
      "host-receipts-workerd",
    ),
    ack,
  );
  const altered = structuredClone(envelope);
  altered.records[0].reply.effects_hex = "abcd";
  await assert.rejects(
    call(
      "persistHostReceiptsV1",
      [altered, sha(canonical(altered))],
      "host-receipts-workerd",
    ),
    /accepted payload/,
  );
  await mf.dispose();
  await start();
  assert.deepEqual(
    await call("lookupHostReceiptV1", ["1", "1"], "host-receipts-workerd"),
    { status: "recorded", record: envelope.records[0] },
  );
  checks.push(
    "Actual workerd persists and restarts exact full host input-hash/reply archives only against verified accepted payloads and a published checkpoint; changed effect bytes are refused",
  );
  // Failed archive writes/readback and transaction failure never remove SQL bytes.
  for (const mode of [
    "put-failure",
    "readback-failure",
    "index-failure",
    "existing-corrupt",
  ]) {
    const lot = mode;
    await init(lot);
    const proposals = [make(1, { effects: true }), make(2)].map((p) => {
      p.payload.ticks_hex = sha(mode + p.payload.batch_id).slice(0, 8);
      p.digest = sha(canonical(p.payload));
      return p;
    });
    await call("commitMany", [proposals, false], lot);
    const before = await call("rawCounts", [], lot),
      outbox = await call("inspectOutbox", [], lot);
    if (mode === "index-failure") await call("failIndex", ["1:2"], lot);
    else if (mode === "existing-corrupt") {
      const body = Buffer.concat(
        proposals.map((p) => Buffer.from(canonical(p.payload))),
      );
      await (
        await mf.getR2Bucket("CHECKPOINTS")
      ).put(
        `lots/${cur(0).lot_id}/journal/${sha(body)}`,
        Buffer.alloc(body.length, 1),
      );
    } else await call("setArchiveFault", [mode], lot);
    await assert.rejects(publish(2, lot), /injected|archive/);
    assert.deepEqual(await call("rawCounts", [], lot), before);
    assert.deepEqual(await call("inspectOutbox", [], lot), outbox);
    for (const p of proposals)
      assert.deepEqual(
        await call("readBatch", ["1", p.payload.batch_id], lot),
        p.payload,
      );
    await call("setArchiveFault", [""], lot);
    await call("failIndex", [null], lot);
    if (mode === "existing-corrupt") {
      const body = Buffer.concat(
        proposals.map((p) => Buffer.from(canonical(p.payload))),
      );
      await (
        await mf.getR2Bucket("CHECKPOINTS")
      ).put(`lots/${cur(0).lot_id}/journal/${sha(body)}`, body);
    }
    await publish(2, lot);
    assert.equal((await call("rawCounts", [], lot)).chunks, 0);
  }
  checks.push(
    "Archive put/readback failures, preexisting corrupt content-addressed object and aborted SQLite pointer transaction retain full SQL payloads; exact retry compacts successfully",
  );
  const lot = "races";
  await init(lot);
  await call("commitBatch", [make(1)], lot);
  await call("setArchiveFault", ["pause-readback"], lot);
  const publishing = publish(1, lot);
  for (let i = 0; !(await call("archivePaused", [], lot)); i++) {
    assert.ok(i < 200);
    await new Promise((r) => setTimeout(r, 5));
  }
  await call("commitBatch", [make(2)], lot);
  const grant = await call("beginRecovery", ["1"], lot);
  await call("resumeArchive", [], lot);
  await publishing;
  const bundle = await call("recoveryBundle", ["1"], lot);
  assert.deepEqual(bundle.grant, grant);
  assert.deepEqual(
    bundle.batches.map((p) => p.batch_id),
    ["2"],
  );
  assert.equal((await call("inspect", [], lot)).journal.count, 1);
  checks.push(
    "A successor and recovery fence arriving during archive R2 I/O remain intact; only the verified checkpoint prefix is tiered",
  );
  const archived = (await call("archiveRows", [], lot))[0],
    bucket = await mf.getR2Bucket("CHECKPOINTS"),
    saved = Buffer.from(
      await (await bucket.get(archived.object_key)).arrayBuffer(),
    );
  await bucket.delete(archived.object_key);
  await assert.rejects(call("readBatch", ["1", "1"], lot), /missing/);
  await bucket.put(archived.object_key, Buffer.alloc(saved.length, 3));
  await assert.rejects(call("readBatch", ["1", "1"], lot), /digest mismatch/);
  await bucket.put(archived.object_key, saved);
  assert.deepEqual(await call("readBatch", ["1", "1"], lot), make(1).payload);
  checks.push(
    "Missing and corrupt archived payloads fail closed; exact retained receipt remains independently queryable",
  );
  await init("controlled-publication-race", true);
  await call(
    "commitControlledBatchV2",
    [make(1, { controlled: true })],
    "controlled-publication-race",
  );
  await call("pauseCheckpoint", [], "controlled-publication-race");
  const controlledPublish = publish(
    1,
    "controlled-publication-race",
    true,
  ).then(
    (value) => ({ value }),
    (error) => ({ error: String(error) }),
  );
  for (
    let i = 0;
    !(await call("checkpointPaused", [], "controlled-publication-race"));
    i++
  ) {
    assert.ok(i < 200);
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  await call(
    "recordControlFence",
    [
      {
        version: 1,
        authority: setup.authority_id,
        avatar_id: "7",
        generation: "1",
        lot_id: cur(0).lot_id,
        request_digest: "e".repeat(64),
      },
    ],
    "controlled-publication-race",
  );
  await call("resumeCheckpoint", [], "controlled-publication-race");
  assert.match(
    (await controlledPublish).error,
    /controlled checkpoint revision or registry changed/,
  );
  assert.equal(
    (await call("inspect", [], "controlled-publication-race")).journal.count,
    1,
  );
  assert.deepEqual(
    await call("listCheckpoints", [], "controlled-publication-race"),
    [],
  );
  checks.push(
    "Control-fence revision changes during checkpoint R2 I/O refuse obsolete V2 publication and leave every hot payload untouched",
  );
  // Pre-upgrade rows acquire live accounting exactly once after restart.
  await init("migration");
  await call("commitBatch", [make(1)], "migration");
  await call("legacyLayout", [], "migration");
  await mf.dispose();
  await start();
  assert.equal((await call("inspect", [], "migration")).journal.count, 1);
  await publish(1, "migration");
  await mf.dispose();
  await start();
  assert.equal((await call("inspect", [], "migration")).journal.count, 0);
  assert.equal((await call("inspect", [], "migration")).retained_receipts, 1);
  checks.push(
    "Restart-safe legacy-layout migration counts existing hot rows once and never resurrects compacted rows",
  );
  await mkdir("../../tests/output/journal-compaction", { recursive: true });
  const report = {
    passed: true,
    scope:
      "Local actual workerd SQLite/R2 storage contracts with synthetic proposal/checkpoint envelopes; no native VM/playability claim, no deployment",
    checks,
  };
  await writeFile(
    "../../tests/output/journal-compaction/report.json",
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report, null, 2));
} finally {
  await mf?.dispose();
}
