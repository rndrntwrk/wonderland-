import assert from "node:assert/strict";
import { test } from "node:test";
import { ownedPlacement, ownedReadiness } from "./placement.mjs";
const actor = {
  principal: "1",
  avatar_id: "7",
  control_generation: "3",
  session_ref: "a".repeat(64),
  actor: { object_id: 1, generation: 2 },
};
const entry = {
  control: actor,
  binding: {
    source_epoch: "3",
    lot_incarnation: "4",
    avatar_id: 7,
    lot_location: 11,
  },
};
const state = (inside) => ({
  actors: [actor],
  actor_placements: [
    {
      principal: "1",
      avatar_id: "7",
      control_generation: "3",
      actor: actor.actor,
      position: inside
        ? { x: 24, y: 24, level: 1 }
        : { x: -32768, y: -32768, level: 1 },
      in_world: inside,
    },
  ],
});
test("committed OOW custody has explicit placement blocker and never issues a ticket", () => {
  const current = state(false);
  const before = JSON.stringify(current);
  const value = ownedReadiness(current, entry, () => assert.fail());
  assert.equal(value.state, "admitted_awaiting_placement");
  assert.equal(value.ticket, undefined);
  assert.equal(value.blocker, "source_placement_unavailable");
  assert.equal(JSON.stringify(current), before);
});
test("only exact committed placement issues an action-ready ticket", () => {
  const value = ownedReadiness(state(true), entry, (e) => {
    assert.equal(e, entry);
    return "ticket";
  });
  assert.equal(value.state, "lot_ready");
  assert.equal(value.ticket, "ticket");
  for (const kind of [
    "missing",
    "duplicate",
    "generation",
    "actor",
    "principal",
  ]) {
    const s = state(true);
    if (kind === "missing") delete s.actor_placements;
    else if (kind === "duplicate")
      s.actor_placements.push(s.actor_placements[0]);
    else if (kind === "generation")
      s.actor_placements[0].control_generation = "4";
    else if (kind === "actor")
      s.actor_placements[0].actor = { object_id: 1, generation: 3 };
    else s.actor_placements[0].principal = "2";
    assert.throws(() => ownedPlacement(s, entry));
  }
});
