// Dedicated process group lets the harness kill both supervisor and workerd.
import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
const [root, barrier] = process.argv.slice(2);
const bundled = await build({
  entryPoints: ["tests/account-crash-driver.ts"],
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
const mf = new Miniflare({
  modulesRoot: root,
  modules: [
    {
      type: "ESModule",
      path: join(root, "worker.mjs"),
      contents: bundled.outputFiles[0].text,
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
  bindings: { CRASH_BARRIER: barrier },
  durableObjects: { ACCOUNTS: { className: "TestAccounts", useSQLite: true } },
  durableObjectsPersist: join(root, "do"),
});
process.send({ url: String(await mf.ready) });
process.on("message", async (message) => {
  if (message === "close") {
    await mf.dispose();
    process.exit(0);
  }
});
