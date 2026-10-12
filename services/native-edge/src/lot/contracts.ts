export interface Cursor {
  lot_id: string;
  authority_epoch: string;
  completed_tick: string;
  state_hash: string;
}
export interface Binding {
  protocol: 1;
  source_pin: string;
  binding_sha256: string;
  lot_incarnation: string;
}
export interface Payload {
  binding: Binding;
  batch_id: string;
  predecessor: Cursor;
  result: Cursor;
  ticks_hex: string;
  checkpoint_sha256: string;
  effects_hex: string;
  events_hex: string;
}
export interface Proposal {
  kind: "proposal";
  status: "pending";
  digest: string;
  payload: Payload;
}
const U64 = 18446744073709551615n;
export function decimal(value: unknown): string {
  if (
    typeof value !== "string" ||
    !/^(0|[1-9][0-9]{0,19})$/.test(value) ||
    BigInt(value) > U64
  )
    throw Error("invalid decimal identity");
  return value;
}
export function hash(value: unknown): string {
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value))
    throw Error("invalid digest");
  return value;
}
export function fields(
  value: unknown,
  names: string[],
): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw Error("expected record");
  const obj = value as Record<string, unknown>;
  if (Object.keys(obj).sort().join("|") !== [...names].sort().join("|"))
    throw Error("unexpected record fields");
  return obj;
}
export function cursor(value: unknown): Cursor {
  const v = fields(value, [
    "lot_id",
    "authority_epoch",
    "completed_tick",
    "state_hash",
  ]);
  return {
    lot_id: decimal(v.lot_id),
    authority_epoch: decimal(v.authority_epoch),
    completed_tick: decimal(v.completed_tick),
    state_hash: hash(v.state_hash),
  };
}
export function binding(value: unknown): Binding {
  const v = fields(value, [
    "protocol",
    "source_pin",
    "binding_sha256",
    "lot_incarnation",
  ]);
  if (
    v.protocol !== 1 ||
    v.source_pin !== "4c6b3e8f5835b228723caea3c9f683c62f244f73" ||
    decimal(v.lot_incarnation) === "0"
  )
    throw Error("binding mismatch");
  return {
    protocol: 1,
    source_pin: v.source_pin,
    binding_sha256: hash(v.binding_sha256),
    lot_incarnation: v.lot_incarnation as string,
  };
}
function hex(value: unknown, max: number): string {
  if (
    typeof value !== "string" ||
    value.length > max * 2 ||
    value.length % 2 !== 0 ||
    !/^[0-9a-f]*$/.test(value)
  )
    throw Error("invalid binary payload");
  return value;
}
export function proposal(value: unknown): Proposal {
  const v = fields(value, ["kind", "status", "digest", "payload"]);
  if (v.kind !== "proposal" || v.status !== "pending")
    throw Error("expected pending proposal");
  const p = fields(v.payload, [
    "binding",
    "batch_id",
    "predecessor",
    "result",
    "ticks_hex",
    "checkpoint_sha256",
    "effects_hex",
    "events_hex",
  ]);
  const payload: Payload = {
    binding: binding(p.binding),
    batch_id: decimal(p.batch_id),
    predecessor: cursor(p.predecessor),
    result: cursor(p.result),
    ticks_hex: hex(p.ticks_hex, 8 * 1024 * 1024),
    checkpoint_sha256: hash(p.checkpoint_sha256),
    effects_hex: hex(p.effects_hex, 4 * 1024 * 1024),
    events_hex: hex(p.events_hex, 4 * 1024 * 1024),
  };
  if (
    payload.batch_id === "0" ||
    payload.result.lot_id !== payload.predecessor.lot_id ||
    payload.result.authority_epoch !== payload.predecessor.authority_epoch
  )
    throw Error("batch identity mismatch");
  const n =
    BigInt(payload.result.completed_tick) -
    BigInt(payload.predecessor.completed_tick);
  if (n < 1n || n > 16n) throw Error("batch tick bounds");
  return {
    kind: "proposal",
    status: "pending",
    digest: hash(v.digest),
    payload,
  };
}
export function canonical(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  return (
    "{" +
    Object.entries(value as Record<string, unknown>)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([k, v]) => JSON.stringify(k) + ":" + canonical(v))
      .join(",") +
    "}"
  );
}
export async function digest(bytes: Uint8Array): Promise<string> {
  const hash = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(hash)]
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}
export const encode = (value: unknown) =>
  new TextEncoder().encode(canonical(value));
export const equal = (a: unknown, b: unknown) => canonical(a) === canonical(b);
