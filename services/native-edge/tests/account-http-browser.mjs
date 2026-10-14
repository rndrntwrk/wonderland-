// Rendered native-account entry against real local AccountAuthority. Test setup is private to this runner.
import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { createServer } from "node:http";
import { readFile, mkdtemp, writeFile, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, extname } from "node:path";
import assert from "node:assert/strict";
const { chromium } = await import(
  process.env.PLAYWRIGHT_MODULE || "playwright"
);
const root = await mkdtemp(join(tmpdir(), "wonderland-native-browser-"));
const dist = resolve("../../apps/web-shell/dist");
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
let mf, browser;
const errors = [];
const observed = [];
const server = createServer(async (req, res) => {
  try {
    const origin = `http://127.0.0.1:${server.address().port}`;
    const url = new URL(req.url, origin);
    if (url.pathname.startsWith("/native-account/")) {
      const chunks = [];
      for await (const c of req) chunks.push(c);
      const headers = { ...req.headers, "cf-connecting-ip": "127.0.0.1" };
      delete headers.host;
      delete headers["content-length"];
      const r = await mf.dispatchFetch(url.href, {
        method: req.method,
        headers,
        body: Buffer.concat(chunks),
      });
      res.writeHead(r.status, Object.fromEntries(r.headers));
      res.end(Buffer.from(await r.arrayBuffer()));
      return;
    }
    if (url.pathname === "/wonderland-config.json") {
      res.setHeader("Content-Type", "application/json");
      res.end(
        JSON.stringify({
          version: 1,
          mode: "native_account",
          gateway_url: "/native-account",
          native_lots: false,
        }),
      );
      return;
    }
    const rel =
      decodeURIComponent(url.pathname).replace(/^\//, "") || "index.html";
    if (rel.includes("..")) throw Error("path");
    const bytes = await readFile(join(dist, rel));
    res.setHeader(
      "Content-Type",
      {
        ".html": "text/html",
        ".js": "text/javascript",
        ".mjs": "text/javascript",
        ".wasm": "application/wasm",
        ".css": "text/css",
        ".svg": "image/svg+xml",
        ".png": "image/png",
        ".jpg": "image/jpeg",
        ".woff2": "font/woff2",
      }[extname(rel)] || "application/octet-stream",
    );
    res.end(bytes);
  } catch (error) {
    res.writeHead(404);
    res.end("Not found");
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
try {
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
        contents: await readFile("generated/account_crypto_bg.wasm"),
      },
    ],
    compatibilityDate: "2026-07-01",
    durableObjects: {
      ACCOUNTS: { className: "AccountAuthority", useSQLite: true },
    },
    bindings: {
      NATIVE_ACCOUNT_HTTP: "staged-v1",
      NATIVE_ACCOUNT_ORIGIN: origin,
      NATIVE_ACCOUNT_REALM: "rendered-test",
    },
  });
  await mf.ready;
  for (const [username, name] of [
    ["browser_owner_a", "First Traveller"],
    ["browser_owner_b", "Second Traveller"],
  ]) {
    const r = await mf.dispatchFetch(origin + "/test-setup", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ username, name }),
    });
    assert.equal(r.status, 200);
  }
  browser = await chromium.launch({
    headless: true,
    executablePath: process.env.CHROMIUM_EXECUTABLE,
    args: ["--no-sandbox"],
  });
  for (const [width, height, username, name, other] of [
    [1280, 900, "browser_owner_a", "First Traveller", "Second Traveller"],
    [390, 844, "browser_owner_b", "Second Traveller", "First Traveller"],
  ]) {
    const context = await browser.newContext({ viewport: { width, height } });
    const page = await context.newPage();
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("request", (r) => observed.push(new URL(r.url()).pathname));
    await page.goto(origin);
    await page.getByLabel("Account name", { exact: true }).fill(username);
    await page
      .getByLabel("Password", { exact: true })
      .fill("native-account-test-passphrase");
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page
      .getByRole("heading", { name: "Your Sims", exact: true })
      .waitFor();
    assert.equal(await page.getByText(name, { exact: true }).count(), 1);
    assert.equal(await page.getByText(other, { exact: true }).count(), 0);
    assert.equal(
      await page.getByRole("button", { name: "Play", exact: true }).count(),
      0,
    );
    let blocked = false;
    await page.route("**/native-account/roster", async (route) => {
      if (!blocked) {
        blocked = true;
        await route.abort();
      } else await route.continue();
    });
    await page
      .getByRole("button", { name: "Refresh roster", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Refresh roster", exact: true })
      .waitFor();
    await page
      .getByRole("button", { name: "Refresh roster", exact: true })
      .click();
    await page.getByText(name, { exact: true }).waitFor();
    await page.getByRole("button", { name: "Sign out", exact: true }).click();
    await page.getByRole("button", { name: "Sign in", exact: true }).waitFor();
    assert.equal(await page.getByText(name, { exact: true }).count(), 0);
    assert.equal(
      await page.getByLabel("Password", { exact: true }).inputValue(),
      "",
    );
    await context.close();
  }
  assert.deepEqual(errors, []);
  assert.ok(
    !observed.some((p) => p.includes("gateway") || p.includes("socket")),
  );
  const evidence = {
    status: "passed",
    viewports: ["1280x900", "390x844"],
    checks: [
      "compiled Rust/WASM native account mode loads packaged module",
      "two isolated accounts display own persisted avatars only",
      "transient roster failure recovers without repeated login",
      "logout clears account presentation and password",
      "no Play or fabricated admission, no legacy gateway/socket request",
      "no page errors",
    ],
    scope:
      "Trunk development build, headless Chromium, real local workerd SQLite and Rust Argon2id. No deployment, original assets, lot admission or playable-game acceptance.",
  };
  await mkdir("../../tests/output/native-account-http", { recursive: true });
  await writeFile(
    "../../tests/output/native-account-http/browser.json",
    JSON.stringify(evidence, null, 2),
  );
  console.log(JSON.stringify(evidence, null, 2));
} finally {
  await browser?.close();
  await mf?.dispose();
  await new Promise((resolve) => server.close(resolve));
}
