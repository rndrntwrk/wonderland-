/** SQL-backed account tests. Fixtures do not claim a native source leave ran or
 * that a real coordinator batch committed. Production trusted lookup is required.
 */
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { readFile, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";
import { createHash } from "node:crypto";
import { build } from "esbuild";
const root = await mkdtemp(join(tmpdir(), "wonderland-owned-leave-save-"));
const bundle = join(root, "lot-save.mjs");
await build({
  entryPoints: ["src/account/lot-save.ts"],
  bundle: true,
  format: "esm",
  platform: "node",
  outfile: bundle,
});
const {
  AccountLotSaveStore,
  sourceLotSaveColumns,
  sourceCurrentJobRow,
  sourceAvatarLotSaveV1,
  mergeSourceAvatarLotSaveV1,
  sourceLockColumn,
  sourceLockReload,
  committedAvatarDetachV1,
  saveProofReferenceV1,
  departureIdentityV1,
  existingAvatarSourceV1,
  sourceI16,
  sourceU16,
  saveReference,
} = await import(pathToFileURL(bundle));
const canonical = (v) =>
  v === null || typeof v !== "object"
    ? JSON.stringify(v)
    : Array.isArray(v)
      ? `[${v.map(canonical).join(",")}]`
      : `{${Object.entries(v)
          .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
          .map(([k, v]) => `${JSON.stringify(k)}:${canonical(v)}`)
          .join(",")}}`;
const sha = (v) => createHash("sha256").update(canonical(v)).digest("hex");
const clone = (v) => JSON.parse(JSON.stringify(v));
const h = (digit) => digit.repeat(64);
const SOURCE_PIN = "4c6b3e8f5835b228723caea3c9f683c62f244f73";
// Extract actual AccountAuthority DDL, not a separate toy account schema.
const authorityCode = await readFile("src/account/authority.ts", "utf8");
const ddl = authorityCode.match(/ctx\.storage\.sql\.exec\(`([\s\S]*?)`\);/)[1];
class Storage {
  constructor() {
    this.db = new DatabaseSync(":memory:");
    this.db.exec(ddl);
    this.sql = {
      exec: (query, ...args) => {
        if (
          !args.length &&
          (query.trim().split(";").filter(Boolean).length > 1 ||
            /^CREATE TRIGGER|^DROP TRIGGER/.test(query.trim()))
        ) {
          this.db.exec(query);
          return {
            toArray: () => [],
            one: () => {
              throw Error("no row");
            },
          };
        }
        const rows = this.db
          .prepare(query)
          .all(...args)
          .map((row) => ({ ...row }));
        return {
          toArray: () => rows,
          one: () => {
            if (rows.length !== 1) throw Error("not exactly one row");
            return rows[0];
          },
        };
      },
    };
  }
  transactionSync(fn) {
    this.db.exec("BEGIN IMMEDIATE");
    try {
      const value = fn();
      this.db.exec("COMMIT");
      return value;
    } catch (e) {
      this.db.exec("ROLLBACK");
      throw e;
    }
  }
  snapshot() {
    return Object.fromEntries(
      this.db
        .prepare(
          "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
        )
        .all()
        .map(({ name }) => [
          name,
          this.sql.exec(`SELECT * FROM ${name} ORDER BY rowid`).toArray(),
        ]),
    );
  }
}
const identity = {
  authority_id: h("a"),
  account_id: "1",
  avatar_id: "7",
  lot_id: "16777472",
  control_generation: "3",
  session_ref: h("2"),
  admission_operation: "owned_admission_0001",
  reservation_digest: h("3"),
  export_digest: h("4"),
  principal: "99",
  actor: { object_id: 10, generation: 8 },
};
const spec = {
  name: "Source Owner",
  description: "Leave test",
  gender: "female",
  skin: "dark",
  head_key: "100",
  body_key: "200",
  release: h("b"),
};
const source = {
  schema_version: 1,
  source_pin: SOURCE_PIN,
  created_at_ms: 1700000000000,
  body_current: "200",
  default_suits: { daywear: "200", swimwear: "201", sleepwear: "202" },
  motive_data: Array.from({ length: 16 }, (_, i) => i * 10),
  person_data: Array.from({ length: 27 }, (_, i) => i + 100),
  relationships: [
    { target: "99", values: [11, 12, 13] },
    { target: "100", values: [21, 22] },
  ],
  job_info: [
    { id: -1, experience: 17, level: 18, sick_days: 19, status_flags: 20 },
    { id: 2, experience: 21, level: 22, sick_days: 23, status_flags: 24 },
  ],
  ignored_avatars: ["44"],
  roommate_lots: [{ lot_id: "16777472", permissions_level: 2, is_pending: 0 }],
  moderation_level: 7,
  mayor_nhood: "13",
};
const outfits = [
  {
    id: "1",
    asset_key: "200",
    suit_type: 0,
    source: "cas",
    purchase_price: "0",
    sale_price: "0",
  },
  {
    id: "2",
    asset_key: "201",
    suit_type: 2,
    source: "cas",
    purchase_price: "0",
    sale_price: "0",
  },
  {
    id: "3",
    asset_key: "202",
    suit_type: 5,
    source: "cas",
    purchase_price: "0",
    sale_price: "0",
  },
  {
    id: "4",
    asset_key: "203",
    suit_type: 0,
    source: "purchase",
    purchase_price: "1000",
    sale_price: "300",
  },
];
const person = [
  -32768, -1, 32767, 1, -199, 100, 1, -3, 3, 4, -1, -199, -99, -32768, 32767,
  199, -100, -1, 32767, -32768, -2, -1, 1, 2, 3, 4, 5,
];
const columns = sourceLotSaveColumns({
  motive_data: [-32768, -1, 32767, ...Array(13).fill(10)],
  person_data: person,
  default_suits: { daywear: "203", swimwear: "201", sleepwear: "202" },
  body_for_persistence: "18446744073709551615",
});
const origin = {
  binding: {
    protocol: 1,
    source_pin: SOURCE_PIN,
    binding_sha256: h("5"),
    lot_incarnation: "1",
  },
  content_release: spec.release,
  protocol_release: h("c"),
  batch_id: "11",
  proposal_digest: h("6"),
  predecessor: {
    lot_id: identity.lot_id,
    authority_epoch: "2",
    completed_tick: "10",
    state_hash: h("7"),
  },
  result: {
    lot_id: identity.lot_id,
    authority_epoch: "2",
    completed_tick: "11",
    state_hash: h("8"),
  },
  checkpoint_sha256: h("9"),
  captured_tick: "11",
  event_ordinal: 0,
};
function records() {
  const save = {
    schema_version: 1,
    source_pin: SOURCE_PIN,
    identity: clone(identity),
    departure_operation: "owned_departure_0001",
    save_sequence: "1",
    captured_tick: "11",
    capture_reason: "first_leave_tick",
    avatar_columns: clone(columns),
    current_job_row: {
      job_type: 65535,
      job_experience: 65535,
      job_level: 32768,
      job_sickdays: 1,
      job_statusflags: 65534,
    },
    changed_relationships: [
      { target: "99", values: [-32768] },
      { target: "100", values: [] },
      { target: "4294967295", values: [1, -1] },
    ],
  };
  const committed = {
    schema_version: 1,
    status: "committed_source_save",
    identity: clone(identity),
    departure_operation: save.departure_operation,
    save_sha256: sha(save),
    origin: clone(origin),
    save_record: save,
    source_baseline: { progression: sha(source), wardrobe: sha(outfits) },
  };
  const detach = {
    schema_version: 1,
    status: "committed_actor_detach",
    identity: clone(identity),
    departure_operation: save.departure_operation,
    source_save: saveReference(committed),
    detached_tick: "12",
    removal_reason: "source_leave",
    origin: {
      ...clone(origin),
      batch_id: "12",
      captured_tick: "12",
      proposal_digest: h("d"),
      predecessor: clone(origin.result),
      result: {
        ...clone(origin.result),
        completed_tick: "12",
        state_hash: h("e"),
      },
    },
    control_revision_before: "4",
    control_revision_after: "5",
    actor_registry_before_sha256: h("1"),
    actor_registry_after_sha256: h("f"),
  };
  return { committed, save, detach };
}
const detachReference = (receipt) => ({
  schema_version: 1,
  identity: clone(receipt.identity),
  departure_operation: receipt.departure_operation,
  detach_sha256: sha(receipt),
  origin: clone(receipt.origin),
});
function seed(storage, id = identity, initialSource = source) {
  const sql = storage.sql;
  const claim = {
    version: 1,
    account_id: id.account_id,
    avatar_id: id.avatar_id,
    lot_id: id.lot_id,
    generation: id.control_generation,
    content_release: spec.release,
    protocol_release: origin.protocol_release,
    expires_at: 1,
    state: "fence_pending",
  };
  const frozen = {
    authority_id: id.authority_id,
    account_id: id.account_id,
    account_revision: "1",
    session_ref: id.session_ref,
    admission_operation: id.admission_operation,
    reservation_digest: id.reservation_digest,
    claim: { ...claim, expires_at: 0 },
    spec: clone(spec),
    source: clone(initialSource),
    registered_at_ms: 1600000000000,
    balance: "7000",
    outfits: outfits.map((row) => ({ ...row, id: `${id.avatar_id}${row.id}` })),
    permission_binding: { revision: "1", sha256: h("a") },
    observed_at_ms: 1800000000000,
  };
  const exported = {
    schema_version: 2,
    authority_id: id.authority_id,
    account_id: id.account_id,
    avatar_id: id.avatar_id,
    lot_id: id.lot_id,
    control_generation: id.control_generation,
    session_ref: id.session_ref,
    admission_operation: id.admission_operation,
    reservation_digest: id.reservation_digest,
    export_digest: id.export_digest,
    account_revision: "1",
    content_release: spec.release,
    protocol_release: origin.protocol_release,
    source_revisions: {
      progression: sha(initialSource),
      wardrobe: sha(frozen.outfits),
    },
  };
  sql.exec(
    "INSERT OR IGNORE INTO account_meta(key,value) VALUES('admission_authority_v2',?)",
    id.authority_id,
  );
  sql.exec(
    "INSERT OR IGNORE INTO account_meta(key,value) VALUES('control_generation',?)",
    id.control_generation,
  );
  sql.exec(
    "INSERT OR IGNORE INTO accounts(id,username,verifier,revision) VALUES(?,?,?,'9')",
    id.account_id,
    `owner_${id.account_id}`,
    "unchanged verifier",
  );
  // No session row: expired/logout/rotated sessions cannot prevent cleanup.
  sql.exec(
    "INSERT OR IGNORE INTO account_source_state(account_id,registered_at_ms) VALUES(?,1600000000000)",
    id.account_id,
  );
  sql.exec(
    "INSERT INTO avatars(id,account_id,name_key,spec) VALUES(?,?,?,?)",
    id.avatar_id,
    id.account_id,
    `owner_${id.avatar_id}`,
    canonical(spec),
  );
  sql.exec(
    "INSERT INTO avatar_source_state(avatar_id,source_json) VALUES(?,?)",
    id.avatar_id,
    canonical(initialSource),
  );
  sql.exec(
    "INSERT INTO avatar_ownership_initializations(avatar_id,version) VALUES(?,1)",
    id.avatar_id,
  );
  sql.exec(
    "INSERT INTO avatar_balances(avatar_id,balance) VALUES(?,'7000')",
    id.avatar_id,
  );
  for (const row of frozen.outfits)
    sql.exec(
      "INSERT INTO avatar_outfits(id,avatar_id,asset_key,suit_type,source,purchase_price,sale_price) VALUES(?,?,?,?,?,?,?)",
      row.id,
      id.avatar_id,
      row.asset_key,
      row.suit_type,
      row.source,
      row.purchase_price,
      row.sale_price,
    );
  sql.exec(
    "INSERT INTO avatar_control_claims(avatar_id,account_id,session_hash,session_revision,operation,digest,claim) VALUES(?,?,?,'1',?,?,?)",
    id.avatar_id,
    id.account_id,
    id.session_ref,
    id.admission_operation,
    id.reservation_digest,
    canonical(claim),
  );
  sql.exec(
    "INSERT INTO avatar_control_exports(avatar_id,operation,source_json,export_json) VALUES(?,?,?,?)",
    id.avatar_id,
    id.admission_operation,
    canonical(frozen),
    canonical(exported),
  );
  sql.exec(
    "INSERT INTO avatar_control_leases(avatar_id,revision,expires_at) VALUES(?,'4',1)",
    id.avatar_id,
  );
  sql.exec(
    "INSERT INTO avatar_operations(account_id,operation,digest,result) VALUES(?,?,?,?)",
    id.account_id,
    `create_operation_${id.avatar_id}`,
    h("9"),
    canonical({ avatar_id: id.avatar_id, ...spec }),
  );
  return { claim, frozen, exported };
}
function fixture() {
  const storage = new Storage(),
    state = records(),
    seeded = seed(storage);
  state.committed.source_baseline.wardrobe =
    seeded.exported.source_revisions.wardrobe;
  let saveReads = 0,
    detachReads = 0;
  const reader = {
    verifyCommittedSave: async () => {
      saveReads++;
      return clone(state.committed);
    },
    verifyCommittedDetach: async () => {
      detachReads++;
      return clone(state.detach);
    },
  };
  const store = new AccountLotSaveStore(storage, reader);
  return {
    storage,
    state,
    seeded,
    reader,
    store,
    reads: () => ({ saveReads, detachReads }),
  };
}
function assertUnchangedExcept(before, after, tables) {
  for (const [name, values] of Object.entries(before))
    if (!tables.includes(name))
      assert.deepEqual(after[name], values, `unexpected writes to ${name}`);
}
async function rejectedWithoutWrites(f, action, match) {
  const before = f.storage.snapshot();
  await assert.rejects(action(), match);
  assert.deepEqual(f.storage.snapshot(), before);
}
let passed = 0;
async function test(name, fn) {
  await fn();
  passed++;
  console.log(`PASS ${name}`);
}
try {
  await test("exact StateToDb casts, big-endian motives and reload normalization", () => {
    assert.equal(columns.motive_data_hex.slice(0, 12), "8000ffff7fff");
    assert.equal(columns.motive_data_hex.slice(44, 48), "0000");
    assert.equal(columns.skilllock, 255);
    assert.deepEqual(
      [
        columns.lock_body,
        columns.lock_charisma,
        columns.lock_cooking,
        columns.lock_creativity,
        columns.lock_logic,
        columns.lock_mechanical,
      ],
      [65535, 0, 65209, 327, 1, 65535],
    );
    assert.deepEqual(
      [
        columns.skill_cooking,
        columns.skill_charisma,
        columns.skill_mechanical,
        columns.skill_creativity,
        columns.skill_body,
        columns.skill_logic,
      ],
      [32768, 65535, 32767, 1, 65337, 100],
    );
    for (const n of [-32768, -199, -100, -99, -1, 0, 99, 100, 199, 32767])
      assert.equal(
        sourceLockReload(sourceLockColumn(n)),
        sourceI16(Math.trunc(n / 100) * 100),
      );
    assert.deepEqual(
      sourceCurrentJobRow(person, [
        {
          id: -1,
          experience: -1,
          level: -32768,
          sick_days: 1,
          status_flags: -2,
        },
      ]),
      records().save.current_job_row,
    );
    assert.equal(sourceCurrentJobRow(person, []), null);
    assert.equal(
      sourceCurrentJobRow(
        person.map((v, i) => (i === 21 ? 0 : v)),
        source.job_info,
      ),
      null,
    );
    assert.throws(
      () =>
        sourceLotSaveColumns({
          motive_data: source.motive_data,
          person_data: person,
          default_suits: source.default_suits,
          body_for_persistence: "named outfit",
        }),
      /decimal/,
    );
    assert.equal(sourceU16(-1), 65535);
  });
  await test("strict whitelist, unknown fields, capacities, sorted u32 targets and SleepState", () => {
    const { save } = records();
    assert.deepEqual(sourceAvatarLotSaveV1(save), save);
    for (const field of [
      "budget",
      "permissions",
      "name",
      "portable_json",
      "avatar_flags",
      "ignored_avatars",
      "url",
    ]) {
      assert.throws(
        () => sourceAvatarLotSaveV1({ ...save, [field]: 0 }),
        /fields/,
      );
      assert.throws(
        () =>
          sourceAvatarLotSaveV1({
            ...save,
            avatar_columns: { ...save.avatar_columns, [field]: 0 },
          }),
        /fields/,
      );
    }
    for (const changed of [
      [...save.changed_relationships].reverse(),
      [{ target: "0", values: [] }],
      [{ target: "4294967296", values: [] }],
      [{ target: "99", values: Array(257).fill(0) }],
      [{ target: "99", values: [32768] }],
      [
        { target: "99", values: [] },
        { target: "99", values: [] },
      ],
      Array(257).fill({ target: "99", values: [] }),
    ])
      assert.throws(() =>
        sourceAvatarLotSaveV1({ ...save, changed_relationships: changed }),
      );
    assert.throws(
      () =>
        sourceAvatarLotSaveV1({
          ...save,
          avatar_columns: {
            ...columns,
            motive_data_hex:
              columns.motive_data_hex.slice(0, 44) +
              "0001" +
              columns.motive_data_hex.slice(48),
          },
        }),
      /SleepState/,
    );
    assert.throws(
      () =>
        sourceAvatarLotSaveV1({
          ...save,
          capture_reason: "guessed_pre_remove",
        }),
      /profile/,
    );
    assert.throws(
      () =>
        sourceAvatarLotSaveV1({
          ...save,
          current_job_row: { ...save.current_job_row, job_type: 2 },
        }),
      /current-job/,
    );
    assert.throws(
      () =>
        sourceAvatarLotSaveV1({
          ...save,
          changed_relationships: Array.from({ length: 256 }, (_, i) => ({
            target: String(i + 1),
            values: Array(256).fill(-32768),
          })),
        }),
      /byte capacity/,
    );
    const sparse = clone(save);
    sparse.changed_relationships[0].values = Array(1);
    assert.throws(() => sourceAvatarLotSaveV1(sparse), /integer/);
    assert.throws(
      () =>
        saveProofReferenceV1({
          ...saveReference(records().committed),
          url: "https://untrusted.invalid",
        }),
      /fields/,
    );
    assert.throws(() =>
      departureIdentityV1({
        ...identity,
        actor: { object_id: 0, generation: 1 },
      }),
    );
  });
  await test("selective merge preserves other jobs, relationship tails and every excluded source slot", () => {
    const { save } = records(),
      merged = mergeSourceAvatarLotSaveV1(source, save);
    assert.deepEqual(source, existingAvatarSourceV1(source));
    assert.deepEqual(merged.relationships, [
      { target: "99", values: [-32768, 12, 13] },
      { target: "100", values: [21, 22] },
      { target: "4294967295", values: [1, -1] },
    ]);
    assert.deepEqual(merged.job_info, [
      { id: -1, experience: -1, level: -32768, sick_days: 1, status_flags: -2 },
      source.job_info[1],
    ]);
    for (const i of [6, 8, 9, 22, 23, 24, 25, 26])
      assert.equal(merged.person_data[i], source.person_data[i]);
    for (const key of [
      "schema_version",
      "source_pin",
      "created_at_ms",
      "ignored_avatars",
      "roommate_lots",
      "moderation_level",
      "mayor_nhood",
    ])
      assert.deepEqual(merged[key], source[key]);
    assert.equal(merged.body_current, "18446744073709551615");
    const noJob = mergeSourceAvatarLotSaveV1(source, {
      ...save,
      current_job_row: null,
      changed_relationships: [{ target: "500", values: [] }],
    });
    assert.deepEqual(noJob.job_info, source.job_info);
    assert.deepEqual(noJob.relationships, source.relationships);
  });
  await test("production SQLite tables: expiry/logout/rotation save keeps custody and touches only whitelist", async () => {
    const f = fixture(),
      other = {
        ...identity,
        account_id: "2",
        avatar_id: "8",
        principal: "100",
        control_generation: "4",
        admission_operation: "owned_admission_0002",
      };
    seed(f.storage, other);
    const before = f.storage.snapshot(),
      receipt = await f.store.applyCommittedAvatarLotSaveV1(
        saveReference(f.state.committed),
      ),
      after = f.storage.snapshot();
    assert.equal(receipt.status, "applied_source_save");
    assert.equal(
      receipt.source_progression_after,
      sha(mergeSourceAvatarLotSaveV1(source, f.state.save)),
    );
    assertUnchangedExcept(before, after, [
      "avatar_source_state",
      "avatars",
      "avatar_control_claims",
      "avatar_lot_save_receipts",
      "avatar_control_operation_tombstones",
    ]);
    const afterSpec = JSON.parse(
      after.avatars.find((r) => r.id === identity.avatar_id).spec,
    );
    assert.equal(afterSpec.body_key, "203");
    assert.deepEqual({ ...afterSpec, body_key: spec.body_key }, spec);
    for (const table of [
      "avatar_source_state",
      "avatars",
      "avatar_control_claims",
    ])
      assert.deepEqual(
        after[table].at(-1),
        before[table].at(-1),
        "other account untouched",
      );
    assert.equal(after.avatar_control_claims.length, 2);
    assert.equal(
      f.store.lookupSavedAvatarControlV1(
        identity,
        f.state.save.departure_operation,
      ).source_progression_after,
      receipt.source_progression_after,
    );
    assert.throws(
      () =>
        f.store.assertReservationOperationUnused(
          identity.account_id,
          identity.admission_operation,
        ),
      /already terminal/,
    );
    f.store.assertReservationOperationUnused(
      identity.account_id,
      "unused_operation_0001",
    );
    const retryBefore = f.storage.snapshot();
    assert.deepEqual(
      await f.store.applyCommittedAvatarLotSaveV1(
        saveReference(f.state.committed),
      ),
      receipt,
    );
    assert.deepEqual(f.storage.snapshot(), retryBefore);
    assert.equal(f.reads().saveReads, 1);
  });
  await test("required trusted lookup rejects missing/prepared/rejected/wrong-origin proof without writes", async () => {
    assert.throws(
      () => new AccountLotSaveStore(new Storage(), undefined),
      /trusted/,
    );
    for (const variant of [
      null,
      { ...records().committed, status: "prepared" },
    ]) {
      const f = fixture();
      f.reader.verifyCommittedSave = async () => variant;
      await rejectedWithoutWrites(f, () =>
        f.store.applyCommittedAvatarLotSaveV1(saveReference(f.state.committed)),
      );
    }
    const rejected = fixture();
    rejected.reader.verifyCommittedSave = async () => {
      throw Error("coordinator batch rejected");
    };
    await rejectedWithoutWrites(
      rejected,
      () =>
        rejected.store.applyCommittedAvatarLotSaveV1(
          saveReference(rejected.state.committed),
        ),
      /rejected/,
    );
    for (const alter of [
      (r) => (r.origin.proposal_digest = h("0")),
      (r) => (r.origin.binding.lot_incarnation = "2"),
      (r) => r.identity.actor.generation++,
      (r) => (r.identity.account_id = "2"),
      (r) => r.save_record.avatar_columns.skill_body++,
    ]) {
      const f = fixture(),
        ref = saveReference(clone(f.state.committed));
      alter(f.state.committed);
      await rejectedWithoutWrites(f, () =>
        f.store.applyCommittedAvatarLotSaveV1(ref),
      );
    }
  });
  await test("frozen baseline, real source presence, and currently owned default suits required", async () => {
    for (const mutate of [
      (f) =>
        f.storage.sql.exec(
          "DELETE FROM avatar_source_state WHERE avatar_id=?",
          identity.avatar_id,
        ),
      (f) =>
        f.storage.sql.exec(
          "DELETE FROM avatar_ownership_initializations WHERE avatar_id=?",
          identity.avatar_id,
        ),
      (f) =>
        f.storage.sql.exec(
          "DELETE FROM avatar_control_exports WHERE avatar_id=?",
          identity.avatar_id,
        ),
      (f) =>
        f.storage.sql.exec(
          "DELETE FROM avatar_outfits WHERE avatar_id=? AND asset_key='203'",
          identity.avatar_id,
        ),
      (f) => (f.state.committed.source_baseline.progression = h("0")),
      (f) => (f.state.committed.source_baseline.wardrobe = h("0")),
      (f) =>
        f.storage.sql.exec(
          "UPDATE avatar_source_state SET source_json=? WHERE avatar_id=?",
          canonical({ ...source, moderation_level: 8 }),
          identity.avatar_id,
        ),
    ]) {
      const f = fixture();
      mutate(f);
      await rejectedWithoutWrites(f, () =>
        f.store.applyCommittedAvatarLotSaveV1(saveReference(f.state.committed)),
      );
    }
  });
  await test("async trusted lookup cannot race a source or generation replacement", async () => {
    for (const kind of ["source", "generation"]) {
      const f = fixture();
      let unblock, entered;
      const started = new Promise((resolve) => (entered = resolve));
      f.reader.verifyCommittedSave = async () => {
        entered();
        await new Promise((resolve) => (unblock = resolve));
        return clone(f.state.committed);
      };
      const pending = f.store.applyCommittedAvatarLotSaveV1(
        saveReference(f.state.committed),
      );
      await started;
      if (kind === "source")
        f.storage.sql.exec(
          "UPDATE avatar_source_state SET source_json=? WHERE avatar_id=?",
          canonical({ ...source, body_current: "777" }),
          identity.avatar_id,
        );
      else
        f.storage.sql.exec(
          "UPDATE avatar_control_claims SET claim=? WHERE avatar_id=?",
          canonical({ ...f.seeded.claim, generation: "9" }),
          identity.avatar_id,
        );
      const changed = f.storage.snapshot();
      unblock();
      await assert.rejects(
        pending,
        /baseline changed|control or export mismatch/,
      );
      assert.deepEqual(f.storage.snapshot(), changed);
    }
  });
  await test("SQLite save failure is atomic across merge, fence, receipt and tombstone", async () => {
    const f = fixture();
    f.storage.sql.exec(
      "CREATE TRIGGER fail_save BEFORE INSERT ON avatar_lot_save_receipts BEGIN SELECT RAISE(ABORT, 'injected save failure'); END;",
    );
    await rejectedWithoutWrites(
      f,
      () =>
        f.store.applyCommittedAvatarLotSaveV1(saveReference(f.state.committed)),
      /injected save/,
    );
    f.storage.sql.exec("DROP TRIGGER fail_save");
    await f.store.applyCommittedAvatarLotSaveV1(
      saveReference(f.state.committed),
    );
  });
  await test("save alone and detach alone cannot release; exact independent proof gates release", async () => {
    const f = fixture();
    await rejectedWithoutWrites(
      f,
      () =>
        f.store.releaseSavedAvatarControlV1(detachReference(f.state.detach)),
      /account save required/,
    );
    await f.store.applyCommittedAvatarLotSaveV1(
      saveReference(f.state.committed),
    );
    assert.equal(f.storage.snapshot().avatar_control_claims.length, 1);
    const wrong = clone(f.state.detach);
    wrong.source_save.save_sha256 = h("0");
    f.reader.verifyCommittedDetach = async () => wrong;
    await rejectedWithoutWrites(
      f,
      () => f.store.releaseSavedAvatarControlV1(detachReference(wrong)),
      /save identity reused/,
    );
    f.reader.verifyCommittedDetach = async () => clone(f.state.detach);
    const before = f.storage.snapshot(),
      released = await f.store.releaseSavedAvatarControlV1(
        detachReference(f.state.detach),
      ),
      after = f.storage.snapshot();
    assert.equal(released.status, "released_saved_control");
    assert.equal(
      released.account_save_receipt_sha256,
      sha(
        f.store.lookupSavedAvatarControlV1(
          identity,
          f.state.save.departure_operation,
        ),
      ),
    );
    assert.equal(after.avatar_control_claims.length, 0);
    assert.equal(after.avatar_control_leases.length, 0);
    assert.equal(after.avatar_control_exports.length, 0);
    assertUnchangedExcept(before, after, [
      "avatar_control_claims",
      "avatar_control_leases",
      "avatar_control_exports",
      "avatar_control_release_receipts",
    ]);
    assert.deepEqual(
      f.store.lookupReleasedAvatarControlV1(
        identity,
        f.state.save.departure_operation,
      ),
      released,
    );
  });
  await test("release transaction failure retains claim, export, lease and immutable save", async () => {
    const f = fixture();
    await f.store.applyCommittedAvatarLotSaveV1(
      saveReference(f.state.committed),
    );
    f.storage.sql.exec(
      "CREATE TRIGGER fail_release BEFORE DELETE ON avatar_control_claims BEGIN SELECT RAISE(ABORT, 'injected release failure'); END;",
    );
    await rejectedWithoutWrites(
      f,
      () =>
        f.store.releaseSavedAvatarControlV1(detachReference(f.state.detach)),
      /injected release/,
    );
    f.storage.sql.exec("DROP TRIGGER fail_release");
    await f.store.releaseSavedAvatarControlV1(detachReference(f.state.detach));
  });
  await test("exact old save/release retry after re-admission cannot mutate newer generation", async () => {
    const f = fixture(),
      saved = await f.store.applyCommittedAvatarLotSaveV1(
        saveReference(f.state.committed),
      ),
      released = await f.store.releaseSavedAvatarControlV1(
        detachReference(f.state.detach),
      );
    f.storage.sql.exec(
      "UPDATE account_meta SET value='50' WHERE key='control_generation'",
    );
    f.storage.sql.exec(
      "INSERT INTO avatar_control_claims(avatar_id,account_id,session_hash,session_revision,operation,digest,claim) VALUES(?,?,?,'9',?,?,?)",
      identity.avatar_id,
      identity.account_id,
      h("1"),
      "owned_admission_new1",
      h("2"),
      canonical({ ...f.seeded.claim, generation: "50", state: "reserved" }),
    );
    f.storage.sql.exec(
      "INSERT INTO avatar_control_leases(avatar_id,revision,expires_at) VALUES(?,'1',9999999)",
      identity.avatar_id,
    );
    f.storage.sql.exec(
      "INSERT INTO avatar_control_exports(avatar_id,operation,source_json,export_json) VALUES(?,?,?,?)",
      identity.avatar_id,
      "owned_admission_new1",
      "new-source",
      "new-export",
    );
    const before = f.storage.snapshot();
    assert.deepEqual(
      await f.store.applyCommittedAvatarLotSaveV1(
        saveReference(f.state.committed),
      ),
      saved,
    );
    assert.deepEqual(
      await f.store.releaseSavedAvatarControlV1(
        detachReference(f.state.detach),
      ),
      released,
    );
    assert.deepEqual(f.storage.snapshot(), before);
    await rejectedWithoutWrites(
      f,
      () =>
        f.store.applyCommittedAvatarLotSaveV1({
          ...saveReference(f.state.committed),
          save_sha256: h("0"),
        }),
      /identity reused/,
    );
    await rejectedWithoutWrites(
      f,
      () =>
        f.store.releaseSavedAvatarControlV1({
          ...detachReference(f.state.detach),
          detach_sha256: h("0"),
        }),
      /identity reused/,
    );
    assert.throws(
      () =>
        f.store.assertReservationOperationUnused(
          identity.account_id,
          identity.admission_operation,
        ),
      /already terminal/,
    );
  });
  await test("detach after retained-save takeover allows newer host incarnation/epoch", async () => {
    const f = fixture();
    await f.store.applyCommittedAvatarLotSaveV1(
      saveReference(f.state.committed),
    );
    f.state.detach.origin.binding = {
      ...f.state.detach.origin.binding,
      lot_incarnation: "9",
      binding_sha256: h("0"),
    };
    f.state.detach.origin.predecessor.authority_epoch = "99";
    f.state.detach.origin.result.authority_epoch = "99";
    // Reader establishes ancestry; account preserves exact actor/generation.
    committedAvatarDetachV1(f.state.detach);
    assert.equal(
      (
        await f.store.releaseSavedAvatarControlV1(
          detachReference(f.state.detach),
        )
      ).status,
      "released_saved_control",
    );
  });
  await test("async detach race never erases a replacement active claim", async () => {
    const f = fixture();
    await f.store.applyCommittedAvatarLotSaveV1(
      saveReference(f.state.committed),
    );
    let unblock, entered;
    const started = new Promise((resolve) => (entered = resolve));
    f.reader.verifyCommittedDetach = async () => {
      entered();
      await new Promise((resolve) => (unblock = resolve));
      return clone(f.state.detach);
    };
    const pending = f.store.releaseSavedAvatarControlV1(
      detachReference(f.state.detach),
    );
    await started;
    f.storage.sql.exec(
      "UPDATE avatar_control_claims SET claim=? WHERE avatar_id=?",
      canonical({ ...f.seeded.claim, generation: "20" }),
      identity.avatar_id,
    );
    const changed = f.storage.snapshot();
    unblock();
    await assert.rejects(pending, /release current control/);
    assert.deepEqual(f.storage.snapshot(), changed);
  });
  await test("immutable operation conflict rolls back second-generation save", async () => {
    const f = fixture();
    await f.store.applyCommittedAvatarLotSaveV1(
      saveReference(f.state.committed),
    );
    const otherId = {
      ...identity,
      avatar_id: "8",
      control_generation: "4",
      principal: "100",
      admission_operation: "owned_admission_0002",
    };
    const seeded = seed(f.storage, otherId),
      other = records().committed;
    other.identity = clone(otherId);
    other.save_record.identity = clone(otherId);
    other.save_sha256 = sha(other.save_record);
    other.source_baseline.wardrobe = seeded.exported.source_revisions.wardrobe;
    f.reader.verifyCommittedSave = async () => other;
    await rejectedWithoutWrites(
      f,
      () => f.store.applyCommittedAvatarLotSaveV1(saveReference(other)),
      /UNIQUE/,
    );
  });
  await test("concurrent ledger and wardrobe price changes survive source save", async () => {
    const f = fixture();
    f.storage.sql.exec(
      "UPDATE avatar_balances SET balance='8123' WHERE avatar_id=?",
      identity.avatar_id,
    );
    f.storage.sql.exec(
      "UPDATE avatar_outfits SET sale_price='444' WHERE avatar_id=?",
      identity.avatar_id,
    );
    const before = f.storage.snapshot();
    await f.store.applyCommittedAvatarLotSaveV1(
      saveReference(f.state.committed),
    );
    const after = f.storage.snapshot();
    assert.deepEqual(after.avatar_balances, before.avatar_balances);
    assert.deepEqual(after.avatar_outfits, before.avatar_outfits);
    assert.equal(after.avatar_balances[0].balance, "8123");
  });
  await test("snapshot recheck catches a change after proof lookup and before final transaction", async () => {
    const f = fixture();
    const transaction = f.storage.transactionSync.bind(f.storage);
    let changed;
    f.storage.transactionSync = (fn) => {
      f.storage.sql.exec(
        "UPDATE avatar_source_state SET source_json=? WHERE avatar_id=?",
        canonical({ ...source, body_current: "999" }),
        identity.avatar_id,
      );
      changed = f.storage.snapshot();
      return transaction(fn);
    };
    await assert.rejects(
      f.store.applyCommittedAvatarLotSaveV1(saveReference(f.state.committed)),
      /changed during proof verification/,
    );
    assert.deepEqual(f.storage.snapshot(), changed);
  });
  await test("relationship merge capacity blocks rather than truncates existing source", async () => {
    const f = fixture();
    const full = {
      ...source,
      relationships: Array.from({ length: 256 }, (_, i) => ({
        target: String(i + 1),
        values: [1],
      })),
    };
    f.storage.sql.exec(
      "UPDATE avatar_source_state SET source_json=? WHERE avatar_id=?",
      canonical(full),
      identity.avatar_id,
    );
    f.seeded.frozen.source = full;
    f.seeded.exported.source_revisions.progression = sha(full);
    f.state.committed.source_baseline.progression = sha(full);
    f.storage.sql.exec(
      "UPDATE avatar_control_exports SET source_json=?,export_json=? WHERE avatar_id=?",
      canonical(f.seeded.frozen),
      canonical(f.seeded.exported),
      identity.avatar_id,
    );
    await rejectedWithoutWrites(
      f,
      () =>
        f.store.applyCommittedAvatarLotSaveV1(saveReference(f.state.committed)),
      /relationship capacity/,
    );
  });
  console.log(
    `PASS ${passed} owned leave SQL-backed account tests; native/coordinator source journey remains separately required`,
  );
} finally {
  await rm(root, { recursive: true, force: true });
}

/** Run separately/serially with --workerd when the shared workerd slot is free.
 * Uses the actual AccountAuthority class, real password WASM, durable storage,
 * and the configured COORD namespace. Only the coordinator is a test fixture.
 */
async function runActualAccountAuthority() {
  const { Miniflare } = await import("miniflare");
  const worker = `
    import { DurableObject } from "cloudflare:workers";
    import { AccountAuthority } from "./src/account/authority";
    import { canonical, equal } from "./src/lot/contracts";
    import { saveReference } from "./src/account/lot-save";
    export class TestAccounts extends AccountAuthority {
      testSource(id) {
        return JSON.parse(this.ctx.storage.sql.exec("SELECT source_json FROM avatar_source_state WHERE avatar_id=?",id).one().source_json);
      }
      testSnapshot() {
        const tables = ["accounts","sessions","avatars","avatar_source_state","avatar_balances","avatar_outfits","avatar_operations","avatar_control_claims","avatar_control_exports","avatar_control_leases","account_meta"];
        return Object.fromEntries(tables.map(name => [name,this.ctx.storage.sql.exec("SELECT * FROM "+name+" ORDER BY rowid").toArray()]));
      }
    }
    export class ProofCoordinator extends DurableObject {
      constructor(ctx,env) { super(ctx,env); ctx.storage.sql.exec("CREATE TABLE IF NOT EXISTS proofs(kind TEXT PRIMARY KEY,value TEXT NOT NULL)"); }
      install(save,detach) {
        this.ctx.storage.sql.exec("INSERT OR REPLACE INTO proofs(kind,value) VALUES('save',?)",canonical(save));
        this.ctx.storage.sql.exec("INSERT OR REPLACE INTO proofs(kind,value) VALUES('detach',?)",canonical(detach));
        return true;
      }
      clear() { this.ctx.storage.sql.exec("DELETE FROM proofs"); return true; }
      lookupCommittedAvatarSaveV1(ref) {
        const value = JSON.parse(this.ctx.storage.sql.exec("SELECT value FROM proofs WHERE kind='save'").one().value);
        if (!equal(saveReference(value),ref)) throw Error("test coordinator reference mismatch");
        return value;
      }
      lookupCommittedAvatarDetachV1(ref) {
        const value = JSON.parse(this.ctx.storage.sql.exec("SELECT value FROM proofs WHERE kind='detach'").one().value);
        if (!equal(value.identity,ref.identity)) throw Error("test coordinator reference mismatch");
        return value;
      }
    }
    const allowed = new Set(["initializeCatalog","initializeAdmissionAuthorityV2","register","login","logout","createAvatar","reserveAvatarControl","exportReservedAvatarV2","applyCommittedAvatarLotSaveV1","releaseSavedAvatarControlV1","lookupSavedAvatarControlV1","lookupReleasedAvatarControlV1","requestAvatarControlFenceV1","testSource","testSnapshot"]);
    export default { async fetch(request,env) {
      try {
        const {op,args=[],lot_id} = await request.json();
        if (op === "installProof" || op === "clearProof") {
          const coordinator = env.COORD.get(env.COORD.idFromName(lot_id));
          return Response.json(await coordinator[op === "installProof" ? "install" : "clear"](...args));
        }
        if (!allowed.has(op)) throw Error("unknown test operation");
        const accounts = env.ACCOUNTS.get(env.ACCOUNTS.idFromName("owned-leave-real-account"));
        return Response.json(await accounts[op](...args));
      } catch(e) { return Response.json({error:e.message},{status:400}); }
    }};
  `;
  const bundled = await build({
    stdin: {
      contents: worker,
      resolveDir: process.cwd(),
      sourcefile: "owned-leave-account-fixture.ts",
      loader: "ts",
    },
    bundle: true,
    format: "esm",
    platform: "neutral",
    external: ["cloudflare:workers"],
    write: false,
    plugins: [
      {
        name: "wasm",
        setup(b) {
          b.onResolve({ filter: /account_crypto_bg\.wasm$/ }, () => ({
            path: "./account_crypto_bg.wasm",
            external: true,
          }));
        },
      },
    ],
  });
  const wasm = await readFile("generated/account_crypto_bg.wasm");
  const directory = await mkdtemp(
    join(tmpdir(), "wonderland-owned-leave-workerd-"),
  );
  let mf;
  async function start() {
    mf = new Miniflare({
      modulesRoot: directory,
      modules: [
        {
          type: "ESModule",
          path: join(directory, "worker.mjs"),
          contents: bundled.outputFiles[0].text,
        },
        {
          type: "CompiledWasm",
          path: join(directory, "account_crypto_bg.wasm"),
          contents: wasm,
        },
      ],
      compatibilityDate: "2026-07-01",
      durableObjects: {
        ACCOUNTS: { className: "TestAccounts", useSQLite: true },
        COORD: { className: "ProofCoordinator", useSQLite: true },
      },
      durableObjectsPersist: join(directory, "do"),
    });
    await mf.ready;
  }
  async function call(op, args = []) {
    const response = await mf.dispatchFetch("https://test.invalid/", {
      method: "POST",
      body: JSON.stringify({ op, args, lot_id: identity.lot_id }),
    });
    const result = await response.json();
    if (!response.ok) throw Error(result.error);
    return result;
  }
  try {
    await start();
    await call("initializeCatalog", [
      {
        release: spec.release,
        entries: [
          {
            key: spec.head_key,
            part: "head",
            gender: "female",
            skins: ["dark"],
          },
          {
            key: spec.body_key,
            part: "body",
            gender: "female",
            skins: ["dark"],
          },
        ],
      },
    ]);
    await call("initializeAdmissionAuthorityV2", [identity.authority_id]);
    const phrase = "source-lot-save-account-fixture-password",
      network = h("e");
    await call("register", ["source_leave_owner", phrase, network]);
    const session = await call("login", [
      "source_leave_owner",
      phrase,
      network,
    ]);
    const created = await call("createAvatar", [
      session.token,
      "source_create_operation",
      spec,
    ]);
    const target = {
      avatar_id: created.avatar_id,
      lot_id: identity.lot_id,
      content_release: spec.release,
      protocol_release: origin.protocol_release,
    };
    const claim = await call("reserveAvatarControl", [
      session.token,
      identity.admission_operation,
      target,
    ]);
    const exported = await call("exportReservedAvatarV2", [
      session.token,
      identity.admission_operation,
      created.avatar_id,
      { revision: "1", sha256: h("a") },
    ]);
    const portable = JSON.parse(exported.portable_json),
      realIdentity = {
        ...identity,
        account_id: exported.account_id,
        avatar_id: exported.avatar_id,
        control_generation: exported.control_generation,
        session_ref: exported.session_ref,
        reservation_digest: exported.reservation_digest,
        export_digest: exported.export_digest,
      };
    // Only the session that owns this exact claim may request its fence.
    // Fencing must retain custody and all saved avatar data until native proof.
    const secondSession = await call("login", [
      "source_leave_owner",
      phrase,
      network,
    ]);
    await call("register", ["source_leave_other", phrase, network]);
    const otherSession = await call("login", [
      "source_leave_other",
      phrase,
      network,
    ]);
    const fenceRequest = {
      avatar_id: created.avatar_id,
      generation: claim.generation,
      admission_operation: identity.admission_operation,
      reservation_digest: exported.reservation_digest,
    };
    const beforeFence = await call("testSnapshot");
    for (const token of [secondSession.token, otherSession.token]) {
      await assert.rejects(
        call("requestAvatarControlFenceV1", [token, fenceRequest]),
        /owned departure session mismatch/,
      );
      assert.deepEqual(await call("testSnapshot"), beforeFence);
    }
    const fence = await call("requestAvatarControlFenceV1", [
      session.token,
      fenceRequest,
    ]);
    assert.equal(fence.claim.state, "fence_pending");
    assert.equal(fence.claim.generation, claim.generation);
    assert.equal(fence.operation, identity.admission_operation);
    const afterFence = await call("testSnapshot");
    assertUnchangedExcept(beforeFence, afterFence, ["avatar_control_claims"]);
    assert.deepEqual(
      await call("requestAvatarControlFenceV1", [session.token, fenceRequest]),
      fence,
    );
    assert.deepEqual(await call("testSnapshot"), afterFence);
    const changedPerson = [...portable.person_data];
    changedPerson[0] = -32768;
    changedPerson[10] = 255;
    changedPerson[11] = -199;
    const saveColumns = sourceLotSaveColumns({
      motive_data: [-32768, ...portable.motive_data.slice(1)],
      person_data: changedPerson,
      default_suits: portable.default_suits,
      body_for_persistence: "18446744073709551615",
    });
    const state = records();
    state.save.identity = realIdentity;
    state.save.avatar_columns = saveColumns;
    state.save.current_job_row = null;
    state.committed.identity = realIdentity;
    state.committed.save_sha256 = sha(state.save);
    state.committed.source_baseline = {
      progression: exported.source_revisions.progression,
      wardrobe: exported.source_revisions.wardrobe,
    };
    state.detach.identity = realIdentity;
    state.detach.source_save = saveReference(state.committed);
    await call("installProof", [state.committed, state.detach]);
    const before = await call("testSnapshot"),
      saved = await call("applyCommittedAvatarLotSaveV1", [
        saveReference(state.committed),
      ]),
      after = await call("testSnapshot");
    assertUnchangedExcept(before, after, [
      "avatar_source_state",
      "avatars",
      "avatar_control_claims",
    ]);
    assert.equal(saved.status, "applied_source_save");
    const activeRetry = await call("reserveAvatarControl", [
      session.token,
      identity.admission_operation,
      target,
    ]);
    assert.equal(activeRetry.generation, claim.generation);
    assert.equal(activeRetry.state, "fence_pending");
    await call("logout", [session.token, true]);
    const released = await call("releaseSavedAvatarControlV1", [
      detachReference(state.detach),
    ]);
    assert.equal(released.status, "released_saved_control");
    await mf.dispose();
    await start();
    const nextSession = await call("login", [
      "source_leave_owner",
      phrase,
      network,
    ]);
    await assert.rejects(
      call("reserveAvatarControl", [
        nextSession.token,
        identity.admission_operation,
        target,
      ]),
      /already terminal/,
    );
    const nextOperation = "source_reentry_operation",
      nextClaim = await call("reserveAvatarControl", [
        nextSession.token,
        nextOperation,
        target,
      ]);
    assert.ok(BigInt(nextClaim.generation) > BigInt(claim.generation));
    const beforeStaleFence = await call("testSnapshot");
    await assert.rejects(
      call("requestAvatarControlFenceV1", [nextSession.token, fenceRequest]),
      /owned departure admission mismatch/,
    );
    assert.deepEqual(await call("testSnapshot"), beforeStaleFence);
    const nextExport = await call("exportReservedAvatarV2", [
      nextSession.token,
      nextOperation,
      created.avatar_id,
      { revision: "1", sha256: h("a") },
    ]);
    const restored = JSON.parse(nextExport.portable_json);
    assert.equal(restored.person_data[0], -32768);
    assert.equal(restored.person_data[11], -100);
    assert.equal(
      restored.person_data[10],
      portable.person_data[10],
      "available skill locks still derive from age",
    );
    assert.equal(
      (await call("testSource", [created.avatar_id])).person_data[10],
      255,
      "saved byte remains in source row",
    );
    assert.equal(restored.motive_data[0], -32768);
    assert.equal(restored.motive_data[11], 0);
    assert.equal(restored.body, "18446744073709551615");
    assert.equal(restored.budget_mirror, "0");
    assert.deepEqual(
      restored.relationships,
      state.save.changed_relationships.filter((row) => row.values.length),
    );
    // Clear test coordinator proofs: exact old receipts remain permanent after
    // account restart/re-admission and do not read unavailable old native state.
    await call("clearProof");
    const beforeRetry = await call("testSnapshot");
    assert.deepEqual(
      await call("applyCommittedAvatarLotSaveV1", [
        saveReference(state.committed),
      ]),
      saved,
    );
    assert.deepEqual(
      await call("releaseSavedAvatarControlV1", [
        detachReference(state.detach),
      ]),
      released,
    );
    assert.deepEqual(await call("testSnapshot"), beforeRetry);
    assert.deepEqual(
      await call("lookupSavedAvatarControlV1", [
        {
          identity: realIdentity,
          departure_operation: state.save.departure_operation,
        },
      ]),
      saved,
    );
    assert.deepEqual(
      await call("lookupReleasedAvatarControlV1", [
        {
          identity: realIdentity,
          departure_operation: state.save.departure_operation,
        },
      ]),
      released,
    );
    console.log(
      "PASS actual workerd AccountAuthority: real registration/crypto/export, configured coordinator lookup, active retry, logout cleanup, restart, retained tombstone, newer re-entry with saved values and old retry isolation",
    );
  } finally {
    await mf?.dispose();
    await rm(directory, { recursive: true, force: true });
  }
}
if (process.argv.includes("--workerd")) await runActualAccountAuthority();
