import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { readFile, mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import assert from "node:assert/strict";
import { createNativeAccountProvider } from "../../../apps/web-shell/public/native-account.mjs";
const root = await mkdtemp(join(tmpdir(), "wonderland-account-http-"));
const result = await build({
  entryPoints: ["tests/account-http-driver.ts"],
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
const mf = new Miniflare({
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
    ACCOUNTS: { className: "AccountAuthority", useSQLite: true },
  },
  bindings: {
    NATIVE_ACCOUNT_HTTP: "staged-v1",
    NATIVE_ACCOUNT_ORIGIN: "https://test",
    NATIVE_ACCOUNT_REALM: "test-realm",
  },
});
const options = (data, extra = {}) => ({
  method: "POST",
  headers: {
    "content-type": "application/json",
    origin: "https://test",
    "cf-connecting-ip": "127.0.0.1",
    ...extra,
  },
  body: JSON.stringify(data),
});
const request = (path, data, headers) =>
  mf.dispatchFetch("https://test" + path, options(data, headers));
const checks = [];
try {
  await mf.ready;
  for (const [username, name] of [
    ["http_owner_a", "First Traveller"],
    ["http_owner_b", "Second Traveller"],
  ])
    assert.equal(
      (await request("/test-setup", { username, name })).status,
      200,
    );
  for (const path of [
    "/native-account/register",
    "/native-account/createAvatar",
    "/native-account/reserveAvatarControl",
    "/rpc/commitBatch",
  ])
    assert.equal((await request(path, {})).status, 404);
  assert.equal(
    (
      await request(
        "/native-account/login",
        {},
        { origin: "https://evil.example" },
      )
    ).status,
    403,
  );
  assert.equal(
    (
      await request(
        "/native-account/login",
        {},
        { "sec-fetch-site": "cross-site" },
      )
    ).status,
    403,
  );
  assert.equal(
    (await request("/native-account/login", { x: "x".repeat(8193) })).status,
    400,
  );
  checks.push(
    "unknown privileged/signup routes, cross-origin, cross-site and oversized body denied",
  );
  const a = await (
    await request("/native-account/login", {
      username: "http_owner_a",
      password: "native-account-test-passphrase",
    })
  ).json();
  assert.equal(a.state, "account_ready");
  assert.equal(a.admission_available, false);
  const r = await request(
    "/native-account/roster",
    { after: "0" },
    { authorization: "Bearer " + a.token },
  );
  assert.equal(r.headers.get("cache-control"), "no-store");
  assert.equal(r.headers.get("access-control-allow-origin"), null);
  assert.deepEqual(
    (await r.json()).avatars.map((v) => v.name),
    ["First Traveller"],
  );
  const rotated = await (
    await request(
      "/native-account/rotate",
      {},
      { authorization: "Bearer " + a.token },
    )
  ).json();
  assert.notEqual(rotated.token, a.token);
  assert.equal(
    (
      await request(
        "/native-account/roster",
        { after: "0" },
        { authorization: "Bearer " + a.token },
      )
    ).status,
    401,
  );
  assert.equal(
    (
      await request(
        "/native-account/logout",
        { all: false },
        { authorization: "Bearer " + rotated.token },
      )
    ).status,
    200,
  );
  assert.equal(
    (
      await request(
        "/native-account/roster",
        { after: "0" },
        { authorization: "Bearer " + rotated.token },
      )
    ).status,
    401,
  );
  checks.push(
    "actual Rust Argon2id and SQLite login, isolated roster, no-store replies, rotation and logout revocation",
  );
  let latest;
  const emitted = [];
  const browserFetch = (url, opts) =>
    mf.dispatchFetch(url, {
      ...opts,
      headers: {
        ...opts.headers,
        origin: "https://test",
        "cf-connecting-ip": "127.0.0.2",
      },
    });
  const provider = createNativeAccountProvider(
    "/native-account",
    (v) => {
      latest = v;
      emitted.push(v);
    },
    { origin: "https://test", fetch: browserFetch },
  );
  await provider.login("http_owner_b", "native-account-test-passphrase");
  assert.equal(latest.state, "account_ready");
  assert.deepEqual(
    latest.avatars.map((v) => v.name),
    ["Second Traveller"],
  );
  assert.ok(!JSON.stringify(emitted).includes("token"));
  await provider.refresh();
  assert.equal(latest.state, "account_ready");
  await provider.logout();
  assert.equal(latest.state, "signed_out");
  provider.dispose();
  checks.push(
    "actual browser provider module login and roster with second real SQLite account; refresh, logout and no token in view state",
  );
  let release;
  let calls = 0;
  const stale = [];
  const pending = createNativeAccountProvider(
    "/native-account",
    (v) => stale.push(v),
    {
      origin: "https://test",
      fetch: async () => {
        calls++;
        return await new Promise((resolve) => (release = resolve));
      },
    },
  );
  const login = pending.login("test", "test");
  await pending.login("test", "test");
  assert.equal(calls, 1);
  pending.dispose();
  release(
    Response.json({
      version: 1,
      state: "account_ready",
      admission_available: false,
      token: "f".repeat(64),
    }),
  );
  await login;
  assert.ok(!stale.some((v) => v.state === "account_ready"));
  checks.push(
    "duplicate login suppressed and disposed late response cannot restore account",
  );
  assert.throws(
    () =>
      createNativeAccountProvider("/native-account", () => {}, {
        origin: "http://public.example",
      }),
    /Invalid/,
  );
  let retained,
    tokenForRevocation,
    rosterFails = true;
  const retryProvider = createNativeAccountProvider(
    "/native-account",
    (v) => {
      retained = v;
    },
    {
      origin: "https://test",
      fetch: async (url, opts) => {
        if (url.endsWith("/roster") && rosterFails) {
          tokenForRevocation = opts.headers.Authorization;
          throw Error("temporary network failure");
        }
        return browserFetch(url, opts);
      },
    },
  );
  await retryProvider.login("http_owner_a", "native-account-test-passphrase");
  assert.equal(retained.state, "account_error");
  const liveBefore = await request(
    "/native-account/roster",
    { after: "0" },
    { authorization: tokenForRevocation },
  );
  assert.equal(liveBefore.status, 200);
  await retryProvider.logout();
  assert.equal(retained.state, "signed_out");
  assert.equal(
    (
      await request(
        "/native-account/roster",
        { after: "0" },
        { authorization: tokenForRevocation },
      )
    ).status,
    401,
  );
  retryProvider.dispose();
  checks.push(
    "cleartext public origin rejected before network; roster failure retains token for confirmed server logout",
  );
  const adapter = await build({
    entryPoints: ["src/http/accounts.ts"],
    bundle: true,
    format: "esm",
    platform: "neutral",
    write: false,
  });
  const handlerPath = join(root, "adapter.mjs");
  await writeFile(handlerPath, adapter.outputFiles[0].text);
  const { accountHttp } = await import("file://" + handlerPath);
  let authorityCalls = 0;
  const boundaryEnv = {
    NATIVE_ACCOUNT_HTTP: "staged-v1",
    NATIVE_ACCOUNT_ORIGIN: "https://test",
    NATIVE_ACCOUNT_REALM: "test",
    ACCOUNTS: {
      idFromName: () => "",
      get: () => ({
        logout: () => {
          authorityCalls++;
          return { signed_out: true };
        },
      }),
    },
  };
  const unclosed = new ReadableStream({
    start(controller) {
      controller.enqueue(new TextEncoder().encode('{"all":false}'));
    },
  });
  const timeoutResponse = await accountHttp(
    new Request("https://test/native-account/logout", {
      method: "POST",
      headers: {
        origin: "https://test",
        "content-type": "application/json",
        authorization: "Bearer " + "a".repeat(64),
      },
      body: unclosed,
      duplex: "half",
    }),
    boundaryEnv,
  );
  assert.equal(timeoutResponse.status, 400);
  assert.equal(authorityCalls, 0);
  assert.equal(
    (
      await accountHttp(
        new Request("https://test/native-account/login", options({})),
        {},
      )
    ).status,
    404,
  );
  checks.push(
    "negative-only handler boundary: unclosed valid JSON body times out without authority invocation; unconfigured surface returns404",
  );
  const upstreamDown = {
    ...boundaryEnv,
    ACCOUNTS: {
      idFromName: () => "",
      get: () => ({
        roster: () => {
          throw Error("private upstream diagnostic");
        },
      }),
    },
  };
  const outage = await accountHttp(
    new Request(
      "https://test/native-account/roster",
      options({ after: "0" }, { authorization: "Bearer " + "a".repeat(64) }),
    ),
    upstreamDown,
  );
  assert.equal(outage.status, 503);
  assert.deepEqual(await outage.json(), { error: "unavailable" });
  checks.push(
    "negative-only boundary: transient authority failure returns503 without private diagnostic or false revocation",
  );
  const dir = resolve("../../tests/output/native-account-http");
  await mkdir(dir, { recursive: true });
  const evidence = {
    status: "passed",
    checks,
    scope:
      "Real local workerd, SQLite and compiled Rust password WASM; browser transport module executed against real service. No Rust/WASM UI rendering or gameplay admission claimed by this suite.",
  };
  await writeFile(join(dir, "journey.json"), JSON.stringify(evidence, null, 2));
  console.log(JSON.stringify(evidence, null, 2));
} finally {
  await mf.dispose();
}
