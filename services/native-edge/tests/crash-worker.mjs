// Dedicated process group lets the harness kill both supervisor and workerd.
import { Miniflare } from "miniflare";
import { build } from "esbuild";
import { join } from "node:path";
const [root, barrier] = process.argv.slice(2);
const bundled = await build({
  entryPoints: ["tests/crash-driver.ts"],
  bundle: true,
  format: "esm",
  platform: "neutral",
  external: ["cloudflare:workers"],
  write: false,
});
const mf = new Miniflare({
  modules: true,
  script: bundled.outputFiles[0].text,
  compatibilityDate: "2026-07-01",
  host: "127.0.0.1",
  port: 0,
  bindings: { CRASH_BARRIER: barrier },
  durableObjects: { COORD: { className: "TestCoordinator", useSQLite: true } },
  durableObjectsPersist: join(root, "do"),
  r2Buckets: ["CHECKPOINTS"],
  r2Persist: join(root, "r2"),
});
process.send({ url: String(await mf.ready) });
process.on("message", async (message) => {
  if (message === "close") {
    await mf.dispose();
    process.exit(0);
  }
});
