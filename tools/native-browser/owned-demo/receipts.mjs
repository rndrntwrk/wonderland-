// Private supervisor custody transfer. A checkpoint alone cannot prove that an
// action receipt, rejected decision, or original retry input was retained.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";

const canonical = (value) =>
  value === null || typeof value !== "object"
    ? JSON.stringify(value)
    : Array.isArray(value)
      ? "[" + value.map(canonical).join(",") + "]"
      : "{" +
        Object.entries(value)
          .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
          .map(([key, value]) => JSON.stringify(key) + ":" + canonical(value))
          .join(",") +
        "}";
const sha = (value) =>
  createHash("sha256").update(canonical(value)).digest("hex");

export async function askWithDurableReceipts(host, lot, request) {
  const reply = await host.ask(request);
  if (reply.kind !== "retired") return reply;
  assert.equal(reply.batch_id, request.batch_id);
  if (request.binding) assert.deepEqual(reply.binding, request.binding);
  const found = await lot("lookupHostReceiptV1", [
    reply.binding.lot_incarnation,
    request.batch_id,
  ]);
  if (found.status !== "recorded")
    throw Error(
      "Retired native receipt is unavailable; refusing to execute again.",
    );
  const record = found.record;
  assert.equal(record.batch_id, request.batch_id);
  assert.equal(record.reply.batch_id, record.batch_id);
  assert.equal(record.reply.digest, record.digest);
  assert.ok(["committed", "rejected"].includes(record.reply.kind));
  if (request.kind.startsWith("prepare")) {
    assert.equal(typeof reply.input_sha256, "string");
    assert.equal(
      record.input_sha256,
      reply.input_sha256,
      "Retired batch input differs.",
    );
  } else if (request.kind === "resolve") {
    assert.equal(
      record.digest,
      request.digest,
      "Retired batch digest differs.",
    );
    assert.equal(record.digest, reply.digest);
  } else assert.equal(request.kind, "lookup");
  return record.reply;
}

export async function retireCheckpointReceipts(host, lot, manifest) {
  for (;;) {
    const exported = await host.ask({ kind: "export_receipts" });
    assert.equal(exported.kind, "receipt_export");
    const envelope = exported.payload;
    assert.equal(envelope.schema_version, 1);
    assert.deepEqual(envelope.binding, manifest.binding);
    assert.deepEqual(envelope.cursor, manifest.cursor);
    assert.equal(envelope.checkpoint_sha256, manifest.sha256);
    assert.equal(sha(envelope), exported.digest);
    assert.ok(Array.isArray(envelope.records));
    if (!envelope.records.length) return;
    const expected = {
      schema_version: 1,
      status: "persisted",
      binding: envelope.binding,
      cursor: envelope.cursor,
      checkpoint_sha256: envelope.checkpoint_sha256,
      through_batch_id: envelope.through_batch_id,
      receipt_set_sha256: exported.digest,
    };
    const acknowledgement = await lot("persistHostReceiptsV1", [
      envelope,
      exported.digest,
    ]);
    assert.deepEqual(acknowledgement, expected);
    const retired = await host.ask({
      kind: "acknowledge_receipts",
      acknowledgement,
    });
    assert.equal(retired.kind, "receipts_retired");
    assert.deepEqual(retired.acknowledgement, acknowledgement);
  }
}
