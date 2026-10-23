/** Narrow private consumer for complete owned supervisor-event lanes. It never
 * acknowledges an effect, a partially understood vector, or a socket write. */
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
} from "./contracts";
import { departureRequestV1 } from "./departures";
export function eventBytes(hex: unknown) {
  if (
    typeof hex !== "string" ||
    hex.length < 16 ||
    hex.length > 512 * 1024 ||
    hex.length % 2 ||
    !/^[0-9a-f]+$/.test(hex)
  )
    throw Error("owned event byte bound");
  return Uint8Array.from(hex.match(/../g)!, (x) => parseInt(x, 16));
}
export function eventKey(raw: unknown) {
  if (typeof raw !== "string" || !/^[0-9]{1,20}:[0-9]{1,20}$/.test(raw))
    throw Error("invalid owned event batch key");
  const [incarnation, id] = raw.split(":");
  decimal(incarnation);
  decimal(id);
  return raw;
}
/** Independent fixed-layout decoder for append-only native tags 4..6. Unknown
 * tags cannot be skipped safely; the entire lane remains pending. */
export function decodeOwnedEvents(hex: unknown) {
  const bytes = eventBytes(hex),
    v = new DataView(bytes.buffer);
  let pos = 0;
  const take = (n: number) => {
    if (pos + n > bytes.length) throw Error("truncated owned event vector");
    const at = pos;
    pos += n;
    return at;
  };
  const u64 = () => v.getBigUint64(take(8), true).toString(),
    u32 = () => v.getUint32(take(4), true);
  const count = BigInt(u64());
  if (count < 1n || count > 128n) throw Error("owned event member bound");
  const out = [];
  let previousTick = 0n,
    previousOrdinal = -1;
  for (let i = 0; i < Number(count); i++) {
    const tick = u64(),
      ordinal = u32(),
      tag = u32();
    if (![4, 5, 6].includes(tag))
      return { supported: false as const, reason: "unsupported_event_kind" };
    if (
      BigInt(tick) < previousTick ||
      (BigInt(tick) === previousTick && ordinal <= previousOrdinal) ||
      tick === "0"
    )
      throw Error("owned event ordering");
    previousTick = BigInt(tick);
    previousOrdinal = ordinal;
    const actor = { object_id: v.getInt16(take(2), true), generation: u32() },
      control_generation = u64();
    if (
      actor.object_id <= 0 ||
      actor.generation === 0 ||
      control_generation === "0"
    )
      throw Error("owned event actor identity");
    if (tag !== 6 && u64() !== tick)
      throw Error("owned event inner tick mismatch");
    const reason = tag === 5 ? u32() : null;
    if (reason !== null && reason > 1) throw Error("owned detach reason");
    out.push({
      kind: tag === 4 ? "capture" : tag === 5 ? "detach" : "eod",
      tick,
      ordinal,
      actor,
      control_generation,
      removal_reason:
        reason === null
          ? null
          : reason === 0
            ? "source_leave"
            : "forced_cleanup",
    });
  }
  if (pos !== bytes.length) throw Error("trailing owned event bytes");
  return { supported: true as const, members: out };
}
export function ownedEventProof(raw: unknown) {
  const p = fields(raw, [
    "schema_version",
    "kind",
    "binding",
    "cursor",
    "native_eod_checkpoint_sha256",
    "events_sha256",
    "members",
    "pending",
  ]);
  if (
    p.schema_version !== 1 ||
    p.kind !== "owned_event_lane_proof" ||
    !Array.isArray(p.members) ||
    p.members.length > 128 ||
    !Array.isArray(p.pending) ||
    p.pending.length
  )
    throw Error("owned event lane is unresolved");
  const members = p.members.map((raw) => {
    const m = fields(raw, ["kind", "tick", "ordinal", "request", "detail"]);
    if (
      !["capture", "detach", "eod"].includes(m.kind as string) ||
      !Number.isSafeInteger(m.ordinal) ||
      Number(m.ordinal) < 0 ||
      Number(m.ordinal) > 0xffffffff
    )
      throw Error("owned event member shape");
    let detail: Record<string, unknown>;
    if (m.kind === "capture") detail = fields(m.detail, []);
    else if (m.kind === "detach") {
      detail = fields(m.detail, ["removal_reason"]);
      if (
        !["source_leave", "forced_cleanup"].includes(
          detail.removal_reason as string,
        )
      )
        throw Error("detach reason");
    } else {
      detail = fields(m.detail, ["disposition", "session_ids"]);
      if (!Array.isArray(detail.session_ids) || detail.session_ids.length > 64)
        throw Error("EOD session proof bound");
      const ids = detail.session_ids.map(decimal);
      if (
        new Set(ids).size !== ids.length ||
        ids.includes("0") ||
        detail.disposition !==
          (ids.length ? "exact_sessions_terminal" : "complete_known_clear")
      )
        throw Error("EOD terminal disposition mismatch");
      detail = { disposition: detail.disposition, session_ids: ids };
    }
    return {
      kind: m.kind as "capture" | "detach" | "eod",
      tick: decimal(m.tick),
      ordinal: Number(m.ordinal),
      request: departureRequestV1(m.request),
      detail,
    };
  });
  return {
    schema_version: 1 as const,
    kind: "owned_event_lane_proof",
    binding: binding(p.binding),
    cursor: cursor(p.cursor),
    native_eod_checkpoint_sha256: hash(p.native_eod_checkpoint_sha256),
    events_sha256: hash(p.events_sha256),
    members,
    pending: [],
  };
}
export type OwnedEventProof = ReturnType<typeof ownedEventProof>;
export async function matchOwnedEventProof(
  hex: string,
  proof: OwnedEventProof,
) {
  const decoded = decodeOwnedEvents(hex);
  if (!decoded.supported)
    throw Error("unsupported member keeps whole lane pending");
  if (
    (await digest(eventBytes(hex))) !== proof.events_sha256 ||
    decoded.members.length !== proof.members.length
  )
    throw Error("owned event whole-lane digest/count mismatch");
  for (let i = 0; i < decoded.members.length; i++) {
    const e = decoded.members[i],
      p = proof.members[i],
      id = p.request.identity;
    if (
      e.kind !== p.kind ||
      e.tick !== p.tick ||
      e.ordinal !== p.ordinal ||
      !equal(e.actor, id.actor) ||
      e.control_generation !== id.control_generation ||
      (e.kind === "detach" && e.removal_reason !== p.detail.removal_reason)
    )
      throw Error("owned event member identity mismatch");
  }
}
export class OwnedEventReceipts {
  private storage: DurableObjectStorage;
  constructor(storage: DurableObjectStorage) {
    this.storage = storage;
    storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS owned_event_receipts(batch_key TEXT PRIMARY KEY,digest TEXT NOT NULL,proof_sha256 TEXT NOT NULL,receipt TEXT NOT NULL)",
    );
  }
  prior(key: string, sha: string, proofSha: string) {
    const row = this.storage.sql
      .exec<{
        digest: string;
        proof_sha256: string;
        receipt: string;
      }>("SELECT digest,proof_sha256,receipt FROM owned_event_receipts WHERE batch_key=?", key)
      .toArray()[0];
    if (!row) return null;
    if (row.digest !== sha || row.proof_sha256 !== proofSha)
      throw Error("owned event receipt identity reused");
    return JSON.parse(row.receipt);
  }
  record(key: string, sha: string, proofSha: string, receipt: unknown) {
    if (
      this.storage.sql
        .exec<{ n: number }>("SELECT COUNT(*) AS n FROM owned_event_receipts")
        .one().n >= 8192
    )
      throw Error("owned event receipt retention capacity");
    this.storage.sql.exec(
      "INSERT INTO owned_event_receipts(batch_key,digest,proof_sha256,receipt) VALUES(?,?,?,?)",
      key,
      sha,
      proofSha,
      canonical(receipt),
    );
    this.storage.sql.exec(
      "DELETE FROM outbox WHERE batch_key=? AND lane='events' AND digest=?",
      key,
      sha,
    );
  }
}
