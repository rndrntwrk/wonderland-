import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { createHash } from "node:crypto";
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
function decode(hex) {
  assert.equal(typeof hex, "string");
  assert.ok(
    hex.length > 0 &&
      hex.length <= 16 * 1024 * 1024 &&
      /^(?:[0-9a-f]{2})+$/.test(hex),
  );
  return Buffer.from(hex, "hex");
}
export async function stageRecovery(root, bundle, baseConfig) {
  assert.match(bundle.grant.new_binding.lot_incarnation, /^[1-9][0-9]{0,19}$/);
  assert.ok(Array.isArray(bundle.batches) && bundle.batches.length <= 64);
  const checkpointBytes = decode(bundle.checkpoint_hex);
  assert.equal(sha(checkpointBytes), bundle.checkpoint.sha256);
  // Each attempt owns a new directory, including a retry of the same durable
  // pending grant after process loss. Never overwrite or trust leftover files.
  const directory = await mkdtemp(
    join(root, "recovery-" + bundle.grant.new_binding.lot_incarnation + "-"),
  );
  const checkpoint = join(directory, "checkpoint.bin");
  await writeFile(checkpoint, checkpointBytes, { flag: "wx" });
  const tails = [];
  let total = 0;
  for (const [index, b] of bundle.batches.entries()) {
    const bytes = decode(b.ticks_hex);
    total += bytes.length;
    assert.ok(total <= 8 * 1024 * 1024);
    const file = join(directory, `tail-${index}.bin`);
    await writeFile(file, bytes, { flag: "wx" });
    tails.push({ file, sha256: sha(bytes) });
  }
  const config = {
    ...baseConfig,
    lot_incarnation: bundle.grant.old_binding.lot_incarnation,
    authority_epoch: bundle.grant.old_cursor.authority_epoch,
    recovery: {
      checkpoint_file: checkpoint,
      checkpoint_sha256: bundle.checkpoint.sha256,
      additional_tail_files: tails,
      expected_cursor: bundle.grant.old_cursor,
      takeover: bundle.grant,
    },
  };
  const configPath = join(directory, "host.json");
  await writeFile(configPath, JSON.stringify(config), { flag: "wx" });
  return configPath;
}
