/** Private native Timer state is atomically paired with the authoritative VM
 * cursor. This module stores opaque producer-validated bytes; it neither calls a
 * plugin nor treats a socket write/checkpoint as delivery acknowledgement. */
import {
  binding,
  canonical,
  cursor,
  decimal,
  digest,
  fields,
  hash,
  type Binding,
  type Cursor,
} from "./contracts";
export const EOD_PRIVATE_CHECKPOINT_LIMIT = 192 * 1024;
const same = (a: unknown, b: unknown) => canonical(a) === canonical(b);
const bytes = (value: unknown) => {
  if (
    typeof value !== "string" ||
    !value.length ||
    value.length > EOD_PRIVATE_CHECKPOINT_LIMIT * 2 ||
    value.length % 2 ||
    !/^[0-9a-f]+$/.test(value)
  )
    throw Error("private EOD checkpoint bounds");
  return value;
};
const decode = (value: string) =>
  Uint8Array.from(value.match(/../g)!, (v) => parseInt(v, 16));
export function nativeEodCandidateV1(raw: unknown) {
  const v = fields(raw, [
    "schema_version",
    "revision_before",
    "revision_after",
    "predecessor_sha256",
    "checkpoint_sha256",
    "checkpoint_hex",
  ]);
  const before = decimal(v.revision_before),
    after = decimal(v.revision_after);
  if (
    v.schema_version !== 1 ||
    BigInt(after) !== BigInt(before) + 1n ||
    (before === "0") !== (v.predecessor_sha256 === null)
  )
    throw Error("private EOD revision transition");
  return {
    schema_version: 1 as const,
    revision_before: before,
    revision_after: after,
    predecessor_sha256:
      v.predecessor_sha256 === null ? null : hash(v.predecessor_sha256),
    checkpoint_sha256: hash(v.checkpoint_sha256),
    checkpoint_hex: bytes(v.checkpoint_hex),
  };
}
export type NativeEodCandidateV1 = ReturnType<typeof nativeEodCandidateV1>;
export function nativeEodCheckpointV1(raw: unknown) {
  const v = fields(raw, [
    "schema_version",
    "binding",
    "cursor",
    "revision",
    "checkpoint_sha256",
    "checkpoint_hex",
  ]);
  if (v.schema_version !== 1 || decimal(v.revision) === "0")
    throw Error("invalid private EOD checkpoint revision");
  return {
    schema_version: 1 as const,
    binding: binding(v.binding),
    cursor: cursor(v.cursor),
    revision: decimal(v.revision),
    checkpoint_sha256: hash(v.checkpoint_sha256),
    checkpoint_hex: bytes(v.checkpoint_hex),
  };
}
export type NativeEodCheckpointV1 = ReturnType<typeof nativeEodCheckpointV1>;
export async function verifyNativeEodCandidate(
  value: NativeEodCandidateV1 | undefined,
) {
  if (
    value &&
    (await digest(decode(value.checkpoint_hex))) !== value.checkpoint_sha256
  )
    throw Error("private EOD checkpoint digest mismatch");
}
export class NativeEodStore {
  constructor(private readonly storage: DurableObjectStorage) {
    storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS native_eod_state(id INTEGER PRIMARY KEY CHECK(id=1),value TEXT NOT NULL)",
    );
  }
  current(): NativeEodCheckpointV1 | null {
    const row = this.storage.sql
      .exec<{ value: string }>("SELECT value FROM native_eod_state WHERE id=1")
      .toArray()[0];
    return row ? nativeEodCheckpointV1(JSON.parse(row.value)) : null;
  }
  validate(
    candidate: NativeEodCandidateV1 | undefined,
    expectedBinding: Binding,
    predecessor: Cursor,
  ) {
    const current = this.current();
    if (!candidate) {
      if (current)
        throw Error("initialized private EOD stream cannot be omitted");
      return;
    }
    if (current) {
      if (
        !same(current.binding, expectedBinding) ||
        !same(current.cursor, predecessor) ||
        current.revision !== candidate.revision_before ||
        current.checkpoint_sha256 !== candidate.predecessor_sha256
      )
        throw Error("stale private EOD binding, cursor or checkpoint");
    } else if (
      candidate.revision_before !== "0" ||
      candidate.predecessor_sha256 !== null
    )
      throw Error("private EOD predecessor missing");
  }
  /** Caller must already have validated and must use the same SQL transaction
   * as batch/journal/VM cursor. There is deliberately no output ack operation. */
  record(
    candidate: NativeEodCandidateV1 | undefined,
    b: Binding,
    result: Cursor,
  ) {
    if (!candidate) return;
    const value: NativeEodCheckpointV1 = {
      schema_version: 1,
      binding: b,
      cursor: result,
      revision: candidate.revision_after,
      checkpoint_sha256: candidate.checkpoint_sha256,
      checkpoint_hex: candidate.checkpoint_hex,
    };
    this.storage.sql.exec(
      "INSERT INTO native_eod_state(id,value) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET value=excluded.value",
      canonical(value),
    );
  }
  /** Reuse the existing immutable batch receipt (retained across archive
   * compaction); no second unbounded per-tick tombstone/index is created. */
  receipt(candidate: NativeEodCandidateV1 | undefined) {
    return candidate
      ? {
          native_eod_checkpoint_sha256: candidate.checkpoint_sha256,
          native_eod_revision: candidate.revision_after,
        }
      : {};
  }
  assertReplay(
    candidate: NativeEodCandidateV1 | undefined,
    receipt: Record<string, unknown>,
  ) {
    if (
      candidate
        ? receipt.native_eod_checkpoint_sha256 !==
            candidate.checkpoint_sha256 ||
          receipt.native_eod_revision !== candidate.revision_after
        : receipt.native_eod_checkpoint_sha256 !== undefined ||
          receipt.native_eod_revision !== undefined
    )
      throw Error("private EOD durable decision mismatch");
  }
}
