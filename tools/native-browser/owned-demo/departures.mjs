// Private durable departure orchestration. Browser input never becomes save data.
import assert from "node:assert/strict";
export const departureRequest = (identity) => ({
  schema_version: 1,
  identity,
  departure_operation: `${identity.authority_id}_${identity.avatar_id}_${identity.control_generation}`,
});
// Build from the freshly validated committed registry, never a host resolve
// acknowledgment (which is not an admission receipt and has no actor field).
export function departureForOwnedEntry(current, entry, authority, lot) {
  const actors = current.actors.filter(
    (actor) =>
      actor.principal === entry.control.principal &&
      actor.account_id === entry.export.account_id &&
      actor.avatar_id === entry.export.avatar_id &&
      actor.control_generation === entry.export.control_generation &&
      actor.session_ref === entry.export.session_ref &&
      actor.admission_operation === entry.operation &&
      actor.reservation_digest === entry.export.reservation_digest &&
      actor.export_digest === entry.export.export_digest,
  );
  assert.equal(
    actors.length,
    1,
    "Departure requires one exact committed actor",
  );
  return departureRequest({
    ...actors[0],
    authority_id: authority,
    lot_id: lot,
  });
}
export const departureFence = (identity) => ({
  version: 1,
  authority: identity.authority_id,
  avatar_id: identity.avatar_id,
  generation: identity.control_generation,
  lot_id: identity.lot_id,
  request_digest: identity.reservation_digest,
});
export async function activeDepartures(lot) {
  const values = [];
  let after = "";
  for (let page = 0; page < 512; page++) {
    const result = await lot("activeOwnedDeparturesV1", [after]);
    assert.ok(
      Array.isArray(result.departures) && result.departures.length <= 128,
    );
    values.push(...result.departures);
    if (result.next === null) return values;
    assert.ok(typeof result.next === "string" && result.next > after);
    after = result.next;
  }
  throw Error("Departure enumeration exceeded durable capacity.");
}
export async function deliverDepartureReceipts({ lot, account, onReleased }) {
  const rows = await lot("pendingOwnedDeparturesV1");
  assert.ok(Array.isArray(rows) && rows.length <= 256);
  for (const row of rows) {
    if (row.lane === "save") {
      const saved = await account("applyCommittedAvatarLotSaveV1", [
        row.reference,
      ]);
      assert.equal(saved.status, "applied_source_save");
      assert.deepEqual(saved.committed_save.identity, row.reference.identity);
      assert.equal(
        saved.committed_save.departure_operation,
        row.reference.departure_operation,
      );
    } else {
      assert.equal(row.lane, "release");
      const released = await account("releaseSavedAvatarControlV1", [
        row.reference,
      ]);
      assert.equal(released.status, "released_saved_control");
      assert.deepEqual(released.identity, row.reference.identity);
      assert.equal(
        released.departure_operation,
        row.reference.departure_operation,
      );
      const retired = await lot("retireReleasedAvatarControlV1", [
        row.reference,
      ]);
      assert.deepEqual(retired, released);
      await onReleased?.(released.identity);
    }
  }
}
export async function drainOneDeparture({ lot, prepare, commit }) {
  const work = await activeDepartures(lot);
  // Only one native departure starts at a time. A durable observation survives
  // ingress restart and takes precedence over newly queued control fences.
  const selected = work.find((x) => x.observation !== null) ?? work[0];
  if (!selected) return false;
  const request = departureRequest(selected.identity);
  assert.equal(request.departure_operation, selected.departure_operation);
  const proposal = await prepare("prepare_owned_drain", {
    departure: request,
    fence: departureFence(selected.identity),
  });
  const result = await commit(proposal, "commitOwnedDrainV1");
  if (result.kind !== "committed")
    throw Error("Native departure drain was refused.");
  return true;
}
