import assert from "node:assert/strict";
import { test } from "node:test";
import { registerHooks } from "node:module";
registerHooks({
  resolve(s, c, next) {
    if (
      c.parentURL?.endsWith(".ts") &&
      /^\.\.?\//.test(s) &&
      !/\.[cm]?[jt]s$/.test(s)
    )
      s += ".ts";
    return next(s, c);
  },
});
const { configuredDepartureReceiptReader } = await import(
  "../src/account/departure-proof.ts"
);
const { sourceLotSaveColumns, saveReference } = await import(
  "../src/account/lot-save.ts"
);
const { digest, encode } = await import("../src/lot/contracts.ts");
const h = (n) => n.toString(16).padStart(64, "0");
async function fixture() {
  const identity = {
    authority_id: h(1),
    account_id: "1",
    avatar_id: "2",
    lot_id: "3",
    control_generation: "4",
    session_ref: h(2),
    admission_operation: "admission-proof-1",
    reservation_digest: h(3),
    export_digest: h(4),
    principal: "5",
    actor: { object_id: 6, generation: 1 },
  };
  const save_record = {
    schema_version: 1,
    source_pin: "4c6b3e8f5835b228723caea3c9f683c62f244f73",
    identity,
    departure_operation: "departure-proof-1",
    save_sequence: "1",
    captured_tick: "10",
    capture_reason: "first_leave_tick",
    avatar_columns: sourceLotSaveColumns({
      motive_data: Array(16).fill(0),
      person_data: Array(27).fill(0),
      default_suits: { daywear: "1", sleepwear: "2", swimwear: "3" },
      body_for_persistence: "1",
    }),
    current_job_row: null,
    changed_relationships: [],
  };
  const origin = {
    binding: {
      protocol: 1,
      source_pin: save_record.source_pin,
      binding_sha256: h(5),
      lot_incarnation: "1",
    },
    content_release: h(6),
    protocol_release: h(7),
    batch_id: "8",
    proposal_digest: h(8),
    predecessor: {
      lot_id: "3",
      authority_epoch: "1",
      completed_tick: "9",
      state_hash: h(9),
    },
    result: {
      lot_id: "3",
      authority_epoch: "1",
      completed_tick: "10",
      state_hash: h(10),
    },
    checkpoint_sha256: h(11),
    captured_tick: "10",
    event_ordinal: 0,
  };
  const receipt = {
    schema_version: 1,
    identity,
    departure_operation: save_record.departure_operation,
    save_sha256: await digest(encode(save_record)),
    origin,
    status: "committed_source_save",
    save_record,
    source_baseline: { progression: h(12), wardrobe: h(13) },
  };
  const ref = saveReference(receipt),
    calls = [];
  const env = {
    COORD: {
      idFromName(name) {
        calls.push(["id", name]);
        return { name };
      },
      get(id) {
        calls.push(["get", id.name]);
        return {
          async lookupCommittedAvatarSaveV1(r) {
            calls.push(["lookup", r]);
            return structuredClone(receipt);
          },
        };
      },
    },
  };
  return { receipt, ref, calls, env };
}
test("save proof routes only through configured coordinator for canonical lot ID", async () => {
  const f = await fixture();
  assert.deepEqual(
    await configuredDepartureReceiptReader(f.env).verifyCommittedSave(f.ref),
    f.receipt,
  );
  assert.deepEqual(f.calls.slice(0, 2), [
    ["id", "3"],
    ["get", "3"],
  ]);
});
test("missing configured coordinator cannot authorize departure", async () => {
  const f = await fixture();
  await assert.rejects(
    configuredDepartureReceiptReader({}).verifyCommittedSave(f.ref),
    /unavailable/,
  );
});
test("caller URL or namespace injection is refused before lookup", async () => {
  const f = await fixture();
  await assert.rejects(
    configuredDepartureReceiptReader(f.env).verifyCommittedSave({
      ...f.ref,
      url: "https://example.invalid",
    }),
    /fields/,
  );
  assert.equal(f.calls.length, 0);
});
test("authoritative lookup must return exact requested identity and anchored bytes", async () => {
  for (const mutate of [
    (r) => (r.identity.account_id = "99"),
    (r) => (r.save_record.avatar_columns.skill_body = 12),
    (r) => (r.status = "rejected"),
  ]) {
    const f = await fixture();
    const bad = structuredClone(f.receipt);
    mutate(bad);
    f.env.COORD.get = () => ({ lookupCommittedAvatarSaveV1: async () => bad });
    await assert.rejects(
      configuredDepartureReceiptReader(f.env).verifyCommittedSave(f.ref),
    );
  }
});
test("coordinator lookup failures do not fall back to caller proof", async () => {
  const f = await fixture();
  f.env.COORD.get = () => ({
    lookupCommittedAvatarSaveV1: async () => {
      throw Error("uncommitted");
    },
  });
  await assert.rejects(
    configuredDepartureReceiptReader(f.env).verifyCommittedSave(f.ref),
    /uncommitted/,
  );
});

test("detach lookup binds full committed receipt rather than a caller assertion", async () => {
  const f = await fixture();
  const receipt = {
    schema_version: 1,
    status: "committed_actor_detach",
    identity: f.ref.identity,
    departure_operation: f.ref.departure_operation,
    source_save: f.ref,
    detached_tick: "10",
    removal_reason: "source_leave",
    origin: f.ref.origin,
    control_revision_before: "1",
    control_revision_after: "2",
    actor_registry_before_sha256: h(20),
    actor_registry_after_sha256: h(21),
  };
  const reference = {
    schema_version: 1,
    identity: receipt.identity,
    departure_operation: receipt.departure_operation,
    detach_sha256: await digest(encode(receipt)),
    origin: receipt.origin,
  };
  f.env.COORD.get = () => ({
    lookupCommittedAvatarDetachV1: async () => structuredClone(receipt),
  });
  assert.deepEqual(
    await configuredDepartureReceiptReader(f.env).verifyCommittedDetach(
      reference,
    ),
    receipt,
  );
  for (const mutate of [
    (r) => (r.removal_reason = "forced_cleanup"),
    (r) => (r.actor_registry_after_sha256 = h(99)),
    (r) => (r.status = "pending"),
  ]) {
    const bad = structuredClone(receipt);
    mutate(bad);
    f.env.COORD.get = () => ({
      lookupCommittedAvatarDetachV1: async () => bad,
    });
    await assert.rejects(
      configuredDepartureReceiptReader(f.env).verifyCommittedDetach(reference),
    );
  }
});
