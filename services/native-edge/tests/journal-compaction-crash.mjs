// Abrupt process loss across both R2->SQLite custody boundaries, with actual
// persisted workerd SQLite/R2. Synthetic envelopes test storage, not native VM.
import assert from "node:assert/strict";
import { fork, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { once } from "node:events";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
if (process.platform === "win32")
  throw Error("Requires POSIX process-group termination");
const root = await mkdtemp(join(tmpdir(), "wonderland-compaction-crash-"));
const sha = (x) => createHash("sha256").update(x).digest("hex");
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
const binding = {
  protocol: 1,
  source_pin: "4c6b3e8f5835b228723caea3c9f683c62f244f73",
  binding_sha256: "b".repeat(64),
  lot_incarnation: "1",
};
const cursor = (n) => ({
  lot_id: "16777472",
  authority_epoch: "1",
  completed_tick: String(n),
  state_hash: sha(`state-${n}`),
});
const checkpoint = Buffer.from("compaction-process-loss-checkpoint");
const payload = {
  binding,
  batch_id: "1",
  predecessor: cursor(0),
  result: cursor(1),
  ticks_hex: "010203",
  checkpoint_sha256: sha(checkpoint),
  effects_hex: "0100000000000000ab",
  events_hex: "0100000000000000cd",
};
const proposal = {
  kind: "proposal",
  status: "pending",
  digest: sha(canonical(payload)),
  payload,
};
const publishArgs = [binding, cursor(1), sha(checkpoint), [...checkpoint]];
let reached;
const sockets = new Set(),
  barrier = createServer(() => reached?.());
barrier.on("connection", (s) => {
  sockets.add(s);
  s.on("close", () => sockets.delete(s));
});
barrier.listen(0, "127.0.0.1");
await once(barrier, "listening");
const barrierUrl = `http://127.0.0.1:${barrier.address().port}/`;
let worker, url;
const timeout = (promise, label) =>
  Promise.race([
    promise,
    new Promise((_, reject) => {
      const timer = setTimeout(() => reject(Error(label + " timeout")), 20000);
      timer.unref();
    }),
  ]);
const start = async () => {
  worker = fork("tests/compaction-crash-worker.mjs", [root, barrierUrl], {
    detached: true,
    stdio: ["ignore", "ignore", "pipe", "ipc"],
  });
  let errors = "";
  worker.stderr.on("data", (d) => (errors += d));
  const ready = await timeout(
    new Promise((resolve, reject) => {
      worker.once("message", resolve);
      worker.once("exit", (code) =>
        reject(Error(`worker exited ${code}: ${errors}`)),
      );
    }),
    "start",
  );
  url = ready.url;
};
const call = async (op, args = [], lot = "crash", crash) => {
  const r = await fetch(url, {
    method: "POST",
    body: JSON.stringify({ op, args, lot, crash }),
    signal: AbortSignal.timeout(20000),
  });
  const result = await r.json();
  if (!r.ok) throw Error(result.error);
  return result;
};
const killed = [];
const kill = async () => {
  const members = execFileSync("ps", ["-eo", "pid=,pgid=,comm="], {
    encoding: "utf8",
  })
    .trim()
    .split("\n")
    .map((l) => l.trim().split(/\s+/))
    .filter((r) => Number(r[1]) === worker.pid);
  assert.ok(members.some((r) => r[2] === "workerd"));
  const exited = once(worker, "exit");
  process.kill(-worker.pid, "SIGKILL");
  const [, signal] = await exited;
  assert.equal(signal, "SIGKILL");
  killed.push({ signal, members: members.map((r) => r[2]) });
  worker = null;
  for (const s of sockets) s.destroy();
};
const checks = [];
try {
  await start();
  for (const phase of ["archive-written", "compacted"]) {
    await call(
      "initialize",
      [binding, cursor(0), sha(Buffer.from("initial"))],
      phase,
    );
    const receipt = await call("commitBatch", [proposal], phase),
      outbox = await call("inspectOutbox", [], phase);
    if (phase === "archive-written")
      await call("setArchiveFault", ["crash-written"], phase);
    const blocked = new Promise((resolve) => (reached = resolve));
    let returned = false;
    const request = call(
      "publishCheckpoint",
      publishArgs,
      phase,
      phase === "compacted" ? "compacted" : undefined,
    ).then(
      () => {
        returned = true;
        return "unexpected response";
      },
      () => "connection lost",
    );
    await timeout(blocked, "archive barrier");
    assert.equal(returned, false);
    await kill();
    assert.equal(await request, "connection lost");
    reached = null;
    await start();
    const state = await call("inspect", [], phase);
    assert.equal(state.journal.count, phase === "archive-written" ? 1 : 0);
    assert.equal(state.archived_batches, phase === "archive-written" ? 0 : 1);
    assert.deepEqual(await call("getBatch", ["1", "1"], phase), receipt);
    assert.deepEqual(await call("readBatch", ["1", "1"], phase), payload);
    assert.deepEqual(await call("inspectOutbox", [], phase), outbox);
    assert.deepEqual(await call("commitBatch", [proposal], phase), receipt);
    await call("publishCheckpoint", publishArgs, phase);
    assert.equal((await call("inspect", [], phase)).journal.count, 0);
    assert.deepEqual(await call("readBatch", ["1", "1"], phase), payload);
    checks.push(
      phase === "archive-written"
        ? "SIGKILL after durable R2 archive write, before readback/SQL pointers: restart retains hot journal, exact receipt/outboxes, and verified retry completes compaction"
        : "SIGKILL after atomic SQL archive publication/chunk removal, before caller reply: restart reads exact archived payload/receipt/outboxes and retry cannot duplicate effects",
    );
  }
  await mkdir("../../tests/output/journal-compaction", { recursive: true });
  const report = {
    passed: true,
    scope:
      "Actual workerd process groups killed with SIGKILL; synthetic storage envelopes; no native VM claim",
    checks,
    killed,
  };
  await writeFile(
    "../../tests/output/journal-compaction/crash-report.json",
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report, null, 2));
} finally {
  if (worker) {
    worker.send("close");
    await timeout(once(worker, "exit"), "close");
  }
  for (const s of sockets) s.destroy();
  barrier.close();
}
