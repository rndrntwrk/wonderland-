// Read-only reconciliation. This helper has no prepare, commit, host, or renewal
// capability. A missing record must remain Unknown, never become a rejection.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
const decimal = (value) =>
  typeof value === "string" &&
  /^(0|[1-9][0-9]{0,19})$/.test(value) &&
  BigInt(value) <= 18446744073709551615n;
const hex = (value, limit, magic) =>
  typeof value === "string" &&
  value.length <= limit * 2 &&
  /^(?:[0-9a-f]{2})+$/.test(value) &&
  value.startsWith(magic);
const hash = (value) =>
  typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
export async function recoverPlayerAction(
  query,
  entry,
  current,
  { lookup, receipt },
) {
  assert.ok(query && typeof query === "object" && !Array.isArray(query));
  assert.deepEqual(Object.keys(query).sort(), [
    "action_hex",
    "lot_incarnation",
    "type",
  ]);
  assert.equal(query.type, "native_action_result");
  assert.ok(decimal(query.lot_incarnation) && query.lot_incarnation !== "0");
  assert.ok(
    BigInt(query.lot_incarnation) <= BigInt(current.binding.lot_incarnation),
  );
  assert.ok(hex(query.action_hex, 16384, "574c43310d0a1a0a"));
  // None of these authority fields comes from the browser's query.
  const control = { ...entry.control, account_id: entry.export.account_id };
  const found = await lookup({
    schema_version: 1,
    control,
    lot_incarnation: query.lot_incarnation,
    request_hex: query.action_hex,
  });
  assert.equal(found.schema_version, 1);
  if (found.status === "unknown") return;
  assert.ok(["committed", "rejected"].includes(found.status));
  assert.deepEqual(found.control, control);
  assert.equal(found.request_hex, query.action_hex);
  assert.equal(
    found.request_sha256,
    createHash("sha256")
      .update(Buffer.from(query.action_hex, "hex"))
      .digest("hex"),
  );
  assert.deepEqual(found.binding, {
    ...current.binding,
    lot_incarnation: query.lot_incarnation,
  });
  assert.equal(found.cursor.lot_id, current.cursor.lot_id);
  assert.ok(decimal(found.cursor.completed_tick));
  assert.ok(
    BigInt(found.cursor.completed_tick) <=
      BigInt(current.cursor.completed_tick),
  );
  assert.ok(
    decimal(found.batch_id) && found.batch_id !== "0" && hash(found.digest),
  );
  assert.ok(hex(found.receipt_hex, 16384 + 64, "574c41310d0a1a0a"));
  // Rust still validates exact embedded request, current actor and accepted tick
  // against the newly installed checkpoint before changing the visible status.
  receipt(found.receipt_hex);
}

// This is the authenticated socket route, shared with its boundary tests.
// Ordinary security checks may persist expiry/fence obligations. They never
// renew custody or activity. The decision lookup itself does not execute input.
export async function reconcileAuthenticatedPlayerAction(
  query,
  entry,
  { sweepFences, revalidate, lookup, receipt },
) {
  await sweepFences();
  const current = await revalidate(entry);
  await recoverPlayerAction(query, entry, current, { lookup, receipt });
}
