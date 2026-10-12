import { build } from "esbuild";
import { copyFile, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
const root = fileURLToPath(new URL("../", import.meta.url));
await mkdir(root + "dist", { recursive: true });
await build({
  entryPoints: [root + "src/index.ts"],
  bundle: true,
  format: "esm",
  platform: "neutral",
  external: ["cloudflare:workers"],
  outfile: root + "dist/worker.mjs",
  plugins: [
    {
      name: "static-rust-wasm",
      setup(b) {
        b.onResolve({ filter: /account_crypto_bg\.wasm$/ }, () => ({
          path: "./account_crypto_bg.wasm",
          external: true,
        }));
      },
    },
  ],
});
await copyFile(
  root + "generated/account_crypto_bg.wasm",
  root + "dist/account_crypto_bg.wasm",
);
