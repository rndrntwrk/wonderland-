import { decimal, fields, hash } from "../lot/contracts";
/** Binding-only reservation. This never establishes an admitted actor. */
export function controlTarget(raw: unknown) {
  const v = fields(raw, [
    "avatar_id",
    "lot_id",
    "content_release",
    "protocol_release",
  ]);
  const avatarId = decimal(v.avatar_id),
    lotId = decimal(v.lot_id);
  if (BigInt(avatarId) < 1n || BigInt(avatarId) > 16777215n || lotId === "0")
    throw Error("invalid control target");
  return {
    avatar_id: avatarId,
    lot_id: lotId,
    content_release: hash(v.content_release),
    protocol_release: hash(v.protocol_release),
  };
}
export interface ControlClaim {
  version: 1;
  account_id: string;
  avatar_id: string;
  lot_id: string;
  generation: string;
  content_release: string;
  protocol_release: string;
  expires_at: number;
  state: "reserved" | "fence_pending";
}
