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
  type Binding,
  type Cursor,
  type Payload,
} from "./contracts";

const ACTION_LIMIT = 16 * 1024;
const OUTCOME_LIMIT = 100 * 1024;
const nonzero = (raw: unknown) => {
  const value = decimal(raw);
  if (value === "0") throw Error("zero player action identity");
  return value;
};
function binary(raw: unknown, maximum: number): Uint8Array {
  if (
    typeof raw !== "string" ||
    raw.length > maximum * 2 ||
    raw.length % 2 ||
    !/^[0-9a-f]+$/.test(raw)
  )
    throw Error("invalid player action binary payload");
  return Uint8Array.from(raw.match(/../g)!, (byte) => parseInt(byte, 16));
}
function packet(bytes: Uint8Array, magic: string) {
  if (
    bytes.length < 16 ||
    new TextDecoder().decode(bytes.subarray(0, 8)) !== magic + "\r\n\x1a\n"
  )
    throw Error("invalid player action packet");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getBigUint64(8, true) !== BigInt(bytes.length - 16))
    throw Error("invalid player action packet length");
  return view;
}
export function actionControl(raw: unknown) {
  const v = fields(raw, [
    "principal",
    "account_id",
    "avatar_id",
    "control_generation",
    "session_ref",
  ]);
  return {
    principal: nonzero(v.principal),
    account_id: nonzero(v.account_id),
    avatar_id: nonzero(v.avatar_id),
    control_generation: nonzero(v.control_generation),
    session_ref: hash(v.session_ref),
  };
}
export type ActionControl = ReturnType<typeof actionControl>;

/** Only the fixed player input ABI, never a VM/tick decoder. See live_wire/player.rs
 * and sim-core/interactions/{query,queue,offers}.rs: bincode fixint little endian. */
export function decodeActionRequest(raw: unknown) {
  const bytes = binary(raw, ACTION_LIMIT),
    view = packet(bytes, "WLC1");
  if (bytes.length < 20) throw Error("invalid player action variant");
  const variant = view.getUint32(16, true);
  if (
    (variant === 0 && bytes.length !== 94) ||
    (variant === 1 && bytes.length !== 76) ||
    (variant === 2 && bytes.length !== 77) ||
    variant > 2
  )
    throw Error("invalid player action variant or length");
  // Invoke/Cancel retain their original layout. SourceGoto appends variant 2
  // with actor immediately after principal; no target or global flag is exposed.
  if (variant === 0 && view.getUint32(88, true) > 1)
    throw Error("invalid player action interaction scope");
  const principal = nonzero(view.getBigUint64(20, true).toString()),
    command_sequence = nonzero(
      view.getBigUint64(variant === 0 ? 76 : 60, true).toString(),
    ),
    object_id = view.getUint32(variant === 2 ? 28 : 36, true),
    generation = view.getUint32(variant === 2 ? 32 : 40, true);
  if (object_id < 1 || object_id > 32767 || generation === 0)
    throw Error("invalid player action actor");
  return {
    bytes,
    principal,
    command_sequence,
    actor: { object_id, generation },
  };
}
export function validateActionReceipt(
  raw: unknown,
  request: string,
  tick: string | null,
): string {
  const bytes = binary(raw, ACTION_LIMIT + 64),
    view = packet(bytes, "WLA1"),
    requestBytes = binary(request, ACTION_LIMIT),
    suffix = tick === null ? 1 : 9;
  if (
    bytes.length !== 24 + requestBytes.length + suffix ||
    view.getBigUint64(16, true) !== BigInt(requestBytes.length)
  )
    throw Error("player action receipt length mismatch");
  for (let i = 0; i < requestBytes.length; i++)
    if (bytes[24 + i] !== requestBytes[i])
      throw Error("player action receipt request mismatch");
  const option = 24 + requestBytes.length;
  if (
    bytes[option] !== (tick === null ? 0 : 1) ||
    (tick !== null &&
      view.getBigUint64(option + 1, true) !== BigInt(decimal(tick)))
  )
    throw Error("player action receipt tick mismatch");
  return raw as string;
}
export function actionEvidence(raw: unknown, resultTick: string) {
  const v = fields(raw, [
    "schema_version",
    "control",
    "input_sha256",
    "request_hex",
    "request_sha256",
    "command_sequence",
    "accepted_receipt_hex",
    "rejected_receipt_hex",
  ]);
  if (v.schema_version !== 1) throw Error("unsupported player action evidence");
  const control = actionControl(v.control),
    request = decodeActionRequest(v.request_hex),
    sequence = nonzero(v.command_sequence);
  if (
    control.principal !== request.principal ||
    sequence !== request.command_sequence
  )
    throw Error("player action principal or sequence mismatch");
  return {
    schema_version: 1 as const,
    control,
    input_sha256: hash(v.input_sha256),
    request_hex: v.request_hex as string,
    request_sha256: hash(v.request_sha256),
    command_sequence: sequence,
    accepted_receipt_hex: validateActionReceipt(
      v.accepted_receipt_hex,
      v.request_hex as string,
      resultTick,
    ),
    rejected_receipt_hex: validateActionReceipt(
      v.rejected_receipt_hex,
      v.request_hex as string,
      null,
    ),
  };
}
export type ActionEvidence = ReturnType<typeof actionEvidence>;
type ActionPayload = Payload & {
  action_v1?: ActionEvidence;
  control_v2: { revision_before: string };
};
export async function verifyActionEvidence(payload: ActionPayload) {
  const a = payload.action_v1;
  if (!a) return;
  const { account_id: _account, ...control } = a.control;
  const input = {
    kind: "prepare_player_action",
    batch_id: payload.batch_id,
    binding: payload.binding,
    predecessor: payload.predecessor,
    expected_control_revision: payload.control_v2.revision_before,
    control,
    action_hex: a.request_hex,
  };
  if (
    (await digest(decodeActionRequest(a.request_hex).bytes)) !==
      a.request_sha256 ||
    (await digest(encode(input))) !== a.input_sha256
  )
    throw Error("player action request or input digest mismatch");
}
export function actionLookup(raw: unknown) {
  const v = fields(raw, [
    "schema_version",
    "lot_incarnation",
    "control",
    "request_hex",
  ]);
  if (v.schema_version !== 1) throw Error("unsupported player action lookup");
  const control = actionControl(v.control),
    request = decodeActionRequest(v.request_hex);
  if (control.principal !== request.principal)
    throw Error("player action lookup control mismatch");
  return {
    schema_version: 1 as const,
    lot_incarnation: nonzero(v.lot_incarnation),
    control,
    request_hex: v.request_hex as string,
  };
}
export interface ActionOutcome {
  schema_version: 1;
  status: "committed" | "rejected";
  control: ActionControl;
  request_hex: string;
  request_sha256: string;
  receipt_hex: string;
  binding: Binding;
  batch_id: string;
  digest: string;
  cursor: Cursor;
}
function actionOutcome(
  p: ActionPayload,
  sha: string,
  status: ActionOutcome["status"],
): ActionOutcome {
  const a = p.action_v1!;
  return {
    schema_version: 1,
    status,
    control: a.control,
    request_hex: a.request_hex,
    request_sha256: a.request_sha256,
    receipt_hex:
      status === "committed" ? a.accepted_receipt_hex : a.rejected_receipt_hex,
    binding: p.binding,
    batch_id: p.batch_id,
    digest: sha,
    cursor: status === "committed" ? p.result : p.predecessor,
  };
}
/** Pin both possible exact outcomes before entering the synchronous transaction.
 * The chosen hash lives in the independently retained batch/denial receipt. */
export async function actionOutcomeHashes(p: ActionPayload, sha: string) {
  if (!p.action_v1) return undefined;
  const hashOutcome = (status: ActionOutcome["status"]) =>
    digest(
      encode({
        input_sha256: p.action_v1!.input_sha256,
        outcome: actionOutcome(p, sha, status),
      }),
    );
  return {
    committed: await hashOutcome("committed"),
    rejected: await hashOutcome("rejected"),
  };
}
function outcome(raw: unknown): ActionOutcome {
  const v = fields(raw, [
    "schema_version",
    "status",
    "control",
    "request_hex",
    "request_sha256",
    "receipt_hex",
    "binding",
    "batch_id",
    "digest",
    "cursor",
  ]);
  if (
    v.schema_version !== 1 ||
    (v.status !== "committed" && v.status !== "rejected")
  )
    throw Error("invalid player action outcome");
  const control = actionControl(v.control),
    request = decodeActionRequest(v.request_hex),
    c = cursor(v.cursor);
  if (control.principal !== request.principal)
    throw Error("player action outcome control mismatch");
  return {
    schema_version: 1,
    status: v.status,
    control,
    request_hex: v.request_hex as string,
    request_sha256: hash(v.request_sha256),
    receipt_hex: validateActionReceipt(
      v.receipt_hex,
      v.request_hex as string,
      v.status === "committed" ? c.completed_tick : null,
    ),
    binding: binding(v.binding),
    batch_id: nonzero(v.batch_id),
    digest: hash(v.digest),
    cursor: c,
  };
}
function outcomeKey(
  authority: string,
  lot: string,
  incarnation: string,
  control: ActionControl,
  request_sha256: string,
) {
  return canonical({
    authority_id: hash(authority),
    lot_id: nonzero(lot),
    lot_incarnation: nonzero(incarnation),
    control,
    request_sha256,
  });
}
type Stored = {
  outcome_key: string;
  input_sha256: string;
  outcome_json: string;
};
/** Immutable, bounded per-action evidence. No TTL and no post-commit R2 dependency.
 * Rejected native sequences are not VM queue high water, so sequence is evidence,
 * never a uniqueness key. Exact control + request + original incarnation is. */
export class ActionOutcomeStore {
  private storage: DurableObjectStorage;
  constructor(storage: DurableObjectStorage) {
    this.storage = storage;
    storage.sql.exec(
      `CREATE TABLE IF NOT EXISTS player_action_outcomes(outcome_key TEXT PRIMARY KEY,batch_key TEXT NOT NULL UNIQUE,input_sha256 TEXT NOT NULL,outcome_json TEXT NOT NULL CHECK(length(CAST(outcome_json AS BLOB))<=${OUTCOME_LIMIT}));`,
    );
  }
  private read(column: "outcome_key" | "batch_key", key: string) {
    const shape = this.storage.sql
      .exec<{
        bytes: number;
      }>(
        `SELECT length(CAST(outcome_json AS BLOB)) AS bytes FROM player_action_outcomes WHERE ${column}=?`,
        key,
      )
      .toArray()[0];
    if (!shape) return null;
    if (shape.bytes < 1 || shape.bytes > OUTCOME_LIMIT)
      throw Error("player action outcome size bound");
    const row = this.storage.sql
        .exec<Stored>(
          `SELECT outcome_key,input_sha256,outcome_json FROM player_action_outcomes WHERE ${column}=?`,
          key,
        )
        .one(),
      parsed = outcome(JSON.parse(row.outcome_json));
    if (canonical(parsed) !== row.outcome_json)
      throw Error("player action outcome encoding mismatch");
    return {
      key: row.outcome_key,
      input_sha256: hash(row.input_sha256),
      outcome: parsed,
    };
  }
  forBatch(key: string) {
    return this.read("batch_key", key);
  }
  assertFresh(authority: string, lot: string, p: ActionPayload) {
    if (
      p.action_v1 &&
      this.read(
        "outcome_key",
        outcomeKey(
          authority,
          lot,
          p.binding.lot_incarnation,
          p.action_v1.control,
          p.action_v1.request_sha256,
        ),
      )
    )
      throw Error(
        "player action request already has a durable outcome; use lookup",
      );
  }
  record(
    authority: string,
    lot: string,
    p: ActionPayload,
    sha: string,
    status: ActionOutcome["status"],
  ) {
    const a = p.action_v1;
    if (!a) return;
    const result = actionOutcome(p, sha, status),
      serialized = canonical(result);
    if (new TextEncoder().encode(serialized).length > OUTCOME_LIMIT)
      throw Error("player action outcome size bound");
    this.storage.sql.exec(
      "INSERT INTO player_action_outcomes(outcome_key,batch_key,input_sha256,outcome_json) VALUES(?,?,?,?)",
      outcomeKey(
        authority,
        lot,
        p.binding.lot_incarnation,
        a.control,
        a.request_sha256,
      ),
      p.binding.lot_incarnation + ":" + p.batch_id,
      a.input_sha256,
      serialized,
    );
  }
  assertReplay(p: ActionPayload, sha: string, status: ActionOutcome["status"]) {
    const a = p.action_v1;
    if (!a) return;
    const stored = this.forBatch(p.binding.lot_incarnation + ":" + p.batch_id),
      r = stored?.outcome;
    if (
      !r ||
      stored!.input_sha256 !== a.input_sha256 ||
      r.status !== status ||
      r.batch_id !== p.batch_id ||
      r.digest !== sha ||
      !equal(r.control, a.control) ||
      r.request_hex !== a.request_hex ||
      r.request_sha256 !== a.request_sha256 ||
      r.receipt_hex !==
        (status === "committed"
          ? a.accepted_receipt_hex
          : a.rejected_receipt_hex) ||
      !equal(r.binding, p.binding) ||
      !equal(r.cursor, status === "committed" ? p.result : p.predecessor)
    )
      throw Error("player action durable decision mismatch");
    this.decisionFor(stored!);
  }
  private decisionFor(
    stored: NonNullable<ReturnType<ActionOutcomeStore["forBatch"]>>,
  ) {
    const r = stored.outcome,
      table = r.status === "committed" ? "batches" : "controlled_denials",
      row = this.storage.sql
        .exec<{
          digest: string;
          receipt: string;
        }>(
          `SELECT digest,receipt FROM ${table} WHERE batch_key=?`,
          r.binding.lot_incarnation + ":" + r.batch_id,
        )
        .toArray()[0];
    if (!row) throw Error("player action durable decision missing");
    const decision = JSON.parse(row.receipt);
    if (
      row.digest !== r.digest ||
      decision.status !== r.status ||
      decision.batch_id !== r.batch_id ||
      decision.digest !== r.digest ||
      (r.status === "committed" && !equal(decision.cursor, r.cursor))
    )
      throw Error("player action durable decision mismatch");
    return decision;
  }
  async verifyDecision(
    stored: NonNullable<ReturnType<ActionOutcomeStore["forBatch"]>>,
  ) {
    const decision = this.decisionFor(stored);
    if (
      decision.action_outcome_sha256 !==
      (await digest(
        encode({ input_sha256: stored.input_sha256, outcome: stored.outcome }),
      ))
    )
      throw Error("player action durable decision mismatch");
  }
  async lookup(raw: unknown, authority: string, lot: string) {
    const q = actionLookup(raw),
      sha = await digest(decodeActionRequest(q.request_hex).bytes),
      key = outcomeKey(authority, lot, q.lot_incarnation, q.control, sha),
      stored = this.read("outcome_key", key);
    if (!stored)
      return { schema_version: 1 as const, status: "unknown" as const };
    const r = stored.outcome;
    if (
      stored.key !== key ||
      r.cursor.lot_id !== lot ||
      r.binding.lot_incarnation !== q.lot_incarnation ||
      !equal(r.control, q.control) ||
      r.request_hex !== q.request_hex ||
      r.request_sha256 !== sha
    )
      throw Error("player action durable identity mismatch");
    await this.verifyDecision(stored);
    return r;
  }
}
