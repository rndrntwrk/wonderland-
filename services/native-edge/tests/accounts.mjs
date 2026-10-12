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
  const capacityRealm = "avatar-source-capacity";
  await call("initializeCatalog", [catalog], capacityRealm);
  await call("register", ["capacity_owner", password, origin], capacityRealm);
  const capacitySession = await call(
    "login",
    ["capacity_owner", password, origin],
    capacityRealm,
  );
  await call("seedAvatarSequence", ["16777214"], capacityRealm);
  const finalAvatar = await call(
    "createAvatar",
    [capacitySession.token, "capacity_last_avatar_01", female],
    capacityRealm,
  );
  assert.equal(finalAvatar.avatar_id, "16777215");
  const beforeExhaustion = await call("snapshot", [], capacityRealm);
  const exhausted = await Promise.allSettled(
    ["Capacity First", "Capacity Second"].map((name, i) =>
      call(
        "createAvatar",
        [capacitySession.token, "capacity_exhausted_" + i, { ...female, name }],
        capacityRealm,
      ),
    ),
  );
  for (const result of exhausted) {
    assert.equal(result.status, "rejected");
    assert.match(result.reason.message, /identity capacity/);
  }
  const afterExhaustion = await call("snapshot", [], capacityRealm);
  assert.deepEqual(afterExhaustion.avatars, beforeExhaustion.avatars);
  assert.deepEqual(afterExhaustion.operations, beforeExhaustion.operations);
  assert.deepEqual(afterExhaustion.sequences, beforeExhaustion.sequences);
  for (const table of ["outfits", "balances", "initializations"])
    assert.deepEqual(afterExhaustion[table], beforeExhaustion[table]);
  assert.equal(beforeExhaustion.outfits.length, 3);
  assert.equal(beforeExhaustion.balances[0].balance, "0");
  await mf.dispose();
  await start();
  assert.deepEqual(
    await call(
      "createAvatar",
      [capacitySession.token, "capacity_last_avatar_01", female],
      capacityRealm,
    ),
    finalAvatar,
  );
  checks.push(
    "source avatar identity ceiling rejects concurrent exhaustion atomically and preserves last successful create after restart",
  );
  const ownershipRealm = "avatar-initial-ownership";
  const ownCall = (op, args = []) => call(op, args, ownershipRealm);
  await ownCall("initializeCatalog", [catalog]);
  await ownCall("register", ["wardrobe_f", password, origin]);
  await ownCall("register", ["wardrobe_m", password, origin]);
  const of = await ownCall("login", ["wardrobe_f", password, origin]);
  const om = await ownCall("login", ["wardrobe_m", password, origin]);
  const createArgs = [of.token, "ownership_create_female", female];
  const [first, replayed] = await Promise.all([
    ownCall("createAvatar", createArgs),
    ownCall("createAvatar", createArgs),
  ]);
  assert.deepEqual(first, replayed);
  const second = await ownCall("createAvatar", [
    om.token,
    "ownership_create_male_01",
    male,
  ]);
  const fa = await ownCall("ownedAssets", [of.token, first.avatar_id]);
  const ma = await ownCall("ownedAssets", [om.token, second.avatar_id]);
  for (const [actual, expected] of [
    [fa, [female.body_key, "5587752452109", "420906795021"]],
    [ma, [male.body_key, "5789615915021", "5802500816909"]],
  ]) {
    assert.equal(actual.balance, "0");
    assert.equal(actual.version, 1);
    assert.deepEqual(
      actual.outfits.map((x) => x.asset_key),
      expected,
    );
    assert.deepEqual(
      actual.outfits.map((x) => x.suit_type),
      [0, 5, 2],
    );
    for (const row of actual.outfits) {
      assert.equal(row.source, "cas");
      assert.equal(row.purchase_price, "0");
      assert.equal(row.sale_price, "0");
    }
  }
  assert.equal(
    new Set([...fa.outfits, ...ma.outfits].map((x) => x.id)).size,
    6,
  );
  await assert.rejects(
    ownCall("ownedAssets", [om.token, first.avatar_id]),
    /avatar unavailable/,
  );
  const beforeFault = await ownCall("snapshot");
  await ownCall("failReceipt", [true]);
  await assert.rejects(
    ownCall("createAvatar", [
      of.token,
      "ownership_late_fail_01",
      { ...female, name: "Rollback Traveller" },
    ]),
    /test receipt failure/,
  );
  const afterFault = await ownCall("snapshot");
  for (const table of [
    "avatars",
    "operations",
    "sequences",
    "outfits",
    "balances",
    "initializations",
  ])
    assert.deepEqual(afterFault[table], beforeFault[table], table);
  await ownCall("failReceipt", [false]);
  await assert.rejects(
    ownCall("createAvatar", [
      of.token,
      createArgs[1],
      { ...female, name: "Changed Traveller" },
    ]),
    /operation identity reused/,
  );
  await ownCall("testBalance", [first.avatar_id, "123"]);
  await mf.dispose();
  await start();
  assert.deepEqual(await ownCall("createAvatar", createArgs), first);
  assert.deepEqual(await ownCall("ownedAssets", [of.token, first.avatar_id]), {
    ...fa,
    balance: "123",
  });
  // Simulate persisted pre-upgrade appearance-only state. Reads/retries must not mint ownership.
  await ownCall("legacyOwnership", [second.avatar_id]);
  await assert.rejects(
    ownCall("ownedAssets", [om.token, second.avatar_id]),
    /migration required/,
  );
  assert.deepEqual(
    await ownCall("createAvatar", [om.token, "ownership_create_male_01", male]),
    second,
  );
  await assert.rejects(
    ownCall("ownedAssets", [om.token, second.avatar_id]),
    /migration required/,
  );
  checks.push(
    "real SQLite creation atomically grants three source CAS outfits and zero balance, rolls back late receipt failure, preserves changed balance on retry/restart, and refuses uninitialized legacy ownership",
  );
  const unicodeOrigin = "e".repeat(64);
  for (const suffix of ["\ud800", "\udfff", "\ud800x", "x\udfff"]) {
    await assert.rejects(
      call("register", ["unicode_reject", password + suffix, unicodeOrigin]),
      /invalid password Unicode/,
    );
  }
  const unicodePassword = password + "💡é\ufffd";
  await call("register", ["unicode_valid", unicodePassword, unicodeOrigin]);
  await call("login", ["unicode_valid", unicodePassword, unicodeOrigin]);
  await assert.rejects(
    call("login", ["unicode_valid", password + "💡é\ud800", unicodeOrigin]),
    /invalid password Unicode/,
  );
  checks.push(
    "actual account RPC rejects lone surrogates while supplementary Unicode and literal replacement characters remain valid",
  );
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
