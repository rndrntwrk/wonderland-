import {
  binding,
  canonical,
  cursor,
  decimal,
  digest,
  encode,
  equal,
  fields,
  hash,
  proposal,
  type Binding,
  type Cursor,
} from "./contracts";
import { ActionOutcomeStore, verifyActionEvidence } from "./action-outcomes";
import { actorRegistry, controlledProposal } from "./owned";
import { permissionBinding } from "../account/portable";
import {
  departureEvidenceV1,
  verifyDepartureEvidence,
  type DepartureEvidenceV1,
} from "./departures";

const LIMIT = 16 * 1024 * 1024;
const CHECKPOINT_LIMIT = 8 * 1024 * 1024;
const CHUNK = 256 * 1024;
type Control = ReturnType<typeof controlledProposal>["payload"]["control_v2"];
interface TerminalBase {
  batch_id: string;
  digest: string;
  cursor: Cursor;
  receipt_hex: string | null;
}
export type HostTerminalReply =
  | (TerminalBase & {
      kind: "committed";
      ticks_hex: string;
      effects_hex: string;
      events_hex: string;
      control_v2: Control | null;
      departure_v1?: DepartureEvidenceV1;
    })
  | (TerminalBase & { kind: "rejected" });
export interface HostReceiptRecord {
  batch_id: string;
  input_sha256: string;
  digest: string;
  reply: HostTerminalReply;
}
export interface HostReceiptEnvelope {
  schema_version: 1;
  binding: Binding;
  cursor: Cursor;
  checkpoint_sha256: string;
  through_batch_id: string;
  records: HostReceiptRecord[];
}
export interface HostReceiptPersistAck {
  schema_version: 1;
  status: "persisted";
  binding: Binding;
  cursor: Cursor;
  checkpoint_sha256: string;
  through_batch_id: string;
  receipt_set_sha256: string;
}
export type HostReceiptLookup =
  | { status: "recorded"; record: HostReceiptRecord }
  | { status: "unknown" };
type DecisionRow = {
  digest: string;
  receipt: string;
};
type ReceiptIndex = {
  object_key: string;
  ordinal: number;
  input_sha256: string;
  digest: string;
  record_sha256: string;
};
type Archive = {
  object_key: string;
  sha256: string;
  bytes: number;
  checkpoint_key: string;
  binding_json: string;
  cursor_json: string;
  checkpoint_sha256: string;
  through_batch_id: string;
};
type AcceptedArchiveRead = (
  objectKey: string,
  sha256: string,
  bytes: number,
) => Promise<Uint8Array>;

function nonzero(raw: unknown): string {
  const value = decimal(raw);
  if (value === "0") throw Error("zero host receipt identity");
  return value;
}
function hex(raw: unknown, max: number): string {
  if (
    typeof raw !== "string" ||
    raw.length > 2 * max ||
    raw.length % 2 !== 0 ||
    !/^[0-9a-f]*$/.test(raw)
  )
    throw Error("invalid host receipt binary payload");
  return raw;
}
function control(raw: unknown): Control | null {
  if (raw === null) return null;
  const v = fields(raw, [
    "schema_version",
    "revision_before",
    "revision_after",
    "actor_registry_before_sha256",
    "actor_registry_after_sha256",
    "actors",
    "permission_before",
    "permission_after",
  ]);
  if (v.schema_version !== 2)
    throw Error("invalid host receipt control version");
  return {
    schema_version: 2,
    revision_before: decimal(v.revision_before),
    revision_after: decimal(v.revision_after),
    actor_registry_before_sha256: hash(v.actor_registry_before_sha256),
    actor_registry_after_sha256: hash(v.actor_registry_after_sha256),
    actors: actorRegistry(v.actors),
    permission_before: permissionBinding(v.permission_before),
    permission_after: permissionBinding(v.permission_after),
  };
}
function terminal(raw: unknown): HostTerminalReply {
  if (!raw || typeof raw !== "object" || Array.isArray(raw))
    throw Error("invalid host terminal reply");
  const kind = (raw as Record<string, unknown>).kind;
  if (kind !== "committed" && kind !== "rejected")
    throw Error("host receipt is not terminal");
  const v = fields(raw, [
    "kind",
    "batch_id",
    "digest",
    "cursor",
    "receipt_hex",
    ...(kind === "committed"
      ? [
          "ticks_hex",
          "effects_hex",
          "events_hex",
          "control_v2",
          ...(Object.hasOwn(raw, "departure_v1") ? ["departure_v1"] : []),
        ]
      : []),
  ]);
  const base: TerminalBase = {
    batch_id: nonzero(v.batch_id),
    digest: hash(v.digest),
    cursor: cursor(v.cursor),
    receipt_hex: v.receipt_hex === null ? null : hex(v.receipt_hex, LIMIT),
  };
  return kind === "rejected"
    ? { kind, ...base }
    : {
        kind,
        ...base,
        ticks_hex: hex(v.ticks_hex, 8 * 1024 * 1024),
        effects_hex: hex(v.effects_hex, 4 * 1024 * 1024),
        events_hex: hex(v.events_hex, 4 * 1024 * 1024),
        control_v2: control(v.control_v2),
        ...(Object.hasOwn(v, "departure_v1")
          ? { departure_v1: departureEvidenceV1(v.departure_v1) }
          : {}),
      };
}
function envelope(raw: unknown): HostReceiptEnvelope {
  const v = fields(raw, [
    "schema_version",
    "binding",
    "cursor",
    "checkpoint_sha256",
    "through_batch_id",
    "records",
  ]);
  if (
    v.schema_version !== 1 ||
    !Array.isArray(v.records) ||
    v.records.length < 1 ||
    v.records.length > 64
  )
    throw Error("host receipt envelope version or record bound");
  const b = binding(v.binding),
    c = cursor(v.cursor),
    through = nonzero(v.through_batch_id);
  let previous = 0n;
  const records = Array.from(v.records, (raw): HostReceiptRecord => {
    const r = fields(raw, ["batch_id", "input_sha256", "digest", "reply"]);
    const record = {
      batch_id: nonzero(r.batch_id),
      input_sha256: hash(r.input_sha256),
      digest: hash(r.digest),
      reply: terminal(r.reply),
    };
    const id = BigInt(record.batch_id),
      rc = record.reply.cursor;
    if (
      id <= previous ||
      id > BigInt(through) ||
      record.reply.batch_id !== record.batch_id ||
      record.reply.digest !== record.digest ||
      rc.lot_id !== c.lot_id ||
      rc.authority_epoch !== c.authority_epoch ||
      BigInt(rc.completed_tick) > BigInt(c.completed_tick) ||
      (rc.completed_tick === c.completed_tick && !equal(rc, c))
    )
      throw Error("host receipt record identity, order or cursor mismatch");
    previous = id;
    return record;
  });
  if (records[records.length - 1].batch_id !== through)
    throw Error("host receipt through identity mismatch");
  return {
    schema_version: 1,
    binding: b,
    cursor: c,
    checkpoint_sha256: hash(v.checkpoint_sha256),
    through_batch_id: through,
    records,
  };
}
function checkpointKey(e: HostReceiptEnvelope): string {
  return `lots/${e.cursor.lot_id}/${e.cursor.authority_epoch}/${e.cursor.completed_tick}/${e.checkpoint_sha256}`;
}
function objectKey(e: HostReceiptEnvelope, sha: string): string {
  return `host-receipts/${e.cursor.lot_id}/${e.binding.lot_incarnation}/${sha}`;
}

/** Private exact-replay archive. Only bounded metadata is kept in SQLite. */
export class HostReceiptStore {
  private storage: DurableObjectStorage;
  private bucket: R2Bucket;
  private actionOutcomes: ActionOutcomeStore;
  constructor(storage: DurableObjectStorage, bucket: R2Bucket) {
    this.storage = storage;
    this.bucket = bucket;
    this.actionOutcomes = new ActionOutcomeStore(storage);
    storage.sql.exec(
      `CREATE TABLE IF NOT EXISTS host_receipt_archives(object_key TEXT PRIMARY KEY,sha256 TEXT NOT NULL,bytes INTEGER NOT NULL,checkpoint_key TEXT NOT NULL,binding_json TEXT NOT NULL,cursor_json TEXT NOT NULL,checkpoint_sha256 TEXT NOT NULL,through_batch_id TEXT NOT NULL);CREATE TABLE IF NOT EXISTS host_receipts(batch_key TEXT PRIMARY KEY,object_key TEXT NOT NULL,ordinal INTEGER NOT NULL,input_sha256 TEXT NOT NULL,digest TEXT NOT NULL,record_sha256 TEXT NOT NULL);`,
    );
  }
  private index(key: string) {
    return this.storage.sql
      .exec<ReceiptIndex>(
        "SELECT object_key,ordinal,input_sha256,digest,record_sha256 FROM host_receipts WHERE batch_key=?",
        key,
      )
      .toArray()[0];
  }
  private archive(key: string) {
    const row = this.storage.sql
      .exec<Archive>(
        "SELECT object_key,sha256,bytes,checkpoint_key,binding_json,cursor_json,checkpoint_sha256,through_batch_id FROM host_receipt_archives WHERE object_key=?",
        key,
      )
      .toArray()[0];
    if (!row) throw Error("host receipt archive metadata missing");
    return row;
  }
  private decision(key: string, kind: HostTerminalReply["kind"]) {
    const table = kind === "committed" ? "batches" : "controlled_denials";
    const row = this.storage.sql
      .exec<DecisionRow>(
        `SELECT digest,receipt FROM ${table} WHERE batch_key=?`,
        key,
      )
      .toArray()[0];
    if (!row) throw Error("host receipt durable decision missing");
    return row;
  }
  private async readObject(
    key: string,
    sha: string,
    size: number,
    max = LIMIT,
  ) {
    hash(sha);
    if (!Number.isSafeInteger(size) || size < 1 || size > max)
      throw Error("host receipt object size bound");
    const object = await this.bucket.get(key);
    if (!object || object.size !== size)
      throw Error("host receipt object missing or wrong size");
    const bytes = new Uint8Array(await object.arrayBuffer());
    if (bytes.byteLength !== size || (await digest(bytes)) !== sha)
      throw Error("host receipt object digest mismatch");
    return bytes;
  }
  private manifest(e: HostReceiptEnvelope) {
    const row = this.storage.sql
      .exec<{
        manifest: string;
      }>(
        "SELECT manifest FROM checkpoints WHERE object_key=?",
        checkpointKey(e),
      )
      .toArray()[0];
    if (!row) throw Error("host receipt exact checkpoint missing");
    const m = JSON.parse(row.manifest);
    if (
      m.schema_version !== 1 ||
      !equal(binding(m.binding), e.binding) ||
      !equal(cursor(m.cursor), e.cursor) ||
      m.sha256 !== e.checkpoint_sha256 ||
      m.object_key !== checkpointKey(e) ||
      !Number.isSafeInteger(m.byte_length) ||
      m.byte_length < 1 ||
      m.byte_length > CHECKPOINT_LIMIT
    )
      throw Error("host receipt checkpoint manifest mismatch");
    return { json: row.manifest, bytes: m.byte_length as number };
  }
  private async acceptedPayload(
    key: string,
    sha: string,
    readArchive: AcceptedArchiveRead,
  ) {
    const stored = this.storage.sql
      .exec<{
        bytes: number;
      }>("SELECT bytes FROM batches WHERE batch_key=?", key)
      .toArray()[0];
    if (
      !stored ||
      !Number.isSafeInteger(stored.bytes) ||
      stored.bytes < 1 ||
      stored.bytes > LIMIT
    )
      throw Error("host receipt accepted payload bound");
    // Check the aggregate before reading cells, including a damaged oversized row.
    const shape = this.storage.sql
      .exec<{
        count: number;
        total: number;
        largest: number;
        first: number | null;
        last: number | null;
      }>(
        "SELECT COUNT(*) AS count,COALESCE(SUM(length(CAST(payload AS BLOB))),0) AS total,COALESCE(MAX(length(CAST(payload AS BLOB))),0) AS largest,MIN(ordinal) AS first,MAX(ordinal) AS last FROM journal_chunks WHERE batch_key=?",
        key,
      )
      .one();
    let bytes: Uint8Array;
    if (shape.count) {
      if (
        shape.count > 64 ||
        shape.total !== stored.bytes ||
        shape.largest > CHUNK ||
        shape.first !== 0 ||
        shape.last !== shape.count - 1
      )
        throw Error("host receipt accepted chunk bounds mismatch");
      const rows = this.storage.sql
        .exec<{
          payload: string;
        }>(
          "SELECT payload FROM journal_chunks WHERE batch_key=? ORDER BY ordinal",
          key,
        )
        .toArray();
      bytes = new TextEncoder().encode(rows.map((r) => r.payload).join(""));
    } else {
      const a = this.storage.sql
        .exec<{
          object_key: string;
          sha256: string;
          bytes: number;
          byte_offset: number;
          byte_length: number;
        }>(
          "SELECT a.object_key,a.sha256,a.bytes,p.byte_offset,p.byte_length FROM archived_batches p JOIN journal_archives a ON a.object_key=p.object_key WHERE p.batch_key=?",
          key,
        )
        .toArray()[0];
      if (
        !a ||
        !Number.isSafeInteger(a.byte_offset) ||
        a.byte_offset < 0 ||
        !Number.isSafeInteger(a.byte_length) ||
        a.byte_length !== stored.bytes ||
        a.byte_offset + a.byte_length > a.bytes
      )
        throw Error("host receipt accepted archive bounds mismatch");
      bytes = (await readArchive(a.object_key, a.sha256, a.bytes)).slice(
        a.byte_offset,
        a.byte_offset + a.byte_length,
      );
    }
    if (bytes.byteLength !== stored.bytes || (await digest(bytes)) !== sha)
      throw Error("host receipt accepted payload digest mismatch");
    const raw = JSON.parse(
      new TextDecoder("utf-8", { fatal: true, ignoreBOM: false }).decode(bytes),
    );
    const p = {
      kind: "proposal",
      status: "pending",
      digest: sha,
      payload: raw,
    };
    return Object.hasOwn(raw, "control_v2")
      ? controlledProposal(p).payload
      : proposal(p).payload;
  }
  private async validateDecision(
    e: HostReceiptEnvelope,
    r: HostReceiptRecord,
    readArchive: AcceptedArchiveRead,
  ) {
    const key = e.binding.lot_incarnation + ":" + r.batch_id;
    const row = this.decision(key, r.reply.kind),
      decision = JSON.parse(row.receipt);
    if (
      row.digest !== r.digest ||
      decision.status !== r.reply.kind ||
      decision.batch_id !== r.batch_id ||
      decision.digest !== r.digest ||
      (r.reply.kind === "committed" && !equal(decision.cursor, r.reply.cursor))
    )
      throw Error("host receipt durable decision mismatch");
    const action = this.actionOutcomes.forBatch(key);
    if (Object.hasOwn(decision, "action_outcome_sha256") && !action)
      throw Error("player action durable decision evidence missing");
    if (action) await this.actionOutcomes.verifyDecision(action);
    if (
      action &&
      (action.input_sha256 !== r.input_sha256 ||
        action.outcome.status !== r.reply.kind ||
        action.outcome.batch_id !== r.batch_id ||
        action.outcome.digest !== r.digest ||
        !equal(action.outcome.binding, e.binding) ||
        !equal(action.outcome.cursor, r.reply.cursor) ||
        action.outcome.receipt_hex !== r.reply.receipt_hex)
    )
      throw Error("host receipt differs from atomic player action outcome");
    if (r.reply.kind === "committed") {
      const p = await this.acceptedPayload(key, r.digest, readArchive);
      if (
        !equal(p.binding, e.binding) ||
        p.batch_id !== r.batch_id ||
        !equal(p.result, r.reply.cursor) ||
        p.ticks_hex !== r.reply.ticks_hex ||
        p.effects_hex !== r.reply.effects_hex ||
        p.events_hex !== r.reply.events_hex ||
        !equal("control_v2" in p ? p.control_v2 : null, r.reply.control_v2)
      )
        throw Error("host receipt reply does not match accepted payload");
      const controlled = p as ReturnType<typeof controlledProposal>["payload"];
      if (!equal(controlled.departure_v1 ?? null, r.reply.departure_v1 ?? null))
        throw Error(
          "host receipt departure evidence does not match accepted payload",
        );
      if (controlled.departure_v1) {
        await verifyDepartureEvidence(controlled);
        if (
          decision.departure_evidence_sha256 !==
          (await digest(encode(controlled.departure_v1)))
        )
          throw Error("host receipt departure committed evidence mismatch");
      }
      if (controlled.action_v1) {
        await verifyActionEvidence(controlled);
        this.actionOutcomes.assertReplay(controlled, r.digest, "committed");
        if (
          controlled.action_v1.input_sha256 !== r.input_sha256 ||
          controlled.action_v1.accepted_receipt_hex !== r.reply.receipt_hex
        )
          throw Error(
            "host receipt differs from accepted player action evidence",
          );
      }
    }
    return row;
  }
  async persist(
    raw: unknown,
    receipt_set_sha256: unknown,
  ): Promise<HostReceiptPersistAck> {
    const e = envelope(raw),
      sha = hash(receipt_set_sha256),
      bytes = encode(e);
    if (bytes.byteLength > LIMIT)
      throw Error("host receipt envelope size bound");
    if ((await digest(bytes)) !== sha)
      throw Error("host receipt envelope digest mismatch");
    const checkpoint = this.manifest(e);
    await this.readObject(
      checkpointKey(e),
      e.checkpoint_sha256,
      checkpoint.bytes,
      CHECKPOINT_LIMIT,
    );
    const decisions: DecisionRow[] = [],
      recordHashes: string[] = [];
    // One bounded, authenticated archive snapshot per persist call. Each payload
    // still gets its own SQL bounds, copied slice, digest and semantic checks.
    // Reverify an outgoing snapshot before eviction or receipt publication.
    let acceptedArchive:
      | { key: string; sha256: string; size: number; data: Uint8Array }
      | undefined;
    const verifyAcceptedArchive = async () => {
      if (!acceptedArchive) return;
      const { key, sha256, size } = acceptedArchive;
      acceptedArchive = undefined;
      await this.readObject(key, sha256, size);
    };
    const readArchive: AcceptedArchiveRead = async (key, sha256, size) => {
      if (
        !acceptedArchive ||
        acceptedArchive.key !== key ||
        acceptedArchive.sha256 !== sha256 ||
        acceptedArchive.size !== size
      ) {
        if (acceptedArchive) await verifyAcceptedArchive();
        const data = await this.readObject(key, sha256, size);
        acceptedArchive = { key, sha256, size, data };
      }
      return acceptedArchive.data;
    };
    for (const r of e.records) {
      decisions.push(await this.validateDecision(e, r, readArchive));
      recordHashes.push(await digest(encode(r)));
      const prior = await this.lookup(e.binding.lot_incarnation, r.batch_id);
      if (prior.status === "recorded" && !equal(prior.record, r))
        throw Error("host receipt batch identity reused");
    }
    if (acceptedArchive) await verifyAcceptedArchive();
    const key = objectKey(e, sha),
      existing = await this.bucket.get(key);
    if (existing) {
      if (
        existing.size !== bytes.byteLength ||
        (await digest(new Uint8Array(await existing.arrayBuffer()))) !== sha
      )
        throw Error("immutable host receipt object corrupted");
    } else await this.bucket.put(key, bytes);
    await this.readObject(key, sha, bytes.byteLength);
    // The readback precedes this atomic publication. Failed I/O leaves no new
    // replay index or acknowledgement, and never authorizes host retirement.
    this.storage.transactionSync(() => {
      if (this.manifest(e).json !== checkpoint.json)
        throw Error("host receipt checkpoint changed before publication");
      const metadata: Archive = {
        object_key: key,
        sha256: sha,
        bytes: bytes.byteLength,
        checkpoint_key: checkpointKey(e),
        binding_json: canonical(e.binding),
        cursor_json: canonical(e.cursor),
        checkpoint_sha256: e.checkpoint_sha256,
        through_batch_id: e.through_batch_id,
      };
      for (let ordinal = 0; ordinal < e.records.length; ordinal++) {
        const r = e.records[ordinal],
          batchKey = e.binding.lot_incarnation + ":" + r.batch_id;
        if (!equal(this.decision(batchKey, r.reply.kind), decisions[ordinal]))
          throw Error("host receipt decision changed before publication");
        const prior = this.index(batchKey);
        if (
          prior &&
          (prior.input_sha256 !== r.input_sha256 ||
            prior.digest !== r.digest ||
            prior.record_sha256 !== recordHashes[ordinal] ||
            this.archive(prior.object_key).binding_json !==
              metadata.binding_json)
        )
          throw Error("host receipt batch identity reused");
      }
      this.storage.sql.exec(
        "INSERT OR IGNORE INTO host_receipt_archives(object_key,sha256,bytes,checkpoint_key,binding_json,cursor_json,checkpoint_sha256,through_batch_id) VALUES(?,?,?,?,?,?,?,?)",
        key,
        sha,
        bytes.byteLength,
        metadata.checkpoint_key,
        metadata.binding_json,
        metadata.cursor_json,
        metadata.checkpoint_sha256,
        metadata.through_batch_id,
      );
      if (!equal(this.archive(key), metadata))
        throw Error("host receipt archive identity reused");
      for (let ordinal = 0; ordinal < e.records.length; ordinal++) {
        const r = e.records[ordinal];
        this.storage.sql.exec(
          "INSERT OR IGNORE INTO host_receipts(batch_key,object_key,ordinal,input_sha256,digest,record_sha256) VALUES(?,?,?,?,?,?)",
          e.binding.lot_incarnation + ":" + r.batch_id,
          key,
          ordinal,
          r.input_sha256,
          r.digest,
          recordHashes[ordinal],
        );
      }
    });
    return {
      schema_version: 1,
      status: "persisted",
      binding: e.binding,
      cursor: e.cursor,
      checkpoint_sha256: e.checkpoint_sha256,
      through_batch_id: e.through_batch_id,
      receipt_set_sha256: sha,
    };
  }
  async lookup(
    incarnation: unknown,
    batch_id: unknown,
  ): Promise<HostReceiptLookup> {
    const inc = nonzero(incarnation),
      id = nonzero(batch_id),
      index = this.index(inc + ":" + id);
    if (!index) return { status: "unknown" };
    const a = this.archive(index.object_key),
      bytes = await this.readObject(a.object_key, a.sha256, a.bytes),
      serialized = new TextDecoder("utf-8", {
        fatal: true,
        ignoreBOM: false,
      }).decode(bytes),
      e = envelope(JSON.parse(serialized));
    if (
      canonical(e) !== serialized ||
      objectKey(e, a.sha256) !== a.object_key ||
      e.binding.lot_incarnation !== inc ||
      canonical(e.binding) !== a.binding_json ||
      canonical(e.cursor) !== a.cursor_json ||
      checkpointKey(e) !== a.checkpoint_key ||
      e.checkpoint_sha256 !== a.checkpoint_sha256 ||
      e.through_batch_id !== a.through_batch_id ||
      !Number.isSafeInteger(index.ordinal) ||
      index.ordinal < 0 ||
      index.ordinal >= e.records.length
    )
      throw Error("host receipt archive binding or index mismatch");
    const record = e.records[index.ordinal];
    if (
      record.batch_id !== id ||
      record.input_sha256 !== index.input_sha256 ||
      record.digest !== index.digest ||
      (await digest(encode(record))) !== index.record_sha256
    )
      throw Error("host receipt archived record identity mismatch");
    return { status: "recorded", record };
  }
}
