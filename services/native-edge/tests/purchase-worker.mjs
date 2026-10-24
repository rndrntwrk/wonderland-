// Dedicated test-owned process group; no process inspection is used.
import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
const [root, barrier] = process.argv.slice(2);
const b = await build({
  entryPoints: ["tests/purchase-driver.ts"],
  bundle: true,
  format: "esm",
  platform: "neutral",
  external: ["cloudflare:workers"],
  write: false,
  plugins: [
    {
      name: "wasm",
      setup(x) {
        x.onResolve({ filter: /account_crypto_bg\.wasm$/ }, () => ({
          path: "./account_crypto_bg.wasm",
          external: true,
        }));
      },
    },
  ],
});
const mf = new Miniflare({
  modulesRoot: root,
  modules: [
    {
      type: "ESModule",
      path: join(root, "worker.mjs"),
      contents: b.outputFiles[0].text,
    },
    {
      type: "CompiledWasm",
      path: join(root, "account_crypto_bg.wasm"),
      contents: await readFile("generated/account_crypto_bg.wasm"),
    },
  ],
  compatibilityDate: "2026-07-01",
  host: "127.0.0.1",
  port: 0,
  bindings: {
    CRASH_BARRIER: barrier,
    NATIVE_ACCOUNT_HTTP: "staged-v1",
    NATIVE_ACCOUNT_ORIGIN: "https://test",
    NATIVE_ACCOUNT_REALM: "purchase",
  },
  durableObjects: {
    ACCOUNTS: { className: "PurchaseAccounts", useSQLite: true },
    COORD: { className: "PurchaseLot", useSQLite: true },
  },
  durableObjectsPersist: join(root, "do"),
  r2Buckets: ["CHECKPOINTS"],
  r2Persist: join(root, "r2"),
});
process.send({ url: String(await mf.ready) });
process.on("message", async (x) => {
  if (x === "close") {
    await mf.dispose();
    process.exit(0);
  }
});
