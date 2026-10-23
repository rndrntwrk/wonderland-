import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { readFile, mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import assert from "node:assert/strict";
import { createNativeAccountProvider } from "../../../apps/web-shell/public/native-account.mjs";
import { createContainerEdgeTransport } from "../../../tools/native-browser/owned-demo/edge-transport.mjs";
import { createSupervisorOutboundHandler } from "../src/hosted/supervisor-outbound.ts";
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
  let heldRoster,
    releaseRoster,
    holdRoster = false,
    heldToken,
    heldResponse;
  const responseReady = new Promise((resolve) => (heldResponse = resolve));
  const responseGate = new Promise((resolve) => (releaseRoster = resolve));
  const interrupted = createNativeAccountProvider(
    "/native-account",
    (value) => (heldRoster = value),
    {
      origin: "https://test",
      fetch: async (url, opts) => {
        const response = await browserFetch(url, opts);
        if (holdRoster && url.endsWith("/roster")) {
          // Hold a fully received service response, so releasing it models a
          // transport that delivers stale data despite the cancellation signal.
          const bytes = await response.arrayBuffer();
          heldToken = opts.headers.Authorization;
          heldResponse();
          await responseGate;
          return new Response(bytes, {
            status: response.status,
            headers: response.headers,
          });
        }
        return response;
      },
    },
  );
  await interrupted.login("http_owner_a", "native-account-test-passphrase");
  assert.equal(heldRoster.state, "account_ready");
  const ownedRoster = heldRoster.avatars;
  holdRoster = true;
  const refreshing = interrupted.refresh();
  assert.equal(heldRoster.state, "account_refreshing");
  assert.deepEqual(heldRoster.avatars, ownedRoster);
  await responseReady;
  await interrupted.logout();
  assert.equal(heldRoster.state, "signed_out");
  assert.equal(
    (
      await request(
        "/native-account/roster",
        { after: "0" },
        { authorization: heldToken },
      )
    ).status,
    401,
  );
  releaseRoster();
  await refreshing;
  assert.equal(heldRoster.state, "signed_out");
  assert.deepEqual(heldRoster.avatars, []);
  interrupted.dispose();
  checks.push(
    "pending refresh preserves owned roster; sign-out revokes real SQLite session and late roster cannot restore it",
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
  // Exercise the public creation boundary against the real SQLite authority.
  // Removing authentication, operation identity or catalog validation must fail.
  // Independent users avoid consuming the earlier interruption cases' five
  // allowed authentication attempts; production throttles stay unchanged.
  for (const [username, name] of [
    ["http_creator_a", "Creator Alpha"],
    ["http_creator_b", "Creator Beta"],
  ])
    assert.equal(
      (await request("/test-setup", { username, name })).status,
      200,
    );
  const spec = {
    name: "Third Traveller",
    description: "Created through authenticated HTTP",
    gender: "female",
    skin: "dark",
    head_key: "1",
    body_key: "2",
    release: "b".repeat(64),
  };
  const createBody = { operation: "native_http_create_01", avatar: spec };
  assert.equal(
    (await request("/native-account/create", createBody)).status,
    401,
  );
  const signIn = async (username) => {
    const response = await request("/native-account/login", {
      username,
      password: "native-account-test-passphrase",
    });
    assert.equal(response.status, 200);
    return response.json();
  };
  const sa = await signIn("http_creator_a"),
    sb = await signIn("http_creator_b");
  const ha = { authorization: "Bearer " + sa.token },
    hb = { authorization: "Bearer " + sb.token };
  assert.equal(
    (
      await request("/native-account/create", createBody, {
        ...ha,
        origin: "https://other.invalid",
      })
    ).status,
    403,
  );
  assert.equal(
    (
      await mf.dispatchFetch("https://test/native-account/create", {
        method: "GET",
        headers: { origin: "https://test", ...ha },
      })
    ).status,
    405,
  );
  const creationBridge = createContainerEdgeTransport({
    publicOrigin: "https://test",
    fetchOutbound: createSupervisorOutboundHandler({
      publicOrigin: "https://test",
      dispatchRpc: async () => {
        throw Error("Creation must not use supervisor RPC");
      },
      // Miniflare's dispatch API takes URL/init, not Node's Request instance.
      accountHttp: async (request) =>
        mf.dispatchFetch(request.url, {
          method: request.method,
          headers: Object.fromEntries(request.headers),
          body: await request.arrayBuffer(),
        }),
    }),
  });
  const createdResponse = await creationBridge.dispatchFetch(
    "https://test/native-account/create",
    options(createBody, ha),
  );
  assert.equal(createdResponse.status, 200);
  assert.equal(createdResponse.headers.get("cache-control"), "no-store");
  const created = await createdResponse.json();
  assert.equal(created.version, 1);
  assert.equal(created.state, "avatar_created");
  assert.deepEqual(created.avatar, { avatar_id: "5", ...spec });
  assert.deepEqual(
    await (await request("/native-account/create", createBody, ha)).json(),
    created,
  );
  assert.deepEqual(
    (
      await (await request("/native-account/roster", { after: "0" }, ha)).json()
    ).avatars.map((v) => v.name),
    ["Creator Alpha", "Third Traveller"],
  );
  checks.push(
    "authenticated creation retains exact original appearance and idempotent operation across a repeated/lost reply; no extra avatar allocated",
  );
  const changed = await request(
    "/native-account/create",
    { ...createBody, avatar: { ...spec, name: "Changed Traveller" } },
    ha,
  );
  assert.equal(changed.status, 503);
  assert.deepEqual(await changed.json(), { error: "unavailable" });
  const other = await request(
    "/native-account/create",
    { ...createBody, avatar: { ...spec, name: "Other Traveller" } },
    hb,
  );
  assert.equal(other.status, 200);
  assert.equal((await other.json()).avatar.avatar_id, "6");
  assert.deepEqual(
    (
      await (await request("/native-account/roster", { after: "0" }, hb)).json()
    ).avatars.map((v) => v.name),
    ["Creator Beta", "Other Traveller"],
  );
  checks.push(
    "operation scope and roster remain account-owned; changed bytes under one identity reject without allocation",
  );
  for (const data of [
    { ...createBody, account_id: sb.account_id },
    { ...createBody, operation: "short" },
    { ...createBody, avatar: { ...spec, principal: "2" } },
    { ...createBody, avatar: { ...spec, head_key: 1 } },
  ])
    assert.equal(
      (await request("/native-account/create", data, ha)).status,
      400,
    );
  for (const change of [
    { head_key: "999" },
    { release: "c".repeat(64) },
    { gender: "male" },
    { skin: "light" },
  ]) {
    const rejected = await request(
      "/native-account/create",
      {
        operation: "native_http_invalid_01",
        avatar: { ...spec, ...change, name: "Denied Traveller" },
      },
      ha,
    );
    assert.equal(rejected.status, 503);
    assert.deepEqual(await rejected.json(), { error: "unavailable" });
  }
  const last = await request(
    "/native-account/create",
    {
      operation: "native_http_create_03",
      avatar: { ...spec, name: "Last Traveller" },
    },
    ha,
  );
  assert.equal(last.status, 200);
  assert.equal(
    (
      await request(
        "/native-account/create",
        {
          operation: "native_http_create_04",
          avatar: { ...spec, name: "Over Limit Traveller" },
        },
        ha,
      )
    ).status,
    503,
  );
  assert.equal(
    (await (await request("/native-account/roster", { after: "0" }, ha)).json())
      .avatars.length,
    3,
  );
  await request("/native-account/logout", { all: false }, ha);
  assert.equal(
    (await request("/native-account/create", createBody, ha)).status,
    401,
  );
  await creationBridge.dispose();
  checks.push(
    "creation rejects foreign origin/method, malformed authority fields, unpinned appearance/release, revoked session and a fourth Sim; no signup route exposed",
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
