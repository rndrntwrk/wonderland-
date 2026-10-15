import { build } from "esbuild";
import { readFile, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";
import assert from "node:assert/strict";
const root = await mkdtemp(join(tmpdir(), "wonderland-control-vectors-"));
const bundle = await build({
  entryPoints: ["src/lot/control-state.ts"],
  bundle: true,
  platform: "neutral",
  format: "esm",
  write: false,
});
const entry = join(root, "control.mjs");
await writeFile(entry, bundle.outputFiles[0].text);
const { controlStateV2 } = await import("file://" + entry);
const vectors = JSON.parse(
  await readFile("../../fixtures/native/control-state-v2.json", "utf8"),
);
const canonical = (v) =>
  "{" +
  Object.entries(v)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([k, v]) => JSON.stringify(k) + ":" + JSON.stringify(v))
    .join(",") +
  "}";
const results = vectors.map((v) => {
  let value = null;
  try {
    value = controlStateV2(v.value);
  } catch {}
  assert.equal(value !== null, v.valid, v.name);
  return {
    name: v.name,
    accepted: value !== null,
    canonical: value === null ? null : canonical(value),
  };
});
const rust = JSON.parse(
  execFileSync(
    resolve("../../target/debug/examples/control-state-vectors"),
    [],
    { encoding: "utf8" },
  ),
);
assert.deepEqual(results, rust);
console.log(
  JSON.stringify({
    passed: true,
    vectors: results.length,
    scope:
      "Strict V2 control-state data contract and exact Rust/TypeScript JSON agreement; no actor admission or authentication.",
  }),
);
