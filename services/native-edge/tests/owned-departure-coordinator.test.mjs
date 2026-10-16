// Real-SQLite coordinator protocol/fault tests with synthetic private-host
// observations. These do not execute native source leave or prove playable leave.
import assert from "node:assert/strict";
import { test } from "node:test";
import { DatabaseSync } from "node:sqlite";
import { registerHooks } from "node:module";
import { createHash } from "node:crypto";
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "cloudflare:workers")
      return {
        shortCircuit: true,
        url:
          "data:text/javascript," +
          encodeURIComponent(
            "export class DurableObject { constructor(ctx, env) { this.ctx = ctx; this.env = env; } }",
          ),
      };
    if (
      context.parentURL?.endsWith(".ts") &&
      /^\.\.?\//.test(specifier) &&
      !/\.[cm]?[jt]s$/.test(specifier)
    )
      specifier += ".ts";
    return nextResolve(specifier, context);
  },
});
const { LotCoordinator } = await import("../src/lot/coordinator.ts");
const { canonical, encode } = await import("../src/lot/contracts.ts");
const { departureControlKey, departureFence, departureOperationV1 } =
  await import("../src/lot/departures.ts");
const { sourceLotSaveColumns, saveReference } = await import(
  "../src/account/lot-save.ts"
);
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const h = (n) => n.toString(16).padStart(64, "0");
const clone = (v) => structuredClone(v);
const binding = {
  protocol: 1,
  source_pin: "4c6b3e8f5835b228723caea3c9f683c62f244f73",
  binding_sha256: h(1),
  lot_incarnation: "1",
};
const permission = { revision: "1", sha256: h(2) };
const config = {
  authority_id: h(3),
  content_release: h(4),
  protocol_release: h(5),
  permission_binding: permission,
  actors: [],
};
const actors = [1, 2].map((n) => ({
  principal: String(n),
  account_id: String(n + 8),
  avatar_id: String(n + 10),
  actor: { object_id: n + 1, generation: 3 },
  control_generation: "2",
  session_ref: h(n + 5),
  admission_operation: `owned_admission_${n}`,
  reservation_digest: h(n + 8),
  export_digest: h(n + 10),
}));
const identity = {
  authority_id: config.authority_id,
  lot_id: "7",
  ...actors[0],
};
const request = {
  schema_version: 1,
  identity,
  departure_operation: departureOperationV1(identity),
};
function storage() {
  const db = new DatabaseSync(":memory:");
  let depth = 0;
  const s = {
    db,
    fault: null,
    sql: {
      exec(query, ...bindings) {
        s.fault?.(query, bindings);
        let rows = [];
        if (!bindings.length && query.includes(";")) db.exec(query);
        else
          rows = db
            .prepare(query)
            .all(...bindings)
            .map((r) => ({ ...r }));
        s.afterExec?.(query, bindings);
        return {
          toArray: () => rows,
          one: () => {
            assert.equal(rows.length, 1);
            return rows[0];
          },
        };
      },
    },
    transactionSync(fn) {
      const name = `s${depth++}`;
      db.exec(`SAVEPOINT ${name}`);
      try {
        const r = fn();
        db.exec(`RELEASE ${name}`);
        return r;
      } catch (e) {
        db.exec(`ROLLBACK TO ${name}; RELEASE ${name}`);
        throw e;
      } finally {
        depth--;
      }
    },
  };
  return s;
}
class Bucket {
  objects = new Map();
  async get(key) {
    const b = this.objects.get(key);
    return b
      ? { size: b.length, arrayBuffer: async () => Uint8Array.from(b).buffer }
      : null;
  }
  async put(key, b) {
    this.objects.set(key, Uint8Array.from(b));
  }
}
function fixture({ begin = true } = {}) {
  const s = storage(),
    bucket = new Bucket(),
    env = { CHECKPOINTS: bucket };
  const lot = new LotCoordinator({ storage: s }, env);
  lot.now = () => 1000;
  lot.initializeControlledV2(
    binding,
    {
      lot_id: "7",
      authority_epoch: "1",
      completed_tick: "0",
      state_hash: h(20),
    },
    h(21),
    config,
  );
  s.sql.exec("UPDATE lot_control_state SET revision='2'");
  s.sql.exec(
    "UPDATE controlled_registry SET sha256=?,value=?",
    sha(encode(actors)),
    canonical(actors),
  );
  for (const a of actors) {
    const e = {
      schema_version: 2,
      authority_id: config.authority_id,
      account_id: a.account_id,
      avatar_id: a.avatar_id,
      lot_id: "7",
      control_generation: a.control_generation,
      admission_operation: a.admission_operation,
      reservation_digest: a.reservation_digest,
      session_ref: a.session_ref,
      account_revision: "1",
      content_release: config.content_release,
      protocol_release: config.protocol_release,
      expires_at: 2000,
      permission_binding: permission,
      permission_source: {
        registered_at_ms: 0,
        created_at_ms: 0,
        observed_at_ms: 0,
        roommate_lots: [{ lot_id: "7", permissions_level: 1, is_pending: 0 }],
        moderation_level: 0,
        mayor_nhood: null,
      },
      source_revisions: {
        owned_assets: 1,
        wardrobe: h(30),
        ledger: h(31),
        progression: h(32),
        permissions: h(33),
      },
      portable_json: "{}",
      portable_sha256: sha("{}"),
      export_digest: a.export_digest,
    };
    const key = departureControlKey({
      authority_id: config.authority_id,
      lot_id: "7",
      ...a,
    });
    s.sql.exec(
      "INSERT INTO actor_controls(control_key,avatar_id,principal,export_digest,export_json,state,expires_at,record) VALUES(?,?,?,?,?,'active',?,?)",
      key,
      a.avatar_id,
      a.principal,
      a.export_digest,
      canonical(e),
      2000,
      canonical(a),
    );
    s.sql.exec(
      "INSERT INTO controlled_leases(control_key,revision) VALUES(?,'1')",
      key,
    );
  }
  if (begin) lot.beginOwnedDepartureV1(request);
  return {
    s,
    lot,
    env,
    bucket,
    reopen() {
      const next = new LotCoordinator({ storage: s }, env);
      next.now = () => 1000;
      return next;
    },
  };
}
function resign(p) {
  p.digest = sha(encode(p.payload));
  return p;
}
function proposal(
  f,
  { save = true, detach = true, blocked = false, id, departure = true } = {},
) {
  const state = JSON.parse(
    f.s.sql.exec("SELECT value FROM lot_state WHERE id=1").one().value,
  );
  const registry = f.s.sql
    .exec("SELECT sha256,value FROM controlled_registry WHERE id=1")
    .one();
  const revision = f.s.sql
    .exec("SELECT revision FROM lot_control_state WHERE id=1")
    .one().revision;
  const before = JSON.parse(registry.value),
    after = detach
      ? before.filter((a) => a.principal !== identity.principal)
      : before;
  const tick = (BigInt(state.cursor.completed_tick) + 1n).toString();
  const payload = {
    binding: state.binding,
    batch_id: id ?? tick,
    predecessor: state.cursor,
    result: {
      ...state.cursor,
      completed_tick: tick,
      state_hash: h(Number(tick) + 100),
    },
    ticks_hex: "00",
    checkpoint_sha256: sha(Buffer.from(`checkpoint_${tick}`)),
    effects_hex: "0000000000000000",
    events_hex: "0000000000000000",
    control_v2: {
      schema_version: 2,
      revision_before: revision,
      revision_after: detach ? (BigInt(revision) + 1n).toString() : revision,
      actor_registry_before_sha256: registry.sha256,
      actor_registry_after_sha256: sha(encode(after)),
      actors: after,
      permission_before: permission,
      permission_after: permission,
    },
  };
  if (departure) {
    const save_record = {
      schema_version: 1,
      source_pin: binding.source_pin,
      identity: clone(identity),
      departure_operation: request.departure_operation,
      save_sequence: "1",
      captured_tick: tick,
      capture_reason: "first_leave_tick",
      avatar_columns: sourceLotSaveColumns({
        motive_data: Array(16).fill(-100),
        person_data: Array(27).fill(0),
        default_suits: { daywear: "123", sleepwear: "124", swimwear: "125" },
        body_for_persistence: "126",
      }),
      current_job_row: null,
      changed_relationships: [{ target: "4294967295", values: [-100, 200] }],
    };
    payload.departure_v1 = {
      ...clone(request),
      lifecycle: {
        observed_tick: tick,
        status: detach ? "detached" : blocked ? "blocked" : "leaving",
        blocker: blocked ? "source_check_rejected" : null,
      },
      save: save
        ? {
            save_record,
            save_sha256: sha(encode(save_record)),
            event_ordinal: 0,
          }
        : null,
      detach: detach
        ? {
            detached_tick: tick,
            removal_reason: "source_leave",
            event_ordinal: 1,
          }
        : null,
      entity_refs_after: after.map((a) => a.actor),
    };
  }
  return resign({ kind: "proposal", status: "pending", payload });
}
const rows = (s, table) => s.sql.exec(`SELECT * FROM ${table}`).toArray();
function references(f) {
  return Object.fromEntries(
    f.lot.pendingOwnedDeparturesV1().map((r) => [r.lane, r.reference]),
  );
}
function noCommittedProof(f) {
  for (const t of [
    "batches",
    "departure_saves",
    "departure_detaches",
    "departure_outbox",
  ])
    assert.equal(rows(f.s, t).length, 0, t);
}
async function takeover(f) {
  const state = JSON.parse(
    f.s.sql.exec("SELECT value FROM lot_state WHERE id=1").one().value,
  );
  const started = f.lot.beginControlledRecoveryV2(
    state.binding.lot_incarnation,
  );
  const grant = started.grant,
    bytes = Buffer.from(`takeover_${grant.new_binding.lot_incarnation}`);
  const marker = {
    schema_version: 1,
    grant,
    new_cursor: {
      ...state.cursor,
      authority_epoch: grant.new_authority_epoch,
      state_hash: h(500 + Number(grant.new_binding.lot_incarnation)),
    },
    checkpoint_sha256: sha(bytes),
  };
  const result = await f.lot.finishControlledRecoveryV2(
    { schema_version: 2, marker, control: started.control },
    bytes,
  );
  assert.deepEqual(result.marker, marker);
  return marker;
}

test("begin is exact, durable and fences without fabricating save/detach proof", () => {
  const f = fixture({ begin: false }),
    r = f.lot.beginOwnedDepartureV1(request);
  assert.equal(r.status, "leaving");
  assert.deepEqual(f.reopen().beginOwnedDepartureV1(request), r);
  assert.equal(rows(f.s, "control_fences").length, 1);
  assert.equal(rows(f.s, "actor_controls")[0].state, "fence_pending");
  assert.throws(
    () =>
      f.lot.beginOwnedDepartureV1({
        ...request,
        departure_operation: "different_departure",
      }),
    /operation|reused/,
  );
  noCommittedProof(f);
});
test("same-tick source save and detach have immutable independently anchored receipts/outbox", async () => {
  const f = fixture(),
    p = proposal(f);
  const r = await f.lot.commitOwnedDrainV1(p);
  assert.equal(r.status, "committed");
  const refs = references(f),
    save = await f.lot.lookupCommittedAvatarSaveV1(refs.save),
    detach = await f.lot.lookupCommittedAvatarDetachV1(refs.release);
  assert.deepEqual(save.save_record, p.payload.departure_v1.save.save_record);
  assert.deepEqual(save.source_baseline, {
    progression: h(32),
    wardrobe: h(30),
  });
  assert.deepEqual(detach.source_save, refs.save);
  assert.equal(sha(encode(detach)), refs.release.detach_sha256);
  assert.equal(rows(f.s, "actor_controls").length, 2);
  assert.deepEqual(
    rows(f.s, "controlled_registry").map((r) => JSON.parse(r.value))[0],
    [actors[1]],
  );
  assert.deepEqual(await f.reopen().commitOwnedDrainV1(p), r);
  assert.equal(rows(f.s, "departure_outbox").length, 2);
});
test("ordinary lane cannot ingest departure evidence; ordinary removal and fence guards remain", async () => {
  const f = fixture();
  await assert.rejects(
    f.lot.commitControlledBatchV2(proposal(f)),
    /trusted drain/,
  );
  const normal = proposal(f, { departure: false, detach: false });
  assert.equal(
    (await f.lot.commitControlledBatchV2(normal)).reason,
    "fence_drain_required",
  );
  const g = fixture({ begin: false });
  await assert.rejects(
    g.lot.commitControlledBatchV2(proposal(g, { departure: false })),
    /ordinary controlled transition removed/,
  );
  assert.equal(rows(g.s, "batches").length, 0);
});
test("blocked native lifecycle stays blocked with custody held and no invented source proof", async () => {
  const f = fixture();
  await f.lot.commitOwnedDrainV1(
    proposal(f, { save: false, detach: false, blocked: true }),
  );
  assert.equal(f.lot.lookupOwnedDepartureV1(request).status, "blocked");
  assert.equal(rows(f.s, "departure_saves").length, 0);
  assert.equal(rows(f.s, "actor_controls").length, 2);
  await assert.rejects(
    f.lot.commitOwnedDrainV1(proposal(f, { save: false })),
    /source save missing/,
  );
});
test("expiry denial cannot create save proof; next exact fenced generation can drain without live lease", async () => {
  const f = fixture();
  f.s.sql.exec("UPDATE actor_controls SET expires_at=0");
  const p = proposal(f);
  const denied = await f.lot.commitOwnedDrainV1(p);
  assert.equal(denied.reason, "lease_expired");
  noCommittedProof(f);
  assert.deepEqual(await f.lot.commitOwnedDrainV1(p), denied);
  assert.equal(
    (await f.lot.commitOwnedDrainV1(proposal(f, { id: "2" }))).status,
    "committed",
  );
});
for (const [name, mutate] of [
  ["unknown evidence field", (d) => (d.extra = true)],
  ["wrong schema", (d) => (d.schema_version = 2)],
  ["null target", (d) => (d.identity = null)],
  ["save SHA mismatch", (d) => (d.save.save_sha256 = h(999))],
  [
    "wrong save account",
    (d) => (d.save.save_record.identity.account_id = "100"),
  ],
  [
    "unknown save value",
    (d) => (d.save.save_record.avatar_columns.budget = "9"),
  ],
  [
    "target remains in entity table",
    (d) => d.entity_refs_after.unshift(identity.actor),
  ],
  ["peer missing from entity table", (d) => (d.entity_refs_after = [])],
  [
    "duplicate entity slot",
    (d) => d.entity_refs_after.push(d.entity_refs_after[0]),
  ],
  ["unknown cleanup reason", (d) => (d.detach.removal_reason = "delete")],
  ["detach before save event", (d) => (d.detach.event_ordinal = 0)],
  [
    "observation outside batch",
    (d) => {
      d.lifecycle.observed_tick = "2";
      d.detach.detached_tick = "2";
    },
  ],
  [
    "normal success with blocker",
    (d) => (d.lifecycle.blocker = "missing_content"),
  ],
])
  test(`malformed/unrelated ${name} fails closed`, async () => {
    const f = fixture(),
      p = proposal(f);
    mutate(p.payload.departure_v1);
    resign(p);
    await assert.rejects(f.lot.commitOwnedDrainV1(p));
    noCommittedProof(f);
  });
for (const field of [
  "authority_id",
  "account_id",
  "avatar_id",
  "control_generation",
  "session_ref",
  "admission_operation",
  "reservation_digest",
  "export_digest",
  "principal",
  "actor",
])
  test(`exact installed ${field} cannot be substituted`, async () => {
    const f = fixture(),
      p = proposal(f);
    const d = p.payload.departure_v1;
    const value =
      field === "actor"
        ? { object_id: 9, generation: 9 }
        : field.endsWith("id") ||
            field === "control_generation" ||
            field === "principal"
          ? field === "authority_id"
            ? h(998)
            : "998"
          : field === "admission_operation"
            ? "other_admission_01"
            : h(998);
    d.identity[field] = value;
    d.save.save_record.identity[field] = clone(value);
    d.save.save_sha256 = sha(encode(d.save.save_record));
    resign(p);
    await assert.rejects(f.lot.commitOwnedDrainV1(p));
    noCommittedProof(f);
  });
test("drain cannot mutate peer identity, add a control, or mutate membership binding", async () => {
  for (const kind of ["peer", "addition", "permission"]) {
    const f = fixture(),
      p = proposal(f);
    if (kind === "peer") p.payload.control_v2.actors[0].session_ref = h(999);
    if (kind === "addition")
      p.payload.control_v2.actors.push({
        ...clone(actors[0]),
        principal: "3",
        avatar_id: "15",
        actor: { object_id: 4, generation: 3 },
      });
    if (kind === "permission")
      p.payload.control_v2.permission_after = { revision: "2", sha256: h(999) };
    p.payload.control_v2.actor_registry_after_sha256 = sha(
      encode(p.payload.control_v2.actors),
    );
    p.payload.departure_v1.entity_refs_after = p.payload.control_v2.actors
      .map((a) => a.actor)
      .sort((a, b) => a.object_id - b.object_id);
    resign(p);
    await assert.rejects(f.lot.commitOwnedDrainV1(p));
    noCommittedProof(f);
  }
});
test("drain requires an exact fence and stable begun operation", async () => {
  const f = fixture({ begin: false });
  await assert.rejects(f.lot.commitOwnedDrainV1(proposal(f)), /pending/);
  noCommittedProof(f);
  f.lot.beginOwnedDepartureV1(request);
  f.s.sql.exec("UPDATE control_fences SET generation='3'");
  await assert.rejects(f.lot.commitOwnedDrainV1(proposal(f)), /exact fence/);
  noCommittedProof(f);
});
for (const table of [
  "batches",
  "journal_chunks",
  "departure_anchors",
  "departure_detach_lineages",
  "departure_saves",
  "departure_detaches",
  "departure_outbox",
])
  for (const timing of ["before", "after"])
    test(`failure ${timing} ${table} insert rolls back journal, registry, receipts and outbox`, async () => {
      const f = fixture(),
        p = proposal(f);
      const fault = (q) => {
        if (q.startsWith(`INSERT INTO ${table}(`))
          throw Error("injected departure failure");
      };
      f.s[timing === "before" ? "fault" : "afterExec"] = fault;
      await assert.rejects(f.lot.commitOwnedDrainV1(p), /injected/);
      f.s.fault = null;
      f.s.afterExec = null;
      noCommittedProof(f);
      assert.deepEqual(
        JSON.parse(rows(f.s, "controlled_registry")[0].value),
        actors,
      );
      assert.equal(f.lot.lookupOwnedDepartureV1(request).status, "leaving");
      assert.equal((await f.lot.commitOwnedDrainV1(p)).status, "committed");
    });
test("one save tip stays constant through many drains, committed takeovers, detach and real compaction", async () => {
  const f = fixture();
  await f.lot.commitOwnedDrainV1(proposal(f, { detach: false }));
  const saveRef = references(f).save;
  f.lot = f.reopen();
  for (let n = 0; n < 256; n++)
    await f.lot.commitOwnedDrainV1(proposal(f, { save: false, detach: false }));
  assert.equal(rows(f.s, "departure_lineage_tips").length, 1);
  assert.equal(rows(f.s, "departure_anchors").length, 1);
  await takeover(f);
  await takeover(f);
  for (let n = 0; n < 128; n++)
    await f.lot.commitOwnedDrainV1(proposal(f, { save: false, detach: false }));
  const p = proposal(f, { save: false });
  await f.lot.commitOwnedDrainV1(p);
  const refs = references(f),
    d = await f.lot.lookupCommittedAvatarDetachV1(refs.release);
  assert.deepEqual(d.source_save, saveRef);
  assert.equal(d.origin.binding.lot_incarnation, "3");
  assert.equal(d.origin.result.authority_epoch, "3");
  assert.equal(rows(f.s, "departure_lineage_tips").length, 0);
  assert.equal(rows(f.s, "departure_anchors").length, 2);
  assert.equal(rows(f.s, "departure_detach_lineages").length, 1);
  const current = f.lot.inspectControlledV2(),
    control = {
      control_revision: current.control_revision,
      actor_registry_sha256: current.actor_registry_sha256,
      actors: current.actors,
      permission_binding: current.permission_binding,
    };
  await f.lot.publishControlledCheckpointV2(
    current.binding,
    current.cursor,
    current.checkpoint_sha256,
    Buffer.from(`checkpoint_${current.cursor.completed_tick}`),
    control,
  );
  assert.equal(rows(f.s, "journal_live").length, 0);
  assert.equal(rows(f.s, "journal_chunks").length, 0);
  assert.ok(rows(f.s, "archived_batches").length > 384);
  assert.deepEqual(
    await f.reopen().lookupCommittedAvatarDetachV1(refs.release),
    d,
  );
});

test("same lot/epoch numbers without accepted lineage, and corrupted takeover anchors, cannot detach", async () => {
  const f = fixture();
  await f.lot.commitOwnedDrainV1(proposal(f, { detach: false }));
  const state = JSON.parse(rows(f.s, "lot_state")[0].value);
  f.s.sql.exec(
    "UPDATE lot_state SET value=?",
    canonical({ ...state, cursor: { ...state.cursor, state_hash: h(999) } }),
  );
  await assert.rejects(
    f.lot.commitOwnedDrainV1(proposal(f, { save: false })),
    /lineage tip does not match/,
  );
  f.s.sql.exec("UPDATE lot_state SET value=?", canonical(state));
  const marker = await takeover(f);
  f.s.sql.exec(
    "UPDATE recoveries SET marker_json=?",
    canonical({
      ...marker,
      grant: {
        ...marker.grant,
        old_cursor: { ...marker.grant.old_cursor, state_hash: h(999) },
      },
    }),
  );
  await assert.rejects(
    f.lot.commitOwnedDrainV1(proposal(f, { save: false, id: "1" })),
    /takeover anchor/,
  );
  assert.equal(rows(f.s, "departure_detaches").length, 0);
});

test("one-shot save cannot be recaptured under a new committed batch", async () => {
  const f = fixture();
  await f.lot.commitOwnedDrainV1(proposal(f, { detach: false }));
  await assert.rejects(f.lot.commitOwnedDrainV1(proposal(f)), /one-shot/);
  assert.equal(rows(f.s, "batches").length, 1);
});
for (const lane of ["save", "release"])
  test(`lookup ${lane} rejects caller-forged origin/hash and corrupted stored receipt`, async () => {
    const f = fixture();
    await f.lot.commitOwnedDrainV1(proposal(f));
    const ref = references(f)[lane];
    const lookup = (x) =>
      lane === "save"
        ? f.lot.lookupCommittedAvatarSaveV1(x)
        : f.lot.lookupCommittedAvatarDetachV1(x);
    for (const mutate of [
      (r) => (r.origin.proposal_digest = h(900)),
      (r) => (r.identity.account_id = "900"),
      (r) => (r.origin.binding.lot_incarnation = "99"),
      (r) => (r[lane === "save" ? "save_sha256" : "detach_sha256"] = h(901)),
    ]) {
      const altered = clone(ref);
      mutate(altered);
      await assert.rejects(lookup(altered));
    }
    const table = lane === "save" ? "departure_saves" : "departure_detaches",
      row = rows(f.s, table)[0],
      bad = JSON.parse(row.receipt_json);
    bad.origin.event_ordinal += 7;
    f.s.sql.exec(
      `UPDATE ${table} SET receipt_json=?,receipt_sha256=?`,
      canonical(bad),
      sha(encode(bad)),
    );
    const changed = clone(ref);
    changed.origin = bad.origin;
    if (lane === "release") changed.detach_sha256 = sha(encode(bad));
    await assert.rejects(lookup(changed), /accepted batch/);
  });
test("receipt lookup refuses rehashed frozen lineage and missing/unrelated save commit anchor", async () => {
  const f = fixture();
  await f.lot.commitOwnedDrainV1(proposal(f, { detach: false }));
  await f.lot.commitOwnedDrainV1(proposal(f, { save: false, detach: false }));
  await f.lot.commitOwnedDrainV1(proposal(f, { save: false }));
  const refs = references(f),
    row = rows(f.s, "departure_detach_lineages")[0],
    bad = JSON.parse(row.attestation_json);
  bad.predecessor_tip.transition_sequence = "900";
  f.s.sql.exec(
    "UPDATE departure_detach_lineages SET attestation_json=?,attestation_sha256=?",
    canonical(bad),
    sha(encode(bad)),
  );
  await assert.rejects(
    f.lot.lookupCommittedAvatarDetachV1(refs.release),
    /lineage integrity/,
  );
  f.s.sql.exec("UPDATE batches SET digest=? WHERE batch_key='1:1'", h(999));
  await assert.rejects(f.lot.lookupCommittedAvatarSaveV1(refs.save), /anchor/);
});

function released(detach, ref) {
  return {
    schema_version: 1,
    status: "released_saved_control",
    identity: clone(identity),
    departure_operation: request.departure_operation,
    save_sha256: detach.source_save.save_sha256,
    account_save_receipt_sha256: h(700),
    detach_sha256: ref.detach_sha256,
    committed_detach: detach,
  };
}
test("actor retirement requires configured account lookup and preserves history, generations and peer", async () => {
  const f = fixture(),
    p = proposal(f);
  await f.lot.commitOwnedDrainV1(p);
  const refs = references(f),
    detach = await f.lot.lookupCommittedAvatarDetachV1(refs.release),
    release = released(detach, refs.release);
  await assert.rejects(
    f.lot.retireReleasedAvatarControlV1(refs.release),
    /binding unavailable/,
  );
  const seen = [];
  f.env.ACCOUNT_REALM = "configured-realm";
  f.env.ACCOUNTS = {
    idFromName: (n) => {
      seen.push(n);
      return n;
    },
    get: (n) => ({
      lookupReleasedAvatarControlV1: async (q) => {
        assert.equal(n, "configured-realm");
        assert.deepEqual(q, {
          identity,
          departure_operation: request.departure_operation,
        });
        return clone(release);
      },
    }),
  };
  assert.deepEqual(
    await f.lot.retireReleasedAvatarControlV1(refs.release),
    release,
  );
  assert.equal(rows(f.s, "actor_control_history").length, 1);
  assert.equal(rows(f.s, "actor_controls").length, 1);
  assert.equal(rows(f.s, "actor_controls")[0].avatar_id, actors[1].avatar_id);
  assert.equal(rows(f.s, "controlled_leases").length, 1);
  assert.equal(rows(f.s, "control_fences").length, 1);
  assert.equal(rows(f.s, "departure_outbox").length, 0);
  assert.deepEqual(
    await f.reopen().commitOwnedDrainV1(p),
    await f.lot.commitOwnedDrainV1(p),
  );
  assert.deepEqual(
    await f.lot.retireReleasedAvatarControlV1(refs.release),
    release,
  );
  assert.equal(f.lot.beginOwnedDepartureV1(request).status, "released");
  assert.deepEqual(
    await f.lot.lookupCommittedAvatarSaveV1(refs.save),
    await f.reopen().lookupCommittedAvatarSaveV1(refs.save),
  );
  assert.ok(seen.every((n) => n === "configured-realm"));
});
test("forged account release acknowledgment cannot retire control", async () => {
  const f = fixture();
  await f.lot.commitOwnedDrainV1(proposal(f));
  const ref = references(f).release,
    detach = await f.lot.lookupCommittedAvatarDetachV1(ref),
    release = released(detach, ref);
  release.detach_sha256 = h(900);
  f.env.NATIVE_ACCOUNT_REALM = "realm";
  f.env.ACCOUNTS = {
    idFromName: (n) => n,
    get: () => ({ lookupReleasedAvatarControlV1: async () => release }),
  };
  await assert.rejects(
    f.lot.retireReleasedAvatarControlV1(ref),
    /acknowledgment mismatch/,
  );
  assert.equal(rows(f.s, "actor_controls").length, 2);
});
test("host replay archive must carry exact committed departure evidence and keeps legacy optional shape", async () => {
  const f = fixture(),
    p = proposal(f);
  await f.lot.commitOwnedDrainV1(p);
  const bytes = Buffer.from("checkpoint_1"),
    key = `lots/7/1/1/${p.payload.checkpoint_sha256}`;
  f.bucket.objects.set(key, bytes);
  f.s.sql.exec(
    "INSERT INTO checkpoints(object_key,manifest) VALUES(?,?)",
    key,
    canonical({
      schema_version: 1,
      binding: p.payload.binding,
      cursor: p.payload.result,
      sha256: p.payload.checkpoint_sha256,
      object_key: key,
      byte_length: bytes.length,
    }),
  );
  const reply = {
    kind: "committed",
    batch_id: p.payload.batch_id,
    digest: p.digest,
    cursor: p.payload.result,
    receipt_hex: null,
    ticks_hex: p.payload.ticks_hex,
    effects_hex: p.payload.effects_hex,
    events_hex: p.payload.events_hex,
    control_v2: p.payload.control_v2,
    departure_v1: p.payload.departure_v1,
  };
  const envelope = {
    schema_version: 1,
    binding: p.payload.binding,
    cursor: p.payload.result,
    checkpoint_sha256: p.payload.checkpoint_sha256,
    through_batch_id: p.payload.batch_id,
    records: [
      {
        batch_id: p.payload.batch_id,
        input_sha256: h(800),
        digest: p.digest,
        reply,
      },
    ],
  };
  const missing = clone(envelope);
  delete missing.records[0].reply.departure_v1;
  await assert.rejects(
    f.lot.persistHostReceiptsV1(missing, sha(encode(missing))),
    /departure evidence/,
  );
  const bad = clone(envelope);
  bad.records[0].reply.departure_v1.detach.removal_reason = "forced_cleanup";
  await assert.rejects(
    f.lot.persistHostReceiptsV1(bad, sha(encode(bad))),
    /departure evidence/,
  );
  assert.equal(
    (await f.lot.persistHostReceiptsV1(envelope, sha(encode(envelope)))).status,
    "persisted",
  );
  assert.deepEqual(
    (await f.reopen().lookupHostReceiptV1("1", "1")).record.reply.departure_v1,
    p.payload.departure_v1,
  );
});

test("syntactically valid action_v1 cannot ride trusted drain even with exact control", async () => {
  const f = fixture(),
    p = proposal(f, { detach: false });
  const u64 = (n) => {
    const b = Buffer.alloc(8);
    b.writeBigUInt64LE(BigInt(n));
    return b;
  };
  const u32 = (n) => {
    const b = Buffer.alloc(4);
    b.writeUInt32LE(n);
    return b;
  };
  const packet = (magic, body) =>
    Buffer.concat([Buffer.from(magic + "\r\n\x1a\n"), u64(body.length), body]);
  const action = packet(
    "WLC1",
    Buffer.concat([
      u32(1),
      u64(1),
      u64(4),
      u32(2),
      u32(3),
      u64(4),
      u64(2),
      u64(1),
      u64(17),
    ]),
  );
  const receipt = (tick) =>
    packet(
      "WLA1",
      Buffer.concat([
        u64(action.length),
        action,
        Buffer.from([tick === null ? 0 : 1]),
        ...(tick === null ? [] : [u64(tick)]),
      ]),
    ).toString("hex");
  p.payload.action_v1 = {
    schema_version: 1,
    control: {
      principal: identity.principal,
      account_id: identity.account_id,
      avatar_id: identity.avatar_id,
      control_generation: identity.control_generation,
      session_ref: identity.session_ref,
    },
    input_sha256: h(900),
    request_hex: action.toString("hex"),
    request_sha256: sha(action),
    command_sequence: "1",
    accepted_receipt_hex: receipt(1),
    rejected_receipt_hex: receipt(null),
  };
  resign(p);
  await assert.rejects(f.lot.commitOwnedDrainV1(p), /without player action/);
  noCommittedProof(f);
});

test("old retirement replay cannot remove a replacement generation or principal", async () => {
  const f = fixture();
  await f.lot.commitOwnedDrainV1(proposal(f));
  const ref = references(f).release,
    detach = await f.lot.lookupCommittedAvatarDetachV1(ref),
    release = released(detach, ref);
  f.env.ACCOUNT_REALM = "realm";
  f.env.ACCOUNTS = {
    idFromName: (n) => n,
    get: () => ({ lookupReleasedAvatarControlV1: async () => release }),
  };
  await f.lot.retireReleasedAvatarControlV1(ref);
  const old = JSON.parse(rows(f.s, "actor_control_history")[0].record_json),
    replacement = {
      ...old,
      control_key: "replacement_control_3",
      principal: "3",
      export_digest: h(901),
      state: "active",
    };
  f.s.sql.exec(
    "INSERT INTO actor_controls(control_key,avatar_id,principal,export_digest,export_json,state,expires_at,record) VALUES(?,?,?,?,?,?,?,?)",
    replacement.control_key,
    replacement.avatar_id,
    replacement.principal,
    replacement.export_digest,
    replacement.export_json,
    replacement.state,
    replacement.expires_at,
    replacement.record,
  );
  f.s.sql.exec(
    "INSERT INTO controlled_leases(control_key,revision) VALUES(?,'1')",
    replacement.control_key,
  );
  assert.deepEqual(await f.lot.retireReleasedAvatarControlV1(ref), release);
  assert.equal(rows(f.s, "actor_controls").length, 2);
  assert.ok(
    rows(f.s, "actor_controls").some(
      (r) => r.control_key === replacement.control_key,
    ),
  );
  assert.equal(rows(f.s, "controlled_leases").length, 2);
});

for (const boundary of [
  "INSERT INTO actor_control_history(",
  "DELETE FROM controlled_leases",
  "DELETE FROM actor_controls",
  "DELETE FROM departure_outbox",
  "UPDATE owned_departures SET status='released'",
])
  test(`retirement rollback at ${boundary} keeps claim/outbox intact`, async () => {
    const f = fixture();
    await f.lot.commitOwnedDrainV1(proposal(f));
    const ref = references(f).release,
      detach = await f.lot.lookupCommittedAvatarDetachV1(ref),
      release = released(detach, ref);
    f.env.ACCOUNT_REALM = "realm";
    f.env.ACCOUNTS = {
      idFromName: (n) => n,
      get: () => ({ lookupReleasedAvatarControlV1: async () => release }),
    };
    f.s.afterExec = (q) => {
      if (q.startsWith(boundary)) throw Error("injected retirement failure");
    };
    await assert.rejects(f.lot.retireReleasedAvatarControlV1(ref), /injected/);
    f.s.afterExec = null;
    assert.equal(rows(f.s, "actor_controls").length, 2);
    assert.equal(rows(f.s, "controlled_leases").length, 2);
    assert.equal(rows(f.s, "departure_outbox").length, 2);
    assert.equal(rows(f.s, "actor_control_history").length, 0);
    assert.equal(f.lot.lookupOwnedDepartureV1(request).status, "detached");
    await f.lot.retireReleasedAvatarControlV1(ref);
  });

test("rehashed mutable lineage tip cannot claim a different accepted predecessor", async () => {
  for (const kind of ["batch", "recovery"]) {
    const f = fixture();
    await f.lot.commitOwnedDrainV1(proposal(f, { detach: false }));
    if (kind === "recovery") await takeover(f);
    const old = rows(f.s, "departure_lineage_tips")[0],
      tip = JSON.parse(old.tip_json);
    tip.transition_sequence = "900";
    f.s.sql.exec(
      "UPDATE departure_lineage_tips SET tip_json=?,tip_sha256=?",
      canonical(tip),
      sha(encode(tip)),
    );
    await assert.rejects(
      f.lot.commitOwnedDrainV1(proposal(f, { save: false })),
      /tip .* anchor mismatch/,
    );
    assert.equal(rows(f.s, "departure_detaches").length, 0);
  }
});

test("failed active-tip update and committed-takeover update roll back exact lineage", async () => {
  const f = fixture();
  await f.lot.commitOwnedDrainV1(proposal(f, { detach: false }));
  const tip = rows(f.s, "departure_lineage_tips")[0];
  f.s.afterExec = (q) => {
    if (q.startsWith("INSERT INTO departure_lineage_tips("))
      throw Error("injected tip failure");
  };
  await assert.rejects(
    f.lot.commitOwnedDrainV1(proposal(f, { save: false, detach: false })),
    /injected tip/,
  );
  assert.deepEqual(rows(f.s, "departure_lineage_tips"), [tip]);
  assert.equal(rows(f.s, "batches").length, 1);
  await assert.rejects(takeover(f), /injected tip/);
  assert.deepEqual(rows(f.s, "departure_lineage_tips"), [tip]);
  assert.equal(rows(f.s, "recoveries")[0].status, "pending");
  assert.equal(rows(f.s, "controlled_recovery_receipts").length, 0);
  f.s.afterExec = null;
  await takeover(f);
  assert.equal(rows(f.s, "departure_lineage_tips")[0].anchor_kind, "recovery");
  await f.lot.commitOwnedDrainV1(proposal(f, { save: false }));
});

test(
  "65,537 ordinary commits allocate zero departure history or tips",
  { timeout: 180000 },
  async () => {
    const f = fixture({ begin: false });
    for (let n = 0; n < 65537; n++) {
      await f.lot.commitControlledBatchV2(
        proposal(f, { departure: false, detach: false }),
      );
      // Simulate only pre-existing hot-journal compaction retention here; its R2
      // archive integrity is covered separately. Permanent batch decisions remain.
      if (n % 256 === 255)
        f.s.sql.exec("DELETE FROM journal_chunks; DELETE FROM journal_live");
    }
    assert.equal(rows(f.s, "batches").length, 65537);
    for (const table of [
      "owned_departures",
      "departure_saves",
      "departure_detaches",
      "departure_outbox",
      "departure_anchors",
      "departure_lineage_tips",
      "departure_detach_lineages",
    ])
      assert.equal(rows(f.s, table).length, 0, table);
  },
);

test("server-derived canonical departure operation joins retries and fits strict bounds", () => {
  assert.equal(
    request.departure_operation,
    `${identity.authority_id}_${identity.avatar_id}_${identity.control_generation}`,
  );
  assert.equal(
    departureOperationV1({ ...identity, session_ref: h(999) }),
    request.departure_operation,
  );
  assert.equal(
    departureOperationV1({
      authority_id: "f".repeat(64),
      avatar_id: "16777215",
      control_generation: "18446744073709551615",
    }).length,
    94,
  );
  assert.throws(
    () =>
      departureOperationV1({
        authority_id: identity.authority_id,
        avatar_id: "16777216",
        control_generation: "1",
      }),
    /source range/,
  );
  const f = fixture({ begin: false });
  assert.throws(
    () =>
      f.lot.beginOwnedDepartureV1({
        ...request,
        departure_operation: "browser_chosen_command",
      }),
    /server-derived/,
  );
  noCommittedProof(f);
});

test("active departure enumeration recovers pre-save work and paginates blocked actors", () => {
  const f = fixture();
  assert.equal(f.lot.pendingOwnedDeparturesV1().length, 0);
  assert.deepEqual(
    f.lot
      .activeOwnedDeparturesV1()
      .departures.map((x) => x.departure_operation),
    [request.departure_operation],
  );
  for (let n = 100; n < 230; n++) {
    const i = { ...identity, avatar_id: String(n) };
    const r = {
      schema_version: 1,
      identity: i,
      departure_operation: departureOperationV1(i),
    };
    f.s.sql.exec(
      "INSERT INTO owned_departures(control_key,request_json,status) VALUES(?,?,'blocked')",
      departureControlKey(i),
      canonical(r),
    );
  }
  const seen = [];
  let after = "";
  do {
    const page = f.lot.activeOwnedDeparturesV1(after);
    assert.ok(page.departures.length <= 128);
    seen.push(...page.departures.map((x) => x.departure_operation));
    after = page.next;
  } while (after !== null);
  assert.equal(seen.length, 131);
  assert.equal(new Set(seen).size, 131);
  f.s.sql.exec(
    "UPDATE owned_departures SET status='released' WHERE control_key=?",
    departureControlKey(identity),
  );
  assert.ok(
    !f.lot
      .activeOwnedDeparturesV1()
      .departures.some(
        (x) => x.departure_operation === request.departure_operation,
      ),
  );
  assert.throws(() => f.lot.activeOwnedDeparturesV1({}), /cursor/);
  assert.throws(() => f.lot.activeOwnedDeparturesV1("x".repeat(257)), /cursor/);
});

test("departure retry lookup binds exact owner session and admission after retirement", () => {
  const f = fixture();
  const query = Object.fromEntries(
    ["account_id", "avatar_id", "session_ref", "admission_operation"].map(
      (k) => [k, identity[k]],
    ),
  );
  assert.equal(
    f.lot.findOwnedDepartureV1(query).departure_operation,
    request.departure_operation,
  );
  for (const [field, value] of [
    ["account_id", "999"],
    ["avatar_id", "999"],
    ["session_ref", h(999)],
    ["admission_operation", "different_admission"],
  ])
    assert.equal(
      f.lot.findOwnedDepartureV1({ ...query, [field]: value }),
      null,
    );
  f.s.sql.exec(
    "UPDATE owned_departures SET status='released' WHERE control_key=?",
    departureControlKey(identity),
  );
  assert.equal(f.lot.findOwnedDepartureV1(query).status, "released");
  assert.throws(
    () =>
      f.lot.findOwnedDepartureV1({
        ...query,
        url: "https://untrusted.invalid",
      }),
    /fields/,
  );
});
