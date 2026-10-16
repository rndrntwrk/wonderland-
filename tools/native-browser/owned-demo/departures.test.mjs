import assert from "node:assert/strict";
import { test } from "node:test";
import {
  activeDepartures,
  departureRequest,
  departureForOwnedEntry,
  deliverDepartureReceipts,
  drainOneDeparture,
} from "./departures.mjs";
const identity = {
  authority_id: "a".repeat(64),
  avatar_id: "7",
  control_generation: "2",
  lot_id: "11",
  reservation_digest: "b".repeat(64),
};
const request = departureRequest(identity),
  reference = { ...request };
test("departure outbox saves before release and retires only exact account acknowledgment", async () => {
  const events = [],
    saved = {
      schema_version: 1,
      status: "applied_source_save",
      committed_save: { ...request },
      source_progression_before: "c".repeat(64),
      source_progression_after: "d".repeat(64),
    },
    released = { ...request, status: "released_saved_control" };
  const lot = async (method) => {
    events.push(method);
    if (method === "pendingOwnedDeparturesV1")
      return [
        { lane: "save", reference },
        { lane: "release", reference },
      ];
    assert.equal(method, "retireReleasedAvatarControlV1");
    return released;
  };
  const account = async (method, args) => {
    assert.deepEqual(args, [reference]);
    events.push(method);
    return method === "applyCommittedAvatarLotSaveV1" ? saved : released;
  };
  await deliverDepartureReceipts({
    lot,
    account,
    onReleased: (i) => {
      assert.deepEqual(i, identity);
      events.push("forgotten");
    },
  });
  assert.deepEqual(events, [
    "pendingOwnedDeparturesV1",
    "applyCommittedAvatarLotSaveV1",
    "releaseSavedAvatarControlV1",
    "retireReleasedAvatarControlV1",
    "forgotten",
  ]);
});
test("save acknowledgment must bind its nested committed receipt", async () => {
  for (const wrong of ["identity", "operation", "old_flat_shape"]) {
    const saved = {
      schema_version: 1,
      status: "applied_source_save",
      committed_save: {
        ...request,
        ...(wrong === "identity"
          ? { identity: { ...identity, avatar_id: "8" } }
          : {}),
        ...(wrong === "operation" ? { departure_operation: "other" } : {}),
      },
      source_progression_before: "c".repeat(64),
      source_progression_after: "d".repeat(64),
    };
    if (wrong === "old_flat_shape") {
      delete saved.committed_save;
      Object.assign(saved, request);
    }
    const calls = [];
    await assert.rejects(
      deliverDepartureReceipts({
        lot: async () => [
          { lane: "save", reference },
          { lane: "release", reference },
        ],
        account: async (method) => {
          calls.push(method);
          return saved;
        },
        onReleased: () => assert.fail(),
      }),
    );
    assert.deepEqual(calls, ["applyCommittedAvatarLotSaveV1"]);
  }
});
test("ambiguous save delivery cannot attempt release or forget custody", async () => {
  const calls = [];
  await assert.rejects(
    deliverDepartureReceipts({
      lot: async () => [
        { lane: "save", reference },
        { lane: "release", reference },
      ],
      account: async (m) => {
        calls.push(m);
        throw Error("reply lost");
      },
      onReleased: () => assert.fail(),
    }),
    /reply lost/,
  );
  assert.deepEqual(calls, ["applyCommittedAvatarLotSaveV1"]);
});
test("wrong release owner and mismatched retirement cannot report completion", async () => {
  for (const wrong of ["owner", "retire"]) {
    const released = { ...request, status: "released_saved_control" };
    await assert.rejects(
      deliverDepartureReceipts({
        lot: async (m) =>
          m === "pendingOwnedDeparturesV1"
            ? [{ lane: "release", reference }]
            : { ...released, status: "unverified" },
        account: async () =>
          wrong === "owner"
            ? { ...released, identity: { ...identity, avatar_id: "8" } }
            : released,
        onReleased: () => assert.fail(),
      }),
    );
  }
});
test("drain resumes observed departure before newly fenced user and uses dedicated commit lane", async () => {
  const observed = { ...request, observation: { status: "blocked" } },
    newer = {
      ...departureRequest({ ...identity, avatar_id: "8" }),
      observation: null,
    };
  let prepared;
  assert.equal(
    await drainOneDeparture({
      lot: async () => ({ departures: [newer, observed], next: null }),
      prepare: async (k, v) => {
        assert.equal(k, "prepare_owned_drain");
        prepared = v;
        return { kind: "proposal" };
      },
      commit: async (p, m) => {
        assert.equal(m, "commitOwnedDrainV1");
        return { kind: "committed" };
      },
    }),
    true,
  );
  assert.deepEqual(prepared.departure, request);
  assert.equal(prepared.fence.generation, "2");
});
test("active work enumeration rejects stuck cursor and preserves multiple pages", async () => {
  let calls = 0;
  assert.equal(
    (
      await activeDepartures(async (m, [after]) => {
        assert.equal(m, "activeOwnedDeparturesV1");
        calls++;
        return after === ""
          ? { departures: [request], next: "key" }
          : { departures: [request], next: null };
      })
    ).length,
    2,
  );
  assert.equal(calls, 2);
  await assert.rejects(
    activeDepartures(async () => ({ departures: [], next: "" })),
  );
});

test("fresh departure uses exact committed registry instead of host resolve reply", () => {
  const actor = {
    principal: "9",
    account_id: "12",
    avatar_id: "7",
    control_generation: "2",
    session_ref: "c".repeat(64),
    admission_operation: "original_owned_admission",
    reservation_digest: "b".repeat(64),
    export_digest: "d".repeat(64),
    actor: { object_id: 3, generation: 1 },
  };
  const entry = {
    control: { principal: actor.principal },
    export: actor,
    operation: actor.admission_operation,
    receipt: { kind: "committed" },
  };
  const expected = { ...actor, authority_id: "a".repeat(64), lot_id: "11" };
  assert.deepEqual(
    departureForOwnedEntry(
      { actors: [actor] },
      entry,
      expected.authority_id,
      expected.lot_id,
    ),
    departureRequest(expected),
  );
  for (const field of [
    "principal",
    "account_id",
    "avatar_id",
    "control_generation",
    "session_ref",
    "admission_operation",
    "reservation_digest",
    "export_digest",
  ])
    assert.throws(() =>
      departureForOwnedEntry(
        { actors: [{ ...actor, [field]: "mismatch" }] },
        entry,
        expected.authority_id,
        expected.lot_id,
      ),
    );
  assert.throws(() =>
    departureForOwnedEntry(
      { actors: [actor, actor] },
      entry,
      expected.authority_id,
      expected.lot_id,
    ),
  );
});
