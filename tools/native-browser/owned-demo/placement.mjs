// Trusted committed host projection, never browser-provided coordinates.
import assert from "node:assert/strict";
export function ownedPlacement(current, entry) {
  const actors = current.actors.filter(
    (a) =>
      a.principal === entry.control.principal &&
      a.avatar_id === entry.control.avatar_id &&
      a.control_generation === entry.control.control_generation &&
      a.session_ref === entry.control.session_ref,
  );
  assert.equal(
    actors.length,
    1,
    "Owned placement requires one exact committed actor",
  );
  const actor = actors[0];
  const rows = current.actor_placements?.filter(
    (p) =>
      p.principal === actor.principal &&
      p.avatar_id === actor.avatar_id &&
      p.control_generation === actor.control_generation &&
      p.actor.object_id === actor.actor.object_id &&
      p.actor.generation === actor.actor.generation,
  );
  assert.equal(
    rows?.length,
    1,
    "Owned placement evidence is missing or ambiguous",
  );
  const value = rows[0];
  assert.equal(typeof value.in_world, "boolean");
  for (const k of ["x", "y", "level"])
    assert.ok(Number.isInteger(value.position[k]));
  return value;
}
export function ownedReadiness(current, entry, issueTicket) {
  const placement = ownedPlacement(current, entry);
  return placement.in_world
    ? {
        version: 1,
        state: "lot_ready",
        binding: entry.binding,
        ticket: issueTicket(entry),
      }
    : {
        version: 1,
        state: "admitted_awaiting_placement",
        binding: entry.binding,
        blocker: "source_placement_unavailable",
      };
}
