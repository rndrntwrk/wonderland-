// Lightweight tests run the production coordinator against real SQLite. No workerd.
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
const { controlledProposal } = await import("../src/lot/owned.ts");
const { actionEvidence, decodeActionRequest } = await import(
  "../src/lot/action-outcomes.ts"
);
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const h = (n) => n.toString(16).padStart(64, "0");
const clone = (v) => structuredClone(v);
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
const control = {
  principal: "1",
  account_id: "9",
  avatar_id: "11",
  control_generation: "2",
  session_ref: h(8),
};
const actor = {
  ...control,
  actor: { object_id: 2, generation: 3 },
  admission_operation: "owned_control_operation_0",
  reservation_digest: h(9),
  export_digest: h(10),
};
const binding = {
  protocol: 1,
  source_pin: "4c6b3e8f5835b228723caea3c9f683c62f244f73",
  binding_sha256: h(12),
  lot_incarnation: "1",
};
const cursor = (tick) => ({
  lot_id: "7",
  authority_epoch: "1",
  completed_tick: String(tick),
  state_hash: h(tick + 30),
});
const permission = { revision: "1", sha256: h(33) };
function packet(magic, payload) {
  return Buffer.concat([
    Buffer.from(magic + "\r\n\x1a\n"),
    u64(payload.length),
    payload,
  ]);
}
function request({
  sequence = "1",
  principal = "1",
  slot = 2,
  generation = 3,
  variant = 1,
  action = 17,
} = {}) {
  const version = Buffer.concat([u32(slot), u32(generation), u64(4)]);
  const body =
    variant === 1
      ? [
          u32(1),
          u64(principal),
          u64(4),
          version,
          u64(2),
          u64(sequence),
          u64(action),
        ]
      : [
          u32(0),
          u64(principal),
          u64(4),
          version,
          version,
          u64(2),
          u64(sequence),
          u32(9),
          u32(0),
          Buffer.alloc(2),
        ];
  return packet("WLC1", Buffer.concat(body)).toString("hex");
}
function actionReceipt(hex, tick) {
  const bytes = Buffer.from(hex, "hex");
  return packet(
    "WLA1",
    Buffer.concat([
      u64(bytes.length),
      bytes,
      Buffer.from([tick === null ? 0 : 1]),
      ...(tick === null ? [] : [u64(tick)]),
    ]),
  ).toString("hex");
}
function proposed({
  id = "1",
  tick = 0,
  hex = request(),
  evidenceControl = control,
  sequence = "1",
  incarnation = "1",
  action = true,
} = {}) {
  const payload = {
    binding: { ...binding, lot_incarnation: incarnation },
    batch_id: id,
    predecessor: cursor(tick),
    result: cursor(tick + 1),
    ticks_hex: "010203",
    checkpoint_sha256: h(77),
    effects_hex: "0102",
    events_hex: "0304",
    control_v2: {
      schema_version: 2,
      revision_before: "1",
      revision_after: "1",
      actor_registry_before_sha256: sha(encode([actor])),
      actor_registry_after_sha256: sha(encode([actor])),
      actors: [clone(actor)],
      permission_before: permission,
      permission_after: permission,
    },
  };
  if (action) {
    const { account_id, ...nativeControl } = evidenceControl;
    payload.action_v1 = {
      schema_version: 1,
      control: clone(evidenceControl),
      input_sha256: sha(
        encode({
          kind: "prepare_player_action",
          batch_id: id,
          binding: payload.binding,
          predecessor: payload.predecessor,
          expected_control_revision: "1",
          control: nativeControl,
          action_hex: hex,
        }),
      ),
      request_hex: hex,
      request_sha256: sha(Buffer.from(hex, "hex")),
      command_sequence: sequence,
      accepted_receipt_hex: actionReceipt(hex, tick + 1),
      rejected_receipt_hex: actionReceipt(hex, null),
    };
  }
  return resign({ kind: "proposal", status: "pending", payload });
}
function resign(p) {
  p.digest = sha(encode(p.payload));
  return p;
}
function storage() {
  const db = new DatabaseSync(":memory:");
  let depth = 0;
  const result = {
    db,
    fault: null,
    sql: {
      exec(query, ...bindings) {
        result.fault?.(query, bindings);
        let rows = [];
        if (!bindings.length && query.includes(";")) db.exec(query);
        else
          rows = db
            .prepare(query)
            .all(...bindings)
            .map((r) => ({ ...r }));
        result.afterExec?.(query, bindings);
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
      const savepoint = `s${depth++}`;
      db.exec(`SAVEPOINT ${savepoint}`);
      try {
        const value = fn();
        db.exec(`RELEASE ${savepoint}`);
        return value;
      } catch (e) {
        db.exec(`ROLLBACK TO ${savepoint}; RELEASE ${savepoint}`);
        throw e;
      } finally {
        depth--;
      }
    },
  };
  return result;
}
function fixture() {
  const s = storage(),
    env = {
      CHECKPOINTS: {
        get() {
          throw Error("unexpected R2 dependency");
        },
        put() {
          throw Error("unexpected R2 dependency");
        },
      },
    };
  const lot = new LotCoordinator({ storage: s }, env);
  lot.now = () => 1000;
  lot.initializeControlledV2(binding, cursor(0), h(0), {
    authority_id: h(6),
    content_release: h(7),
    protocol_release: h(5),
    permission_binding: permission,
    actors: [],
  });
  s.sql.exec("UPDATE lot_control_state SET revision='1'");
  s.sql.exec(
    "UPDATE controlled_registry SET sha256=?,value=?",
    sha(encode([actor])),
    canonical([actor]),
  );
  const exp = {
    schema_version: 2,
    authority_id: h(6),
    account_id: control.account_id,
    avatar_id: control.avatar_id,
    lot_id: "7",
    control_generation: control.control_generation,
    admission_operation: actor.admission_operation,
    reservation_digest: actor.reservation_digest,
    session_ref: control.session_ref,
    account_revision: "1",
    content_release: h(7),
    protocol_release: h(5),
    expires_at: 2000,
    permission_binding: permission,
    permission_source: {
      registered_at_ms: 0,
      created_at_ms: 0,
      observed_at_ms: 0,
      roommate_lots: [],
      moderation_level: 0,
      mayor_nhood: null,
    },
    source_revisions: {
      owned_assets: 1,
      wardrobe: h(0),
      ledger: h(0),
      progression: h(0),
      permissions: h(0),
    },
    portable_json: "{}",
    portable_sha256: sha("{}"),
    export_digest: actor.export_digest,
  };
  s.sql.exec(
    "INSERT INTO actor_controls(control_key,avatar_id,principal,export_digest,export_json,state,expires_at,record) VALUES(?,?,?,?,?,'active',?,?)",
    "control",
    control.avatar_id,
    control.principal,
    exp.export_digest,
    canonical(exp),
    2000,
    "{}",
  );
  return {
    s,
    lot,
    env,
    reopen() {
      return new LotCoordinator({ storage: s }, env);
    },
  };
}
const query = (p) => ({
  schema_version: 1,
  lot_incarnation: p.payload.binding.lot_incarnation,
  control: p.payload.action_v1.control,
  request_hex: p.payload.action_v1.request_hex,
});
const counts = (f) =>
  Object.fromEntries(
    [
      "batches",
      "controlled_denials",
      "outbox",
      "player_action_outcomes",
      "journal_chunks",
    ].map((table) => [
      table,
      f.s.sql.exec(`SELECT COUNT(*) AS n FROM ${table}`).one().n,
    ]),
  );

test("action parser validates fixed Rust WLC1 and WLA1 ABI", () => {
  for (const variant of [0, 1]) {
    const p = proposed({ hex: request({ variant }) });
    assert.equal(
      decodeActionRequest(p.payload.action_v1.request_hex).command_sequence,
      "1",
    );
    assert.deepEqual(
      actionEvidence(p.payload.action_v1, "1"),
      p.payload.action_v1,
    );
  }
});
test("committed action outcome is durable without host resolve or receipt export", async () => {
  const f = fixture(),
    p = proposed();
  const decision = await f.lot.commitControlledBatchV2(p);
  assert.equal(decision.status, "committed");
  const recovered = await f.reopen().lookupPlayerActionV1(query(p));
  assert.deepEqual(recovered, {
    schema_version: 1,
    status: "committed",
    control,
    request_hex: p.payload.action_v1.request_hex,
    request_sha256: p.payload.action_v1.request_sha256,
    receipt_hex: p.payload.action_v1.accepted_receipt_hex,
    binding,
    batch_id: "1",
    digest: p.digest,
    cursor: cursor(1),
  });
  assert.deepEqual(await f.reopen().commitControlledBatchV2(p), decision);
  assert.deepEqual(counts(f), {
    batches: 1,
    controlled_denials: 0,
    outbox: 2,
    player_action_outcomes: 1,
    journal_chunks: 1,
  });
});
test("expiry rejection retains exact WLA1 and decision in the same transaction", async () => {
  const f = fixture(),
    p = proposed();
  f.lot.now = () => 2000;
  const denied = await f.lot.commitControlledBatchV2(p);
  assert.equal(denied.status, "rejected");
  const recovered = await f.reopen().lookupPlayerActionV1(query(p));
  assert.equal(recovered.status, "rejected");
  assert.equal(recovered.receipt_hex, p.payload.action_v1.rejected_receipt_hex);
  assert.deepEqual(recovered.cursor, cursor(0));
  assert.deepEqual(await f.reopen().commitControlledBatchV2(p), denied);
  assert.deepEqual(counts(f), {
    batches: 0,
    controlled_denials: 1,
    outbox: 0,
    player_action_outcomes: 1,
    journal_chunks: 0,
  });
});

for (const status of ["committed", "rejected"])
  for (const stage of ["before", "after"])
    test(`${status} action INSERT failure (${stage}) rolls back decision, cursor, outbox and fences`, async () => {
      const f = fixture(),
        p = proposed(),
        before = counts(f),
        initial = f.lot.inspect();
      if (status === "rejected") f.lot.now = () => 2000;
      f.s[stage === "before" ? "fault" : "afterExec"] = (sql) => {
        if (sql.startsWith("INSERT INTO player_action_outcomes"))
          throw Error("injected action storage fault");
      };
      await assert.rejects(
        f.lot.commitControlledBatchV2(p),
        /injected action storage fault/,
      );
      f.s.fault = f.s.afterExec = null;
      assert.deepEqual(counts(f), before);
      assert.deepEqual(f.lot.inspect(), initial);
      assert.equal(
        f.s.sql.exec("SELECT revision FROM lot_control_state").one().revision,
        "1",
      );
      assert.equal(
        f.s.sql.exec("SELECT state FROM actor_controls").one().state,
        "active",
      );
      assert.equal(
        f.s.sql.exec("SELECT COUNT(*) AS n FROM control_fences").one().n,
        0,
      );
      assert.deepEqual(await f.reopen().lookupPlayerActionV1(query(p)), {
        schema_version: 1,
        status: "unknown",
      });
      assert.equal((await f.lot.commitControlledBatchV2(p)).status, status);
    });

test("fence rejection atomically records the original request and exact rejection receipt", async () => {
  const f = fixture(),
    p = proposed();
  f.lot.recordControlFence({
    version: 1,
    authority: h(6),
    avatar_id: control.avatar_id,
    generation: control.control_generation,
    lot_id: "7",
    request_digest: actor.reservation_digest,
  });
  const decision = await f.lot.commitControlledBatchV2(p);
  assert.equal(decision.reason, "fence_drain_required");
  assert.equal(
    (await f.reopen().lookupPlayerActionV1(query(p))).receipt_hex,
    p.payload.action_v1.rejected_receipt_hex,
  );
  assert.deepEqual(counts(f), {
    batches: 0,
    controlled_denials: 1,
    outbox: 0,
    player_action_outcomes: 1,
    journal_chunks: 0,
  });
});

test("exact action request cannot create a second cross-batch decision or outbox", async () => {
  const f = fixture(),
    p = proposed();
  await f.lot.commitControlledBatchV2(p);
  const before = counts(f);
  await assert.rejects(
    f.lot.commitControlledBatchV2(proposed({ id: "2", tick: 1 })),
    /already has a durable outcome/,
  );
  assert.deepEqual(counts(f), before);
  assert.deepEqual(f.lot.inspect().cursor, cursor(1));
  assert.equal((await f.lot.lookupPlayerActionV1(query(p))).batch_id, "1");
});

test("rejected same sequence with different request bytes is a distinct request", async () => {
  const f = fixture(),
    p = proposed();
  f.lot.now = () => 2000;
  await f.lot.commitControlledBatchV2(p);
  const changed = proposed({ id: "2", hex: request({ action: 18 }) });
  assert.deepEqual(await f.lot.lookupPlayerActionV1(query(changed)), {
    schema_version: 1,
    status: "unknown",
  });
  await f.lot.commitControlledBatchV2(changed);
  assert.equal(
    (await f.lot.lookupPlayerActionV1(query(changed))).batch_id,
    "2",
  );
  assert.equal((await f.lot.lookupPlayerActionV1(query(p))).batch_id, "1");
  assert.equal(counts(f).player_action_outcomes, 2);
});

test("changed payload under an existing batch fails without replacing exact decision", async () => {
  const f = fixture(),
    p = proposed();
  await f.lot.commitControlledBatchV2(p);
  for (const change of [
    (p) => {
      p.payload.effects_hex = "abcdef";
    },
    (p) => {
      p.payload.action_v1.input_sha256 = h(999);
    },
    (p) => {
      p.payload.action_v1.control.account_id = "10";
    },
    (p) => {
      p.payload.action_v1.command_sequence = "2";
    },
  ]) {
    const changed = clone(p);
    change(changed);
    resign(changed);
    await assert.rejects(f.lot.commitControlledBatchV2(changed));
  }
  assert.equal(
    (await f.reopen().lookupPlayerActionV1(query(p))).digest,
    p.digest,
  );
  assert.equal(counts(f).player_action_outcomes, 1);
});

const invalidEvidence = [
  [
    "unsupported schema",
    (p) => {
      p.payload.action_v1.schema_version = 2;
    },
  ],
  [
    "unknown evidence field",
    (p) => {
      p.payload.action_v1.extra = true;
    },
  ],
  [
    "explicit null evidence",
    (p) => {
      p.payload.action_v1 = null;
    },
  ],
  [
    "missing evidence field",
    (p) => {
      delete p.payload.action_v1.input_sha256;
    },
  ],
  [
    "numeric identity",
    (p) => {
      p.payload.action_v1.control.account_id = 9;
    },
  ],
  [
    "u64 overflow",
    (p) => {
      p.payload.action_v1.command_sequence = "18446744073709551616";
    },
  ],
  [
    "mismatched principal",
    (p) => {
      p.payload.action_v1.control.principal = "2";
    },
  ],
  [
    "mismatched sequence",
    (p) => {
      p.payload.action_v1.command_sequence = "2";
    },
  ],
  [
    "wrong request digest",
    (p) => {
      p.payload.action_v1.request_sha256 = h(99);
    },
  ],
  [
    "wrong input digest",
    (p) => {
      p.payload.action_v1.input_sha256 = h(99);
    },
  ],
  [
    "upper-case request",
    (p) => {
      p.payload.action_v1.request_hex =
        p.payload.action_v1.request_hex.toUpperCase();
    },
  ],
  [
    "oversize request",
    (p) => {
      p.payload.action_v1.request_hex = "ff".repeat(16385);
    },
  ],
  [
    "wrong request header",
    (p) => {
      p.payload.action_v1.request_hex =
        "00" + p.payload.action_v1.request_hex.slice(2);
    },
  ],
  [
    "wrong request length",
    (p) => {
      p.payload.action_v1.request_hex += "00";
    },
  ],
  [
    "unsupported action variant",
    (p) => {
      const bytes = Buffer.from(p.payload.action_v1.request_hex, "hex");
      bytes.writeUInt32LE(2, 16);
      p.payload.action_v1.request_hex = bytes.toString("hex");
    },
  ],
  [
    "accepted receipt wrong tick",
    (p) => {
      p.payload.action_v1.accepted_receipt_hex = actionReceipt(
        p.payload.action_v1.request_hex,
        2,
      );
    },
  ],
  [
    "accepted receipt wrong request",
    (p) => {
      p.payload.action_v1.accepted_receipt_hex = actionReceipt(
        request({ sequence: "2" }),
        1,
      );
    },
  ],
  [
    "accepted receipt rejected option",
    (p) => {
      p.payload.action_v1.accepted_receipt_hex = actionReceipt(
        p.payload.action_v1.request_hex,
        null,
      );
    },
  ],
  [
    "rejected receipt accepted option",
    (p) => {
      p.payload.action_v1.rejected_receipt_hex = actionReceipt(
        p.payload.action_v1.request_hex,
        1,
      );
    },
  ],
  [
    "receipt trailing bytes",
    (p) => {
      p.payload.action_v1.rejected_receipt_hex += "00";
    },
  ],
  [
    "receipt wrong vector length",
    (p) => {
      const bytes = Buffer.from(
        p.payload.action_v1.accepted_receipt_hex,
        "hex",
      );
      bytes.writeBigUInt64LE(1n, 16);
      p.payload.action_v1.accepted_receipt_hex = bytes.toString("hex");
    },
  ],
  [
    "receipt malformed option",
    (p) => {
      const bytes = Buffer.from(
        p.payload.action_v1.rejected_receipt_hex,
        "hex",
      );
      bytes[bytes.length - 1] = 2;
      p.payload.action_v1.rejected_receipt_hex = bytes.toString("hex");
    },
  ],
];
for (const [name, mutate] of invalidEvidence)
  test(`rejects ${name} before any durable decision`, async () => {
    const f = fixture(),
      p = proposed(),
      before = counts(f);
    mutate(p);
    resign(p);
    await assert.rejects(f.lot.commitControlledBatchV2(p));
    assert.deepEqual(counts(f), before);
  });

for (const field of [
  "principal",
  "account_id",
  "avatar_id",
  "control_generation",
  "session_ref",
])
  test(`proposal/lookup refuses changed authenticated ${field}`, async () => {
    const f = fixture(),
      p = proposed(),
      changed = {
        ...control,
        [field]: field === "session_ref" ? h(999) : "90",
      };
    const other = proposed({
      evidenceControl: changed,
      hex: field === "principal" ? request({ principal: "90" }) : request(),
    });
    await assert.rejects(
      f.lot.commitControlledBatchV2(other),
      /installation|control|actor/,
    );
    await f.lot.commitControlledBatchV2(p);
    await assert.rejects(
      f.lot.lookupPlayerActionV1({ ...query(p), control: changed }),
      /installation|control/,
    );
    assert.equal(counts(f).player_action_outcomes, 1);
  });

test("action cannot substitute an actor or add actors in its controlled transition", async () => {
  for (const p of [
    proposed({ hex: request({ slot: 3 }) }),
    proposed({ hex: request({ generation: 4 }) }),
    (() => {
      const p = proposed();
      p.payload.control_v2.actors.push({
        ...actor,
        principal: "2",
        avatar_id: "12",
        actor: { object_id: 3, generation: 1 },
      });
      p.payload.control_v2.actor_registry_after_sha256 = sha(
        encode(p.payload.control_v2.actors),
      );
      return resign(p);
    })(),
  ]) {
    const f = fixture();
    await assert.rejects(
      f.lot.commitControlledBatchV2(p),
      /registry or actor mismatch/,
    );
    assert.equal(counts(f).batches, 0);
  }
});

test("action metadata retains maximum u64 sequence without numeric rounding", async () => {
  const f = fixture(),
    sequence = "18446744073709551615",
    p = proposed({ sequence, hex: request({ sequence }) });
  await f.lot.commitControlledBatchV2(p);
  assert.equal(
    decodeActionRequest(
      (await f.lot.lookupPlayerActionV1(query(p))).request_hex,
    ).command_sequence,
    sequence,
  );
});

test("legacy controlled proposals remain accepted, recoverable and action-unknown", async () => {
  const f = fixture(),
    p = proposed({ action: false });
  assert.deepEqual(controlledProposal(p), p);
  const decision = await f.lot.commitControlledBatchV2(p);
  assert.deepEqual(await f.reopen().commitControlledBatchV2(p), decision);
  assert.deepEqual(await f.reopen().readBatch("1", "1"), p.payload);
  assert.deepEqual(await f.reopen().lookupPlayerActionV1(query(proposed())), {
    schema_version: 1,
    status: "unknown",
  });
});

test("lookup is read-only while expired/fenced/recovering and after a successor incarnation", async () => {
  const f = fixture(),
    p = proposed();
  await f.lot.commitControlledBatchV2(p);
  const expected = await f.lot.lookupPlayerActionV1(query(p)),
    { grant } = f.lot.beginControlledRecoveryV2("1");
  f.lot.now = () => 9000;
  const revision = f.s.sql
    .exec("SELECT revision FROM lot_control_state")
    .one().revision;
  const readonly = (sql) => {
    if (!/^SELECT\b/i.test(sql)) throw Error("lookup attempted a mutation");
  };
  f.s.fault = readonly;
  assert.deepEqual(await f.lot.lookupPlayerActionV1(query(p)), expected);
  assert.deepEqual(
    await f.lot.lookupPlayerActionV1({ ...query(p), lot_incarnation: "2" }),
    { schema_version: 1, status: "unknown" },
  );
  f.s.fault = null;
  // Synthetic successor boundary; the separate native process test qualifies the real takeover.
  f.s.sql.exec(
    "UPDATE lot_state SET value=?",
    canonical({
      binding: grant.new_binding,
      cursor: { ...cursor(1), authority_epoch: grant.new_authority_epoch },
      checkpoint_sha256: h(88),
    }),
  );
  f.s.sql.exec("UPDATE recoveries SET status='completed'");
  const reopened = f.reopen();
  f.s.fault = readonly;
  assert.deepEqual(await reopened.lookupPlayerActionV1(query(p)), expected);
  f.s.fault = null;
  assert.equal(
    f.s.sql.exec("SELECT revision FROM lot_control_state").one().revision,
    revision,
  );
  assert.equal(
    f.s.sql.exec("SELECT state FROM actor_controls").one().state,
    "active",
  );
});

for (const status of ["committed", "rejected"])
  for (const [name, mutate] of [
    [
      "selected status and syntactically valid WLA1",
      (r, p) => {
        r.status = r.status === "committed" ? "rejected" : "committed";
        r.receipt_hex = actionReceipt(
          p.payload.action_v1.request_hex,
          r.status === "rejected" ? null : r.cursor.completed_tick,
        );
      },
    ],
    [
      "digest",
      (r) => {
        r.digest = h(900);
      },
    ],
    [
      "batch",
      (r) => {
        r.batch_id = "2";
      },
    ],
    [
      "cursor state hash",
      (r) => {
        r.cursor.state_hash = h(900);
      },
    ],
    [
      "tick and matching WLA1",
      (r, p) => {
        r.cursor.completed_tick = "42";
        if (r.status === "committed")
          r.receipt_hex = actionReceipt(p.payload.action_v1.request_hex, 42);
      },
    ],
    [
      "authority epoch",
      (r) => {
        r.cursor.authority_epoch = "2";
      },
    ],
    [
      "source binding hash",
      (r) => {
        r.binding.binding_sha256 = h(900);
      },
    ],
  ])
    test(`${status} lookup refuses corrupted ${name}`, async () => {
      const f = fixture(),
        p = proposed();
      if (status === "rejected") f.lot.now = () => 2000;
      await f.lot.commitControlledBatchV2(p);
      const row = f.s.sql
          .exec("SELECT outcome_json FROM player_action_outcomes")
          .one(),
        r = JSON.parse(row.outcome_json);
      mutate(r, p);
      f.s.sql.exec(
        "UPDATE player_action_outcomes SET outcome_json=?",
        canonical(r),
      );
      await assert.rejects(
        f.reopen().lookupPlayerActionV1(query(p)),
        /durable decision|durable identity/,
      );
    });

for (const status of ["committed", "rejected"])
  for (const [name, mutate] of [
    ["missing decision", (f, table) => f.s.sql.exec(`DELETE FROM ${table}`)],
    [
      "decision digest",
      (f, table) => f.s.sql.exec(`UPDATE ${table} SET digest=?`, h(900)),
    ],
    [
      "decision body",
      (f, table) => f.s.sql.exec(`UPDATE ${table} SET receipt='{}'`),
    ],
    [
      "outcome anchor",
      (f, table) => {
        const r = JSON.parse(
          f.s.sql.exec(`SELECT receipt FROM ${table}`).one().receipt,
        );
        r.action_outcome_sha256 = h(900);
        f.s.sql.exec(`UPDATE ${table} SET receipt=?`, canonical(r));
      },
    ],
    [
      "original input hash",
      (f) =>
        f.s.sql.exec(
          "UPDATE player_action_outcomes SET input_sha256=?",
          h(900),
        ),
    ],
  ])
    test(`${status} lookup refuses changed ${name}`, async () => {
      const f = fixture(),
        p = proposed();
      if (status === "rejected") f.lot.now = () => 2000;
      await f.lot.commitControlledBatchV2(p);
      mutate(f, status === "committed" ? "batches" : "controlled_denials");
      await assert.rejects(
        f.reopen().lookupPlayerActionV1(query(p)),
        /durable decision/,
      );
    });

for (const status of ["committed", "rejected"])
  test(`${status} exact retry refuses a changed durable decision body`, async () => {
    const f = fixture(),
      p = proposed();
    if (status === "rejected") f.lot.now = () => 2000;
    await f.lot.commitControlledBatchV2(p);
    const table = status === "committed" ? "batches" : "controlled_denials";
    const receipt = JSON.parse(
      f.s.sql.exec(`SELECT receipt FROM ${table}`).one().receipt,
    );
    receipt.status = status === "committed" ? "rejected" : "committed";
    f.s.sql.exec(`UPDATE ${table} SET receipt=?`, canonical(receipt));
    await assert.rejects(
      f.lot.commitControlledBatchV2(p),
      /durable decision mismatch/,
    );
  });

test("append-only SourceGoto WLC1 preserves exact authenticated actor and sequence", () => {
  const position = Buffer.alloc(9);
  position.writeInt16LE(24, 0);
  position.writeInt16LE(40, 2);
  position.writeInt8(1, 4);
  position.writeUInt16LE(7, 5);
  position.writeInt16LE(-2, 7);
  const bytes = packet(
    "WLC1",
    Buffer.concat([
      u32(2),
      u64(1),
      u32(2),
      u32(3),
      u64(4),
      u64(5),
      u64(6),
      u64(7),
      position,
    ]),
  );
  assert.equal(bytes.length, 77);
  // Same literal is encoded/decoded by the Rust native_player differential test.
  assert.equal(
    bytes.toString("hex"),
    "574c43310d0a1a0a3d000000000000000200000001000000000000000200000003000000040000000000000005000000000000000600000000000000070000000000000018002800010700feff",
  );
  const parsed = decodeActionRequest(bytes.toString("hex"));
  assert.equal(parsed.principal, "1");
  assert.equal(parsed.command_sequence, "7");
  assert.deepEqual(parsed.actor, { object_id: 2, generation: 3 });
  assert.equal(
    Buffer.from(parsed.bytes).toString("hex"),
    bytes.toString("hex"),
  );
  for (const corrupted of [
    Buffer.concat([bytes, Buffer.from([0])]),
    bytes.subarray(0, -1),
  ]) {
    assert.throws(() => decodeActionRequest(corrupted.toString("hex")));
  }
  const unsupported = Buffer.from(bytes);
  unsupported.writeUInt32LE(3, 16);
  assert.throws(() => decodeActionRequest(unsupported.toString("hex")));
});

for (const status of ["committed", "rejected"]) {
  test(`SourceGoto ${status} decision retains exact bytes across durable lookup without replay`, async () => {
    const f = fixture();
    const hex =
      "574c43310d0a1a0a3d000000000000000200000001000000000000000200000003000000040000000000000005000000000000000600000000000000070000000000000018002800010700feff";
    const p = proposed({ hex, sequence: "7" });
    if (status === "rejected")
      f.lot.recordControlFence({
        version: 1,
        authority: h(6),
        avatar_id: control.avatar_id,
        generation: control.control_generation,
        lot_id: "7",
        request_digest: actor.reservation_digest,
      });
    const decision = await f.lot.commitControlledBatchV2(p);
    assert.equal(decision.status, status);
    const before = counts(f);
    const restored = f.reopen();
    const outcome = await restored.lookupPlayerActionV1(query(p));
    assert.equal(outcome.status, status);
    assert.equal(outcome.request_hex, hex);
    assert.equal(
      outcome.receipt_hex,
      p.payload.action_v1[
        status === "committed" ? "accepted_receipt_hex" : "rejected_receipt_hex"
      ],
    );
    assert.deepEqual(await restored.commitControlledBatchV2(p), decision);
    assert.deepEqual(counts(f), before);
  });
}
