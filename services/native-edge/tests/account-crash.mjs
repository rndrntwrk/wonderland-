import assert from "node:assert/strict";
import { fork, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { once } from "node:events";
if (process.platform === "win32") throw Error("POSIX process group required");
const root = await mkdtemp(join(tmpdir(), "wonderland-avatar-crash-"));
const deadline = (promise, label) =>
  Promise.race([
    promise,
    new Promise((_, reject) => {
      const t = setTimeout(() => reject(Error(label + " timeout")), 15000);
      t.unref();
    }),
  ]);
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
  worker = fork("tests/account-crash-worker.mjs", [root, barrierUrl], {
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
const interrupt = async (point, args) => {
  let reached;
  const waiting = new Promise((resolve) => (reached = resolve));
  blocked = reached;
  let replied = false;
  const pending = call("createAvatar", args, point).then(
    () => {
      replied = true;
      return "unexpected reply";
    },
    () => "connection lost",
  );
  await deadline(waiting, "avatar crash barrier");
  assert.equal(replied, false);
  await kill();
  assert.equal(await pending, "connection lost");
  blocked = null;
};
try {
  await start();
  const release = "b".repeat(64),
    origin = "a".repeat(64),
    password = "local-avatar-crash-only-phrase";
  await call("initializeCatalog", [
    {
      release,
      entries: [
        {
          key: "9007199254740993",
          part: "head",
          gender: "female",
          skins: ["dark"],
        },
        {
          key: "18446744073709551615",
          part: "body",
          gender: "female",
          skins: ["dark"],
        },
      ],
    },
  ]);
  await call("register", ["crash_owner", password, origin]);
  const session = await call("login", ["crash_owner", password, origin]);
  const spec = {
    name: "Crash Traveller",
    description: "",
    gender: "female",
    skin: "dark",
    head_key: "9007199254740993",
    body_key: "18446744073709551615",
    release,
  };
  const args = [session.token, "avatar_crash_create_01", spec];
  const before = await call("snapshot");
  await interrupt("before", args);
  await start();
  const recoveredBefore = await call("snapshot");
  for (const table of [
    "avatars",
    "operations",
    "sequences",
    "outfits",
    "balances",
    "initializations",
  ])
    assert.deepEqual(recoveredBefore[table], before[table]);
  await interrupt("after", args);
  await start();
  const durable = await call("snapshot");
  assert.equal(durable.avatars.length, 1);
  assert.equal(durable.operations.length, 1);
  assert.equal(durable.outfits.length, 3);
  assert.equal(durable.balances.length, 1);
  assert.equal(durable.initializations.length, 1);
  const expected = JSON.parse(durable.operations[0].result);
  assert.deepEqual(await call("createAvatar", args), expected);
  const assets = await call("ownedAssets", [session.token, expected.avatar_id]);
  assert.equal(assets.balance, "0");
  assert.equal(assets.outfits.length, 3);
  assert.deepEqual(
    assets.outfits.map((x) => x.asset_key),
    [spec.body_key, "5587752452109", "420906795021"],
  );
  const retried = await call("snapshot");
  for (const table of [
    "avatars",
    "operations",
    "sequences",
    "outfits",
    "balances",
    "initializations",
  ])
    assert.deepEqual(retried[table], durable[table]);
  const evidence = {
    status: "passed",
    scenario:
      "actual workerd SIGKILL before creation and after durable commit before HTTP reply; exact retry with one complete grant set",
    terminatedGroups,
  };
  const evidenceDirectory = resolve("../../tests/output/native-accounts");
  await mkdir(evidenceDirectory, { recursive: true });
  await writeFile(
    join(evidenceDirectory, "avatar-ownership-crash.json"),
    JSON.stringify(evidence, null, 2),
  );
  console.log(JSON.stringify(evidence));
} finally {
  if (worker) {
    worker.send("close");
    await deadline(once(worker, "exit"), "worker close");
  }
  for (const socket of sockets) socket.destroy();
  barrier.close();
}
