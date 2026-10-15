// Lightweight, real-SQLite contract/fault tests. No workerd or Rust build.
import assert from "node:assert/strict";
import { test } from "node:test";
import { DatabaseSync } from "node:sqlite";
import { registerHooks } from "node:module";
import { createHash } from "node:crypto";
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (
      context.parentURL?.endsWith(".ts") &&
      /^\.\.?\//.test(specifier) &&
      !/\.[cm]?[jt]s$/.test(specifier)
    )
      specifier += ".ts";
    return nextResolve(specifier, context);
  },
});
const { HostReceiptStore } = await import("../src/lot/host-receipts.ts");
const { canonical, encode } = await import("../src/lot/contracts.ts");
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const clone = (value) => structuredClone(value);
const h = (n) => n.toString(16).padStart(64, "0");
const b = {
  protocol: 1,
  source_pin: "4c6b3e8f5835b228723caea3c9f683c62f244f73",
  binding_sha256: h(1),
  lot_incarnation: "1",
};
const cursor = (n) => ({
  lot_id: "7",
  authority_epoch: "1",
  completed_tick: String(n),
  state_hash: h(n + 10),
});
function storage() {
  const db = new DatabaseSync(":memory:");
  const sql = {
    exec(query, ...bindings) {
      let rows = [];
      if (!bindings.length && query.includes(";")) db.exec(query);
      else
        rows = db
          .prepare(query)
          .all(...bindings)
          .map((r) => ({ ...r }));
      return {
        toArray: () => rows,
        one: () => {
          assert.equal(rows.length, 1);
          return rows[0];
        },
      };
    },
  };
  return {
    sql,
    db,
    transactionSync(callback) {
      db.exec("BEGIN");
      try {
        const result = callback();
        db.exec("COMMIT");
        return result;
      } catch (error) {
        db.exec("ROLLBACK");
        throw error;
      }
    },
  };
}
class Bucket {
  objects = new Map();
  puts = 0;
  beforeGet;
  afterPut;
  async get(key) {
    await this.beforeGet?.(key);
    const bytes = this.objects.get(key);
    return !bytes
      ? null
      : {
          size: bytes.byteLength,
          arrayBuffer: async () => Uint8Array.from(bytes).buffer,
        };
  }
  async put(key, bytes) {
    this.puts++;
    this.objects.set(key, Uint8Array.from(bytes));
    await this.afterPut?.(key);
  }
}
function fixture({ archived = false, controlled = false } = {}) {
  const s = storage(),
    bucket = new Bucket();
  s.sql
    .exec(`CREATE TABLE batches(batch_key TEXT PRIMARY KEY,digest TEXT,receipt TEXT,bytes INTEGER);
    CREATE TABLE controlled_denials(batch_key TEXT PRIMARY KEY,digest TEXT,receipt TEXT);
    CREATE TABLE checkpoints(object_key TEXT PRIMARY KEY,manifest TEXT);
    CREATE TABLE journal_chunks(batch_key TEXT,ordinal INTEGER,payload TEXT,PRIMARY KEY(batch_key,ordinal));
    CREATE TABLE journal_archives(object_key TEXT PRIMARY KEY,sha256 TEXT,bytes INTEGER,checkpoint_key TEXT);
    CREATE TABLE archived_batches(batch_key TEXT PRIMARY KEY,object_key TEXT,byte_offset INTEGER,byte_length INTEGER);`);
  const store = new HostReceiptStore(s, bucket);
  const checkpoint = new TextEncoder().encode("immutable checkpoint"),
    checkpointHash = sha(checkpoint);
  const checkpointKey = `lots/7/1/2/${checkpointHash}`;
  bucket.objects.set(checkpointKey, checkpoint);
  const manifest = {
    schema_version: 1,
    binding: b,
    cursor: cursor(2),
    object_key: checkpointKey,
    sha256: checkpointHash,
    byte_length: checkpoint.byteLength,
  };
  s.sql.exec(
    "INSERT INTO checkpoints VALUES(?,?)",
    checkpointKey,
    canonical(manifest),
  );
  const records = [],
    payloads = [];
  for (let i = 1; i <= 2; i++) {
    const control = controlled
      ? {
          schema_version: 2,
          revision_before: "0",
          revision_after: "0",
          actor_registry_before_sha256: sha(encode([])),
          actor_registry_after_sha256: sha(encode([])),
          actors: [],
          permission_before: { revision: "0", sha256: h(3) },
          permission_after: { revision: "0", sha256: h(3) },
        }
      : null;
    const payload = {
      binding: b,
      batch_id: String(i),
      predecessor: cursor(i - 1),
      result: cursor(i),
      ticks_hex: "010203",
      effects_hex: "040506",
      events_hex: "070809",
      checkpoint_sha256: checkpointHash,
      ...(control ? { control_v2: control } : {}),
    };
    const bytes = encode(payload),
      digest = sha(bytes),
      receipt = {
        status: "committed",
        batch_id: String(i),
        digest,
        cursor: cursor(i),
        ...(controlled
          ? { control_revision: "0", actor_registry_sha256: sha(encode([])) }
          : {}),
      };
    s.sql.exec(
      "INSERT INTO batches VALUES(?,?,?,?)",
      `1:${i}`,
      digest,
      canonical(receipt),
      bytes.byteLength,
    );
    if (!archived)
      s.sql.exec(
        "INSERT INTO journal_chunks VALUES(?,?,?)",
        `1:${i}`,
        0,
        canonical(payload),
      );
    payloads.push(bytes);
    records.push({
      batch_id: String(i),
      input_sha256: h(100 + i),
      digest,
      reply: {
        kind: "committed",
        batch_id: String(i),
        digest,
        cursor: cursor(i),
        ticks_hex: payload.ticks_hex,
        effects_hex: payload.effects_hex,
        events_hex: payload.events_hex,
        control_v2: control,
        receipt_hex: "abcd012345",
      },
    });
  }
  if (archived) {
    const joined = Buffer.concat(payloads),
      journalKey = "journal/test-segment";
    bucket.objects.set(journalKey, joined);
    s.sql.exec(
      "INSERT INTO journal_archives VALUES(?,?,?,?)",
      journalKey,
      sha(joined),
      joined.byteLength,
      checkpointKey,
    );
    s.sql.exec(
      "INSERT INTO archived_batches VALUES(?,?,?,?)",
      "1:1",
      journalKey,
      0,
      payloads[0].byteLength,
    );
    s.sql.exec(
      "INSERT INTO archived_batches VALUES(?,?,?,?)",
      "1:2",
      journalKey,
      payloads[0].byteLength,
      payloads[1].byteLength,
    );
  }
  const denied = {
    status: "rejected",
    reason: "fence_drain_required",
    batch_id: "3",
    digest: h(999),
    control_revision: "1",
  };
  s.sql.exec(
    "INSERT INTO controlled_denials VALUES(?,?,?)",
    "1:3",
    denied.digest,
    canonical(denied),
  );
  records.push({
    batch_id: "3",
    input_sha256: h(103),
    digest: denied.digest,
    reply: {
      kind: "rejected",
      batch_id: "3",
      digest: denied.digest,
      cursor: cursor(2),
      receipt_hex: "ff012345",
    },
  });
  const envelope = {
    schema_version: 1,
    binding: b,
    cursor: cursor(2),
    checkpoint_sha256: checkpointHash,
    through_batch_id: "3",
    records,
  };
  return { s, bucket, store, envelope, checkpointKey, manifest, payloads };
}
const persist = (f, e = f.envelope) => f.store.persist(e, sha(encode(e)));
const count = (f) =>
  f.s.sql.exec("SELECT COUNT(*) AS n FROM host_receipts").one().n;

for (const archived of [false, true])
  for (const controlled of [false, true]) {
    test(`retains exact full committed/rejected replies, archived=${archived}, controlled=${controlled}`, async () => {
      const f = fixture({ archived, controlled });
      const ack = await persist(f);
      assert.deepEqual(ack, {
        schema_version: 1,
        status: "persisted",
        binding: b,
        cursor: cursor(2),
        checkpoint_sha256: f.envelope.checkpoint_sha256,
        through_batch_id: "3",
        receipt_set_sha256: sha(encode(f.envelope)),
      });
      assert.equal(count(f), 3);
      for (const record of f.envelope.records)
        assert.deepEqual(await f.store.lookup("1", record.batch_id), {
          status: "recorded",
          record,
        });
      assert.deepEqual(await f.store.lookup("2", "1"), { status: "unknown" });
      assert.deepEqual(await persist(f), ack);
      assert.equal(
        f.bucket.puts,
        1,
        "exact retry never rewrites immutable R2 object",
      );
      // Recovery/restart may have moved to another binding; no active binding is needed.
      f.s.sql.exec("CREATE TABLE lot_state(id INTEGER PRIMARY KEY,value TEXT)");
      f.s.sql.exec(
        "INSERT INTO lot_state VALUES(1,?)",
        canonical({
          binding: { ...b, lot_incarnation: "2" },
          cursor: cursor(9),
        }),
      );
      f.store = new HostReceiptStore(f.s, f.bucket);
      assert.deepEqual(await f.store.lookup("1", "1"), {
        status: "recorded",
        record: f.envelope.records[0],
      });
      assert.deepEqual(await persist(f), ack);
    });
  }

const invalid = [
  [
    "unknown envelope field",
    (e) => {
      e.extra = true;
    },
  ],
  [
    "schema version",
    (e) => {
      e.schema_version = 2;
    },
  ],
  [
    "empty set",
    (e) => {
      e.records = [];
    },
  ],
  [
    "more than 64 records",
    (e) => {
      e.records = Array(65).fill(e.records[0]);
    },
  ],
  [
    "numeric identity",
    (e) => {
      e.records[0].batch_id = 1;
    },
  ],
  [
    "u64 overflow",
    (e) => {
      e.through_batch_id = "18446744073709551616";
    },
  ],
  [
    "noncanonical decimal",
    (e) => {
      e.records[0].batch_id = "01";
    },
  ],
  [
    "zero batch",
    (e) => {
      e.records[0].batch_id = "0";
    },
  ],
  [
    "unknown record field",
    (e) => {
      e.records[0].other = 1;
    },
  ],
  [
    "missing original input hash",
    (e) => {
      delete e.records[0].input_sha256;
    },
  ],
  [
    "invalid original input hash",
    (e) => {
      e.records[0].input_sha256 = "a";
    },
  ],
  [
    "duplicate ids",
    (e) => {
      e.records[1] = clone(e.records[0]);
    },
  ],
  [
    "unsorted ids",
    (e) => {
      e.records.reverse();
    },
  ],
  [
    "through id not final",
    (e) => {
      e.through_batch_id = "4";
    },
  ],
  [
    "through id smaller",
    (e) => {
      e.through_batch_id = "2";
    },
  ],
  [
    "missing full reply",
    (e) => {
      delete e.records[0].reply.receipt_hex;
    },
  ],
  [
    "unknown reply field",
    (e) => {
      e.records[0].reply.extra = true;
    },
  ],
  [
    "wrong reply id",
    (e) => {
      e.records[0].reply.batch_id = "2";
    },
  ],
  [
    "wrong reply digest",
    (e) => {
      e.records[0].reply.digest = h(300);
    },
  ],
  [
    "nonterminal reply",
    (e) => {
      e.records[0].reply.kind = "proposal";
    },
  ],
  [
    "malformed action receipt",
    (e) => {
      e.records[0].reply.receipt_hex = "fff";
    },
  ],
  [
    "uppercase binary lane",
    (e) => {
      e.records[0].reply.ticks_hex = "AB";
    },
  ],
  [
    "future reply cursor",
    (e) => {
      e.records[0].reply.cursor = cursor(3);
    },
  ],
  [
    "different cursor epoch",
    (e) => {
      e.records[0].reply.cursor.authority_epoch = "2";
    },
  ],
  [
    "different cursor lot",
    (e) => {
      e.records[0].reply.cursor.lot_id = "8";
    },
  ],
  [
    "same-tick wrong state hash",
    (e) => {
      e.records[2].reply.cursor.state_hash = h(200);
    },
  ],
  [
    "wrong earlier committed cursor",
    (e) => {
      e.records[0].reply.cursor.state_hash = h(200);
    },
  ],
  [
    "wrong binding",
    (e) => {
      e.binding.binding_sha256 = h(200);
    },
  ],
  [
    "wrong checkpoint",
    (e) => {
      e.checkpoint_sha256 = h(200);
    },
  ],
  [
    "altered tick lane",
    (e) => {
      e.records[0].reply.ticks_hex = "ff";
    },
  ],
  [
    "altered effect lane",
    (e) => {
      e.records[0].reply.effects_hex = "ff";
    },
  ],
  [
    "altered event lane",
    (e) => {
      e.records[0].reply.events_hex = "ff";
    },
  ],
];
for (const [name, mutate] of invalid)
  test(`rejects ${name} without publishing replay rows`, async () => {
    const f = fixture(),
      e = clone(f.envelope);
    mutate(e);
    await assert.rejects(persist(f, e));
    assert.equal(count(f), 0);
    assert.equal(f.bucket.puts, 0);
  });

test("rejects envelope hash mismatch before object writes", async () => {
  const f = fixture();
  await assert.rejects(f.store.persist(f.envelope, h(500)), /digest mismatch/);
  assert.equal(count(f), 0);
  assert.equal(f.bucket.puts, 0);
});

test("rejects accepted control lane mismatch rather than trusting digest string", async () => {
  const f = fixture({ controlled: true });
  f.envelope.records[0].reply.control_v2.revision_after = "20";
  await assert.rejects(persist(f), /accepted payload/);
  assert.equal(count(f), 0);
});

test("requires a retained durable denial for every rejected reply", async () => {
  const f = fixture();
  f.s.sql.exec("DELETE FROM controlled_denials WHERE batch_key=?", "1:3");
  await assert.rejects(persist(f), /durable decision missing/);
  assert.equal(count(f), 0);
  assert.equal(f.bucket.puts, 0);
});

test("rejects corrupted durable decisions, missing checkpoint, wrong manifest and bytes", async () => {
  for (const damage of [
    (f) =>
      f.s.sql.exec(
        "UPDATE batches SET digest=? WHERE batch_key=?",
        h(700),
        "1:1",
      ),
    (f) =>
      f.s.sql.exec(
        "UPDATE batches SET receipt=? WHERE batch_key=?",
        canonical({ status: "rejected" }),
        "1:1",
      ),
    (f) =>
      f.s.sql.exec(
        "UPDATE controlled_denials SET digest=? WHERE batch_key=?",
        h(700),
        "1:3",
      ),
    (f) => f.s.sql.exec("DELETE FROM checkpoints"),
    (f) => {
      f.manifest.binding = { ...b, lot_incarnation: "2" };
      f.s.sql.exec("UPDATE checkpoints SET manifest=?", canonical(f.manifest));
    },
    (f) => {
      f.manifest.byte_length = 9 * 1024 * 1024;
      f.s.sql.exec("UPDATE checkpoints SET manifest=?", canonical(f.manifest));
    },
    (f) => f.bucket.objects.delete(f.checkpointKey),
    (f) => f.bucket.objects.set(f.checkpointKey, new Uint8Array(1)),
    (f) => f.bucket.objects.get(f.checkpointKey).fill(0),
  ]) {
    const f = fixture();
    damage(f);
    await assert.rejects(persist(f));
    assert.equal(count(f), 0);
    assert.equal(f.bucket.puts, 0);
  }
});

test("verifies both live and archived accepted payload bytes before trusting reply lanes", async () => {
  for (const damage of [
    (f) =>
      f.s.sql.exec(
        "UPDATE journal_chunks SET payload='{}' WHERE batch_key=?",
        "1:1",
      ),
    (f) => f.s.sql.exec("DELETE FROM journal_chunks WHERE batch_key=?", "1:1"),
    (f) =>
      f.s.sql.exec(
        "UPDATE journal_chunks SET ordinal=1 WHERE batch_key=?",
        "1:1",
      ),
    (f) =>
      f.s.sql.exec(
        "UPDATE journal_chunks SET payload=? WHERE batch_key=?",
        "x".repeat(256 * 1024 + 1),
        "1:1",
      ),
  ]) {
    const f = fixture();
    damage(f);
    await assert.rejects(persist(f));
    assert.equal(count(f), 0);
  }
  for (const damage of [
    (f) => f.bucket.objects.get("journal/test-segment").fill(0),
    (f) => f.bucket.objects.delete("journal/test-segment"),
    (f) =>
      f.s.sql.exec(
        "UPDATE archived_batches SET byte_offset=1 WHERE batch_key=?",
        "1:1",
      ),
    (f) =>
      f.s.sql.exec(
        "UPDATE archived_batches SET byte_length=-1 WHERE batch_key=?",
        "1:1",
      ),
    (f) =>
      f.s.sql.exec("UPDATE journal_archives SET bytes=?", 17 * 1024 * 1024),
  ]) {
    const f = fixture({ archived: true });
    damage(f);
    await assert.rejects(persist(f));
    assert.equal(count(f), 0);
  }
});

test("readback failure cannot acknowledge or retire data; exact retry recovers orphan object", async () => {
  const f = fixture();
  f.bucket.afterPut = () => {
    throw Error("lost R2 response");
  };
  await assert.rejects(persist(f), /lost R2 response/);
  assert.equal(count(f), 0);
  f.bucket.afterPut = undefined;
  await persist(f);
  assert.equal(count(f), 3);
  assert.equal(f.bucket.puts, 1);
});

test("missing or corrupted archive readback never publishes and never overwrites corrupt key", async () => {
  for (const damage of [
    (f, key) => f.bucket.objects.delete(key),
    (f, key) => f.bucket.objects.get(key).fill(0),
  ]) {
    const f = fixture();
    f.bucket.afterPut = (key) => damage(f, key);
    await assert.rejects(persist(f));
    assert.equal(count(f), 0);
  }
  const f = fixture();
  const key = `host-receipts/7/1/${sha(encode(f.envelope))}`;
  f.bucket.objects.set(key, new Uint8Array(encode(f.envelope).byteLength));
  await assert.rejects(persist(f), /immutable.*corrupted/);
  assert.equal(f.bucket.puts, 0);
  assert.equal(count(f), 0);
});

test("changed original input hash or action correlation cannot replace an existing batch", async () => {
  const f = fixture();
  await persist(f);
  for (const change of [
    (e) => {
      e.records[0].input_sha256 = h(444);
    },
    (e) => {
      e.records[0].reply.receipt_hex = "beef";
    },
  ]) {
    const e = clone(f.envelope);
    change(e);
    await assert.rejects(persist(f, e), /identity reused/);
    assert.deepEqual(await f.store.lookup("1", "1"), {
      status: "recorded",
      record: f.envelope.records[0],
    });
    assert.equal(f.bucket.puts, 1);
  }
});

test("transaction revalidates decisions and checkpoint after asynchronous R2 writes", async () => {
  for (const mutate of [
    (f) =>
      f.s.sql.exec(
        "UPDATE controlled_denials SET receipt=?",
        canonical({ status: "rejected" }),
      ),
    (f) => f.s.sql.exec("DELETE FROM checkpoints"),
  ]) {
    const f = fixture();
    f.bucket.afterPut = () => mutate(f);
    await assert.rejects(persist(f));
    assert.equal(count(f), 0);
    assert.equal(
      f.s.sql.exec("SELECT COUNT(*) AS n FROM host_receipt_archives").one().n,
      0,
    );
  }
});

test("concurrent different records for one key publish exactly one immutable result", async () => {
  const f = fixture(),
    other = clone(f.envelope);
  other.records[0].reply.receipt_hex = "ffff";
  let puts = 0,
    release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  f.bucket.afterPut = async () => {
    if (++puts === 2) release();
    await gate;
  };
  const results = await Promise.allSettled([persist(f), persist(f, other)]);
  assert.equal(results.filter((r) => r.status === "fulfilled").length, 1);
  assert.equal(count(f), 3);
  const winner = results[0].status === "fulfilled" ? f.envelope : other;
  assert.deepEqual(await f.store.lookup("1", "1"), {
    status: "recorded",
    record: winner.records[0],
  });
});

test("lookup fails closed for missing/corrupt archive and mismatched index or binding", async () => {
  for (const damage of [
    (f, key) => f.bucket.objects.delete(key),
    (f, key) => f.bucket.objects.get(key).fill(0),
    (f) =>
      f.s.sql.exec(
        "UPDATE host_receipts SET ordinal=99 WHERE batch_key=?",
        "1:1",
      ),
    (f) =>
      f.s.sql.exec(
        "UPDATE host_receipts SET ordinal=1 WHERE batch_key=?",
        "1:1",
      ),
    (f) =>
      f.s.sql.exec(
        "UPDATE host_receipts SET input_sha256=? WHERE batch_key=?",
        h(777),
        "1:1",
      ),
    (f) =>
      f.s.sql.exec(
        "UPDATE host_receipts SET digest=? WHERE batch_key=?",
        h(777),
        "1:1",
      ),
    (f) =>
      f.s.sql.exec(
        "UPDATE host_receipts SET record_sha256=? WHERE batch_key=?",
        h(777),
        "1:1",
      ),
    (f) =>
      f.s.sql.exec(
        "UPDATE host_receipt_archives SET binding_json=?",
        canonical({ ...b, lot_incarnation: "2" }),
      ),
    (f) =>
      f.s.sql.exec(
        "UPDATE host_receipt_archives SET cursor_json=?",
        canonical(cursor(9)),
      ),
    (f) =>
      f.s.sql.exec("UPDATE host_receipt_archives SET checkpoint_key='other'"),
    (f) =>
      f.s.sql.exec(
        "UPDATE host_receipt_archives SET bytes=?",
        17 * 1024 * 1024,
      ),
    (f) => f.s.sql.exec("DELETE FROM host_receipt_archives"),
  ]) {
    const f = fixture();
    await persist(f);
    const key = f.s.sql
      .exec("SELECT object_key FROM host_receipts LIMIT 1")
      .one().object_key;
    damage(f, key);
    await assert.rejects(f.store.lookup("1", "1"));
  }
});

test("large full action receipts live in R2 rather than SQLite cells", async () => {
  const f = fixture();
  f.envelope.records[0].reply.receipt_hex = "ab".repeat(2 * 1024 * 1024);
  await persist(f);
  assert.deepEqual(await f.store.lookup("1", "1"), {
    status: "recorded",
    record: f.envelope.records[0],
  });
  const cells = f.s.sql.exec("SELECT * FROM host_receipts").toArray();
  for (const row of cells)
    for (const value of Object.values(row))
      if (typeof value === "string") assert.ok(value.length < 256);
});

test("oversized envelope safely refuses persistence", async () => {
  const f = fixture();
  f.envelope.records[0].reply.receipt_hex = "ab".repeat(8 * 1024 * 1024);
  await assert.rejects(persist(f), /envelope size bound/);
  assert.equal(count(f), 0);
  assert.equal(f.bucket.puts, 0);
});

test("accepts the 64-record boundary and exact u64 batch identities", async () => {
  const f = fixture();
  f.s.sql.exec(
    "DELETE FROM batches; DELETE FROM controlled_denials; DELETE FROM journal_chunks;",
  );
  f.envelope.records = Array.from({ length: 64 }, (_, i) => {
    const id = i === 63 ? "18446744073709551615" : String(i + 1),
      digest = h(i + 500);
    f.s.sql.exec(
      "INSERT INTO controlled_denials VALUES(?,?,?)",
      `1:${id}`,
      digest,
      canonical({
        status: "rejected",
        batch_id: id,
        digest,
        reason: "lease_expired",
        control_revision: "1",
      }),
    );
    return {
      batch_id: id,
      input_sha256: h(i + 600),
      digest,
      reply: {
        kind: "rejected",
        batch_id: id,
        digest,
        cursor: cursor(2),
        receipt_hex: null,
      },
    };
  });
  f.envelope.through_batch_id = "18446744073709551615";
  await persist(f);
  assert.equal(count(f), 64);
  assert.deepEqual(await f.store.lookup("1", f.envelope.through_batch_id), {
    status: "recorded",
    record: f.envelope.records[63],
  });
});

test("overlapping pages keep the first exact batch-to-object binding forever", async () => {
  const f = fixture();
  const first = clone(f.envelope);
  first.records = first.records.slice(0, 2);
  first.through_batch_id = "2";
  await persist(f, first);
  const original = f.s.sql
    .exec("SELECT * FROM host_receipts WHERE batch_key=?", "1:1")
    .one();
  await persist(f);
  assert.equal(count(f), 3);
  assert.deepEqual(
    f.s.sql.exec("SELECT * FROM host_receipts WHERE batch_key=?", "1:1").one(),
    original,
  );
  assert.deepEqual(await f.store.lookup("1", "1"), {
    status: "recorded",
    record: first.records[0],
  });
  assert.deepEqual(await f.store.lookup("1", "3"), {
    status: "recorded",
    record: f.envelope.records[2],
  });
});
