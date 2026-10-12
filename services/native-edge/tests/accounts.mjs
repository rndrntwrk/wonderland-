import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { readFile, mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import assert from "node:assert/strict";
const root = await mkdtemp(join(tmpdir(), "wonderland-accounts-"));
const result = await build({
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
const start = async () => {
  mf = new Miniflare({
    modulesRoot: root,
    modules: [
      {
        type: "ESModule",
        path: join(root, "worker.mjs"),
        contents: result.outputFiles[0].text,
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
};
const call = async (op, args = [], realm) => {
  const r = await mf.dispatchFetch("https://test/", {
    method: "POST",
    body: JSON.stringify({ op, args, realm }),
  });
  const v = await r.json();
  if (!r.ok) throw Error(v.error);
  return v;
};
const origin = "a".repeat(64),
  release = "b".repeat(64),
  password = "local-account-only-strong-passphrase";
const catalog = {
  release,
  entries: [
    {
      key: "9007199254740993",
      part: "head",
      gender: "female",
      skins: ["light", "medium", "dark"],
    },
    {
      key: "18446744073709551615",
      part: "body",
      gender: "female",
      skins: ["medium", "dark"],
    },
    { key: "9007199254740995", part: "head", gender: "male", skins: ["light"] },
    { key: "9007199254740997", part: "body", gender: "male", skins: ["light"] },
  ],
};
const female = {
  name: "First Traveller",
  description: "Independently selected appearance",
  gender: "female",
  skin: "dark",
  head_key: "9007199254740993",
  body_key: "18446744073709551615",
  release,
};
const male = {
  name: "Second Traveller",
  description: "Separate account and appearance",
  gender: "male",
  skin: "light",
  head_key: "9007199254740995",
  body_key: "9007199254740997",
  release,
};
const checks = [];
try {
  await start();
  assert.deepEqual(await call("knownVector"), {
    hash: "2065f778ea1944dbd5040c4800a565a2da33cacfa7e57ecc436b4f68f9be25d5",
    cleared: true,
  });
  await assert.rejects(call("sparseCatalog", [release, "entries"]));
  await assert.rejects(call("sparseCatalog", [release, "skins"]));
  await call("initializeCatalog", [catalog]);
  await assert.rejects(
    call("initializeCatalog", [{ ...catalog, release: "c".repeat(64) }]),
    /already pinned/,
  );
  const unicodeOrigin = "e".repeat(64);
  for (const suffix of ["\ud800", "\udfff", "\ud800x", "x\udfff"]) {
    await assert.rejects(call("register", ["unicode_reject", password + suffix, unicodeOrigin]), /invalid password Unicode/);
  }
  const unicodePassword = password + "💡é\ufffd";
  await call("register", ["unicode_valid", unicodePassword, unicodeOrigin]);
  await call("login", ["unicode_valid", unicodePassword, unicodeOrigin]);
  await assert.rejects(call("login", ["unicode_valid", password + "💡é\ud800", unicodeOrigin]), /invalid password Unicode/);
  checks.push("actual account RPC rejects lone surrogates while supplementary Unicode and literal replacement characters remain valid");
  const a = await call("register", ["Account_A", password, origin]);
  const b = await call("register", ["account_b", password + "-b", origin]);
  assert.notEqual(a.account_id, b.account_id);
  assert.equal(a.username, "account_a");
  await assert.rejects(
    call("register", ["ACCOUNT_A", password, origin]),
    /unavailable/,
  );
  await assert.rejects(
    call("login", ["account_a", password + "wrong", origin]),
    /unauthorized/,
  );
  const sa = await call("login", ["account_a", password, origin]);
  const sb = await call("login", ["account_b", password + "-b", origin]);
  const av = await call("createAvatar", [
    sa.token,
    "create_avatar_a_0001",
    female,
  ]);
  const bv = await call("createAvatar", [
    sb.token,
    "create_avatar_b_0001",
    male,
  ]);
  assert.notEqual(av.avatar_id, bv.avatar_id);
  assert.equal(av.head_key, female.head_key);
  assert.equal(av.body_key, female.body_key);
  assert.deepEqual((await call("roster", [sa.token])).avatars, [av]);
  assert.deepEqual((await call("roster", [sb.token])).avatars, [bv]);
  await assert.rejects(
    call("ownedAvatar", [sb.token, av.avatar_id]),
    /unavailable/,
  );
  await assert.rejects(call("roster", ["0".repeat(64)]), /unauthorized/);
  checks.push(
    "real Argon2id registration/login and two persisted isolated rosters",
  );
  const repeats = await Promise.all([
    call("createAvatar", [sa.token, "create_avatar_a_0001", female]),
    call("createAvatar", [sa.token, "create_avatar_a_0001", female]),
  ]);
  assert.deepEqual(repeats, [av, av]);
  await assert.rejects(
    call("createAvatar", [
      sa.token,
      "create_avatar_a_0001",
      { ...female, name: "Changed body" },
    ]),
    /identity reused/,
  );
  await assert.rejects(
    call("createAvatar", [sb.token, "create_avatar_b_0002", { ...female }]),
    /name unavailable/,
  );
  for (const spec of [
    { ...female, name: "Wrong release", release: "c".repeat(64) },
    { ...female, name: "Wrong gender", gender: "male" },
    { ...female, name: "Wrong skin", skin: "light" },
    { ...female, name: "Wrong key", head_key: "9007199254740999" },
    { ...female, name: "Spoof account", account_id: b.account_id },
    { ...female, name: "Rounded key", head_key: 9007199254740992 },
  ])
    await assert.rejects(
      call("createAvatar", [sa.token, "invalid_specification_0001", spec]),
    );
  assert.equal((await call("roster", [sa.token])).avatars.length, 1);
  for (const name of ["1st Avatar", "Emoji 🐈", "a".repeat(25)]) {
    await assert.rejects(
      call("createAvatar", [
        sa.token,
        "name_validation_0001",
        { ...female, name },
      ]),
    );
  }
  await assert.rejects(
    call("createAvatar", [
      sa.token,
      "description_limit_001",
      { ...female, name: "Description Bound", description: "a".repeat(500) },
    ]),
  );
  checks.push(
    "atomic operation retry, changed-body rejection, duplicate name and pinned independent appearances",
  );
  await mf.dispose();
  await start();
  assert.deepEqual((await call("roster", [sa.token])).avatars, [av]);
  assert.deepEqual(
    await call("createAvatar", [sa.token, "create_avatar_a_0001", female]),
    av,
  );
  const rotated = await call("rotate", [sa.token]);
  await assert.rejects(call("roster", [sa.token]), /unauthorized/);
  assert.deepEqual((await call("roster", [rotated.token])).avatars, [av]);
  const races = await Promise.allSettled([
    call("rotate", [rotated.token]),
    call("rotate", [rotated.token]),
  ]);
  assert.equal(races.filter((r) => r.status === "fulfilled").length, 1);
  const live = races.find((r) => r.status === "fulfilled").value;
  const snapshot = await call("snapshot");
  const serialized = JSON.stringify(snapshot);
  for (const secret of [
    password,
    sa.token,
    sb.token,
    rotated.token,
    live.token,
  ])
    assert.ok(!serialized.includes(secret));
  assert.equal(snapshot.accounts.length, 3);
  assert.equal(
    new Set(snapshot.accounts.map((x) => JSON.parse(x.verifier).salt)).size,
    3,
  );
  checks.push(
    "SQLite restart, exact retry, single-use rotation races and no stored plaintext secrets",
  );
  const sibling = await call("login", ["account_a", password, origin]);
  const secondSpec = { ...female, name: "Another Traveller" };
  const concurrentFirst = await Promise.all([
    call("createAvatar", [live.token, "first_create_race_0001", secondSpec]),
    call("createAvatar", [live.token, "first_create_race_0001", secondSpec]),
  ]);
  assert.deepEqual(concurrentFirst[0], concurrentFirst[1]);
  await call("createAvatar", [
    live.token,
    "third_avatar_source_001",
    { ...female, name: "Third Traveller", description: "line one\nline two" },
  ]);
  await assert.rejects(
    call("createAvatar", [
      live.token,
      "fourth_avatar_source_01",
      { ...female, name: "Fourth Traveller" },
    ]),
    /source account avatar limit/,
  );
  assert.deepEqual(
    await call("createAvatar", [live.token, "create_avatar_a_0001", female]),
    av,
  );
  assert.equal((await call("roster", [live.token])).avatars.length, 3);
  checks.push(
    "first-create concurrency and original three-avatar ownership policy",
  );
  await call("logout", [live.token, true]);
  await assert.rejects(call("roster", [sibling.token]), /unauthorized/);
  await assert.rejects(call("roster", [live.token]), /unauthorized/);
  assert.equal((await call("roster", [sb.token])).avatars.length, 1);
  await call("advance", [31 * 60 * 1000]);
  await assert.rejects(call("roster", [sb.token]), /unauthorized/);
  const again = await call("login", ["account_b", password + "-b", origin]);
  await call("advance", [12 * 60 * 60 * 1000]);
  await assert.rejects(call("roster", [again.token]), /unauthorized/);
  checks.push(
    "logout and idle/absolute expiry preserve the other account and saved avatars",
  );
  for (let i = 0; i < 5; i++)
    await assert.rejects(
      call("login", ["unknown_account", password, origin]),
      /unauthorized/,
    );
  await assert.rejects(
    call("login", ["unknown_account", password, origin]),
    /temporarily busy/,
  );
  await mf.dispose();
  await start();
  await assert.rejects(
    call("login", ["unknown_account", password, origin]),
    /temporarily busy/,
  );
  await call("advance", [60001]);
  await assert.rejects(
    call("login", ["unknown_account", password, origin]),
    /unauthorized/,
  );
  checks.push(
    "durable bounded authentication rate windows, including unknown users and restart",
  );
  for (let i = 0; i < 20; i++)
    await assert.rejects(
      call("login", ["origin_user_" + i, password, "d".repeat(64)]),
      /unauthorized/,
    );
  await assert.rejects(
    call("login", ["origin_user_twenty", password, "d".repeat(64)]),
    /temporarily busy/,
  );
  checks.push("network-wide throttling across distinct usernames");
  const production = new Miniflare({
    modulesRoot: root,
    modules: [
      {
        type: "ESModule",
        path: join(root, "production.mjs"),
        contents: await readFile("dist/worker.mjs", "utf8"),
      },
      {
        type: "CompiledWasm",
        path: join(root, "account_crypto_bg.wasm"),
        contents: wasm,
      },
    ],
    compatibilityDate: "2026-07-01",
  });
  try {
    for (const path of [
      "/",
      "/account/login",
      "/account/register",
      "/debug",
      "/rpc/createAvatar",
    ])
      assert.equal(
        (
          await production.dispatchFetch("https://test" + path, {
            method: "POST",
            body: "{}",
          })
        ).status,
        404,
      );
  } finally {
    await production.dispose();
  }
  const dir = resolve("../../tests/output/native-accounts");
  await mkdir(dir, { recursive: true });
  await writeFile(
    join(dir, "account-journey.json"),
    JSON.stringify(
      {
        status: "passed",
        publicRoutesDenied: 5,
        checks,
        profile: "argon2id-v19-m19456-t2-p1-l32",
        runtime: "real local workerd with Rust WASM and SQLite",
        scope:
          "private account boundary; no public signup, email recovery, browser UI or lot/travel admission claimed",
      },
      null,
      2,
    ),
  );
  console.log(
    JSON.stringify({
      status: "passed",
      checks: checks.length,
      publicRoutesDenied: 5,
    }),
  );
} finally {
  await mf?.dispose();
}
