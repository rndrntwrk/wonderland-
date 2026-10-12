// Actual native proposals and abrupt workerd loss; no fabricated successful state.
import assert from "node:assert/strict";
import { spawn, fork, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { createInterface } from "node:readline";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { once } from "node:events";
if (process.platform === "win32")
  throw Error("Abrupt process-group witness requires POSIX");
const repo = resolve("../.."),
  root = await mkdtemp(join(tmpdir(), "wonderland-crash-")),
  release = join(root, "release");
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
const lines = [],
  waiters = [];
createInterface({ input: host.stdout }).on("line", (line) => {
  const value = JSON.parse(line);
  if (waiters.length) waiters.shift()(value);
  else lines.push(value);
});
const deadline = (promise, label) =>
  Promise.race([
    promise,
    new Promise((_, reject) => {
      const t = setTimeout(() => reject(Error(label + " timeout")), 15000);
      t.unref();
    }),
  ]);
const next = () =>
  deadline(
    lines.length
      ? Promise.resolve(lines.shift())
      : new Promise((resolve) => waiters.push(resolve)),
    "host",
  );
const ask = (q) => {
  host.stdin.write(JSON.stringify(q) + "\n");
  return next();
};
let blocked;
const sockets = new Set();
const barrier = createServer(() => {
  blocked?.(); /* Deliberately never reply. */
});
barrier.on("connection", (socket) => {
  sockets.add(socket);
  socket.on("close", () => sockets.delete(socket));
});
barrier.listen(0, "127.0.0.1");
await once(barrier, "listening");
const barrierUrl = `http://127.0.0.1:${barrier.address().port}/`;
let worker, url;
const terminatedGroups = [];
const start = async () => {
  worker = fork("tests/crash-worker.mjs", [root, barrierUrl], {
    detached: true,
    stdio: ["ignore", "ignore", "pipe", "ipc"],
  });
  let errors = "";
  worker.stderr.on("data", (d) => (errors += d));
  const ready = await deadline(
    new Promise((resolve, reject) => {
      worker.once("message", resolve);
      worker.once("exit", (code) =>
        reject(Error("worker exited " + code + " " + errors)),
      );
    }),
    "worker start",
  );
  url = ready.url;
};
const call = async (op, args = [], crash) => {
  const response = await fetch(url, {
    method: "POST",
    body: JSON.stringify({ op, args, crash }),
    signal: AbortSignal.timeout(15000),
  });
  const value = await response.json();
  if (!response.ok) throw Error(value.error);
  return value;
};
const kill = async (requireWorkerd = true) => {
  const members = execFileSync("ps", ["-eo", "pid=,pgid=,comm="], {
    encoding: "utf8",
  })
    .trim()
    .split("\n")
    .map((line) => line.trim().split(/\s+/))
    .filter((row) => Number(row[1]) === worker.pid);
  if (requireWorkerd)
    assert.ok(
      members.some((row) => row[2] === "workerd"),
      "Dedicated process group must contain real workerd",
    );
  const exited = once(worker, "exit");
  process.kill(-worker.pid, "SIGKILL");
  const [, signal] = await exited;
  assert.equal(signal, "SIGKILL");
  terminatedGroups.push({ members: members.map((row) => row[2]), signal });
  worker = null;
  for (const socket of sockets) socket.destroy();
};
const interrupt = async (point, proposal) => {
  let reached;
  const waiting = new Promise((resolve) => (reached = resolve));
  blocked = reached;
  let returned = false;
  const request = call("commitBatch", [proposal], point).then(
    () => {
      returned = true;
      return "unexpected reply";
    },
    () => "connection lost",
  );
  await deadline(waiting, "crash barrier");
  assert.equal(returned, false, "Receipt escaped before crash");
  await kill();
  assert.equal(await request, "connection lost");
  blocked = null;
};
try {
  const initial = await next(),
    bytes = Buffer.from(initial.checkpoint_hex, "hex");
  const proposal = await ask({
    kind: "prepare",
    batch_id: "1",
    binding: initial.binding,
    predecessor: initial.cursor,
    commands_hex: (
      await readFile(join(release, "commands.txt"), "utf8")
    ).trim(),
  });
  assert.equal(proposal.kind, "proposal");
  await start();
  await call("initialize", [
    initial.binding,
    initial.cursor,
    createHash("sha256").update(bytes).digest("hex"),
  ]);
  const before = await call("inspect");
  await interrupt("before", proposal);
  await start();
  assert.deepEqual(await call("inspect"), before);
  assert.deepEqual(await call("getBatch", ["1", "1"]), { status: "unknown" });
  await interrupt("after", proposal);
  await start();
  const receipt = await call("getBatch", ["1", "1"]);
  assert.deepEqual(receipt, {
    status: "committed",
    batch_id: "1",
    digest: proposal.digest,
    cursor: proposal.payload.result,
  });
  assert.equal(receipt.digest, proposal.digest);
  assert.deepEqual(receipt.cursor, proposal.payload.result);
  assert.deepEqual(await call("commitBatch", [proposal]), receipt);
  assert.deepEqual(await call("readBatch", ["1", "1"]), proposal.payload);
  assert.equal(
    (await ask({ kind: "inspect" })).checkpoint_hex,
    initial.checkpoint_hex,
    "Unacknowledged proposal became visible",
  );
  const committed = await ask({
    kind: "resolve",
    batch_id: "1",
    digest: receipt.digest,
    decision: { status: "committed", cursor: receipt.cursor },
  });
  assert.equal(committed.kind, "committed");
  assert.deepEqual((await ask({ kind: "inspect" })).cursor, receipt.cursor);
  const effectProposal = await ask({
    kind: "prepare",
    batch_id: "2",
    binding: initial.binding,
    predecessor: receipt.cursor,
    commands_hex: (
      await readFile(join(release, "effect-commands.txt"), "utf8")
    ).trim(),
  });
  assert.equal(effectProposal.kind, "proposal");
  await interrupt("after", effectProposal);
  await start();
  const effectReceipt = await call("getBatch", ["1", "2"]);
  assert.deepEqual(effectReceipt, {
    status: "committed",
    batch_id: "2",
    digest: effectProposal.digest,
    cursor: effectProposal.payload.result,
  });
  assert.equal(effectReceipt.digest, effectProposal.digest);
  const durable = await call("inspect");
  assert.equal(durable.pending_outbox, 1);
  const expectedOutbox = [
    { batch_key: "1:2", lane: "effects", digest: effectProposal.digest },
  ];
  assert.deepEqual(await call("inspectOutbox"), expectedOutbox);
  assert.deepEqual(await call("commitBatch", [effectProposal]), effectReceipt);
  assert.deepEqual(await call("inspect"), durable);
  assert.deepEqual(await call("inspectOutbox"), expectedOutbox);
  assert.deepEqual(await call("readBatch", ["1", "2"]), effectProposal.payload);
  const report = {
    terminated_groups: terminatedGroups,
    passed: true,
    scope:
      "Actual POSIX SIGKILL of Miniflare/workerd process group, persisted SQLite restart and real Rust host; local service evidence only",
    checks: [
      "Killed before commit: unchanged cursor and unknown receipt",
      "Killed after durable commit before HTTP reply: exact receipt/journal survives and retry deduplicates",
      "Rust candidate remains unpublished until recovered exact durable receipt",
      "Pending dialog effect survives pre-reply kill and retry does not duplicate the outbox",
    ],
  };
  const out = join(repo, "tests/output/native-durability");
  await mkdir(out, { recursive: true });
  await writeFile(
    join(out, "crash-receipts.json"),
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report, null, 2));
} finally {
  host.kill();
  if (worker && worker.exitCode === null && worker.signalCode === null)
    await kill(false);
  for (const socket of sockets) socket.destroy();
  barrier.close();
}
