import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { createHash } from "node:crypto";
import { stageRecovery } from "./recovery.mjs";
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
test("same pending recovery grant can be staged after ingress loss without reusing stale bytes", async () => {
  const root = await mkdtemp(join(tmpdir(), "owned-recovery-stage-"));
  // Byte envelopes only. These are never advertised as valid native checkpoints.
  const bundle = {
    grant: {
      new_binding: { lot_incarnation: "2" },
      old_binding: { lot_incarnation: "1" },
      old_cursor: { authority_epoch: "1" },
    },
    checkpoint_hex: "0102",
    checkpoint: { sha256: sha(Buffer.from([1, 2])) },
    batches: [{ ticks_hex: "0304" }],
  };
  const first = await stageRecovery(root, bundle, { lot_id: "41" });
  const stale = JSON.parse(await readFile(first, "utf8"));
  await writeFile(stale.recovery.checkpoint_file, Buffer.from([99]));
  const second = await stageRecovery(root, bundle, { lot_id: "41" });
  assert.notEqual(dirname(first), dirname(second));
  const config = JSON.parse(await readFile(second, "utf8"));
  assert.deepEqual(
    await readFile(config.recovery.checkpoint_file),
    Buffer.from([1, 2]),
  );
  assert.deepEqual(config.recovery.takeover, bundle.grant);
  assert.equal(config.lot_id, "41");
  await assert.rejects(
    stageRecovery(root, { ...bundle, checkpoint_hex: "ff" }, { lot_id: "41" }),
  );
});
