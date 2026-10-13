import { decimal, fields, hash } from "./contracts";

/** Private service identity for a custody fence. Never accepted from public HTTP. */
export function controlFence(raw: unknown) {
  const v = fields(raw, [
    "version",
    "authority",
    "avatar_id",
    "generation",
    "lot_id",
    "request_digest",
  ]);
  if (v.version !== 1) throw Error("unsupported control fence");
  const avatar = decimal(v.avatar_id),
    generation = decimal(v.generation),
    lot = decimal(v.lot_id);
  if (
    avatar === "0" ||
    BigInt(avatar) > 16777215n ||
    generation === "0" ||
    lot === "0"
  )
    throw Error("invalid control fence identity");
  return {
    version: 1 as const,
    authority: hash(v.authority),
    avatar_id: avatar,
    generation,
    lot_id: lot,
    request_digest: hash(v.request_digest),
  };
}
export type ControlFence = ReturnType<typeof controlFence>;
