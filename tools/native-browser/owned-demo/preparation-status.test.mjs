import test from "node:test";
import assert from "node:assert/strict";
import { currentPreparationStatus } from "./preparation-status.mjs";

const validStatus = () => ({
  kind: "committed_status", recovery_pending: false,
  binding: {protocol: 1, source_pin: "paired fixture", binding_sha256: "a".repeat(64), lot_incarnation: "1"},
  cursor: {lot_id: "41", authority_epoch: "7", completed_tick: "17", state_hash: "b".repeat(64)},
  retained_receipts: 2, retained_receipt_bytes: 321,
});

test("preparation requests explicit compact status and retains its metadata", async () => {
  const value = validStatus();
  let calls = 0;
  const host = {async ask(request) {
    calls++;
    assert.deepEqual(request, {kind: "inspect_preparation_status"});
    return value;
  }};
  assert.equal(await currentPreparationStatus(host), value);
  assert.equal(calls, 1);
});

test("missing or invalid preparation metadata cannot bypass checkpoint thresholds", async () => {
  const value = validStatus();
  const invalid = [
    {kind: "committed_status", recovery_pending: false},
    {...value, binding: null},
    {...value, binding: {...value.binding, protocol: 2}},
    {...value, binding: {...value.binding, source_pin: ""}},
    {...value, binding: {...value.binding, binding_sha256: "invalid"}},
    {...value, binding: {...value.binding, lot_incarnation: "0"}},
    {...value, cursor: []},
    {...value, cursor: {...value.cursor, lot_id: "0"}},
    {...value, cursor: {...value.cursor, authority_epoch: "01"}},
    {...value, cursor: {...value.cursor, completed_tick: "18446744073709551616"}},
    {...value, cursor: {...value.cursor, state_hash: "invalid"}},
  ];
  for (const field of ["retained_receipts", "retained_receipt_bytes"])
    for (const count of [undefined, -1, 0.5, "2", NaN, Infinity, Number.MAX_SAFE_INTEGER + 1])
      invalid.push({...value, [field]: count});
  for (const response of invalid) {
    let calls = 0;
    await assert.rejects(currentPreparationStatus({async ask() {calls++; return response;}}), /preparation status/);
    assert.equal(calls, 1);
  }
  const initial = {...value, cursor: {...value.cursor, completed_tick: "0"}, retained_receipts: 0, retained_receipt_bytes: 0};
  assert.equal(await currentPreparationStatus({async ask() {return initial;}}), initial);
});

test("preparation fails closed on recovery, unsupported hosts and malformed status", async () => {
  for (const response of [
    {kind: "recovery_status", recovery_pending: true},
    {kind: "committed_status", recovery_pending: true},
    {kind: "committed_status"},
    {kind: "committed_state", recovery_pending: false, checkpoint_hex: "unused"},
    {kind: "denied", reason: "unknown variant inspect_preparation_status"},
    null,
  ]) {
    let calls = 0;
    await assert.rejects(currentPreparationStatus({async ask(request) {
      calls++;
      assert.deepEqual(request, {kind: "inspect_preparation_status"});
      return response;
    }}), /preparation status/);
    assert.equal(calls, 1, "do not retry or silently fall back to full inspection");
  }
});

test("a failed status transport is propagated without another request", async () => {
  const failure = Error("owned host pipe closed");
  let calls = 0;
  await assert.rejects(currentPreparationStatus({async ask() {
    calls++;
    throw failure;
  }}), error => error === failure);
  assert.equal(calls, 1);
});

test("minimal preparation status keeps exact receipt thresholds without inspection payload", async () => {
  for (const [retained_receipts, retained_receipt_bytes] of [[31, 0], [32, 0], [0, 8 * 1024 * 1024 - 1], [0, 8 * 1024 * 1024]]) {
    const value = {...validStatus(), retained_receipts, retained_receipt_bytes};
    const result = await currentPreparationStatus({async ask(request) {
      assert.deepEqual(request, {kind: "inspect_preparation_status"});
      return value;
    }});
    assert.equal(result.retained_receipts, retained_receipts);
    assert.equal(result.retained_receipt_bytes, retained_receipt_bytes);
    assert.deepEqual(Object.keys(result).sort(), [
      "binding", "cursor", "kind", "recovery_pending", "retained_receipt_bytes", "retained_receipts",
    ]);
  }
});
