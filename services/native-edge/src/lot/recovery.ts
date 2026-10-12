import {
  binding,
  cursor,
  decimal,
  equal,
  fields,
  hash,
  type Binding,
  type Cursor,
} from "./contracts";
export interface RecoveryGrant {
  schema_version: 1;
  old_binding: Binding;
  old_cursor: Cursor;
  old_checkpoint_sha256: string;
  new_binding: Binding;
  new_authority_epoch: string;
}
export interface TakeoverMarker {
  schema_version: 1;
  grant: RecoveryGrant;
  new_cursor: Cursor;
  checkpoint_sha256: string;
}
export function successor(value: string): string {
  return decimal((BigInt(decimal(value)) + 1n).toString());
}
export function recoveryGrant(value: unknown): RecoveryGrant {
  const v = fields(value, [
    "schema_version",
    "old_binding",
    "old_cursor",
    "old_checkpoint_sha256",
    "new_binding",
    "new_authority_epoch",
  ]);
  if (v.schema_version !== 1) throw Error("recovery version mismatch");
  const old = binding(v.old_binding),
    next = binding(v.new_binding),
    previous = cursor(v.old_cursor),
    epoch = decimal(v.new_authority_epoch);
  if (
    !equal(next, { ...old, lot_incarnation: successor(old.lot_incarnation) }) ||
    epoch !== successor(previous.authority_epoch)
  )
    throw Error("recovery successor mismatch");
  return {
    schema_version: 1,
    old_binding: old,
    old_cursor: previous,
    old_checkpoint_sha256: hash(v.old_checkpoint_sha256),
    new_binding: next,
    new_authority_epoch: epoch,
  };
}
export function takeoverMarker(value: unknown): TakeoverMarker {
  const v = fields(value, [
    "schema_version",
    "grant",
    "new_cursor",
    "checkpoint_sha256",
  ]);
  if (v.schema_version !== 1) throw Error("takeover version mismatch");
  const grant = recoveryGrant(v.grant),
    next = cursor(v.new_cursor);
  if (
    next.lot_id !== grant.old_cursor.lot_id ||
    next.completed_tick !== grant.old_cursor.completed_tick ||
    next.authority_epoch !== grant.new_authority_epoch
  )
    throw Error("takeover cursor mismatch");
  return {
    schema_version: 1,
    grant,
    new_cursor: next,
    checkpoint_sha256: hash(v.checkpoint_sha256),
  };
}
