import { canonical, decimal, fields, hash, proposal } from "./contracts";
import { operation } from "../account/contracts";
import {
  permissionBinding,
  PORTABLE_LIMIT,
  type ReservedAvatarExportV2,
} from "../account/portable";
export const MAX_CONTROLS = 256;
export const EMPTY_REGISTRY_SHA256 =
  "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945";
const nonzero = (v: unknown) => {
  const n = decimal(v);
  if (n === "0") throw Error("zero controlled identity");
  return n;
};
export function exportV2(raw: unknown): ReservedAvatarExportV2 {
  const v = fields(raw, [
    "schema_version",
    "authority_id",
    "account_id",
    "avatar_id",
    "lot_id",
    "control_generation",
    "admission_operation",
    "reservation_digest",
    "session_ref",
    "account_revision",
    "content_release",
    "protocol_release",
    "expires_at",
    "permission_binding",
    "permission_source",
    "source_revisions",
    "portable_json",
    "portable_sha256",
    "export_digest",
  ]);
  if (v.schema_version !== 2) throw Error("unsupported avatar export");
  const avatar_id = nonzero(v.avatar_id);
  if (BigInt(avatar_id) > 16777215n)
    throw Error("avatar identity outside source range");
  if (!Number.isSafeInteger(v.expires_at) || (v.expires_at as number) <= 0)
    throw Error("invalid export expiry");
  if (
    typeof v.portable_json !== "string" ||
    new TextEncoder().encode(v.portable_json).byteLength > PORTABLE_LIMIT
  )
    throw Error("portable avatar byte capacity");
  if (canonical(JSON.parse(v.portable_json)) !== v.portable_json)
    throw Error("portable avatar encoding is not canonical");
  const revisions = fields(v.source_revisions, [
    "owned_assets",
    "wardrobe",
    "ledger",
    "progression",
    "permissions",
  ]);
  if (revisions.owned_assets !== 1)
    throw Error("source ownership migration required");
  const source = fields(v.permission_source, [
    "registered_at_ms",
    "created_at_ms",
    "observed_at_ms",
    "roommate_lots",
    "moderation_level",
    "mayor_nhood",
  ]);
  for (const key of ["registered_at_ms", "created_at_ms", "observed_at_ms"])
    if (!Number.isSafeInteger(source[key]) || (source[key] as number) < 0)
      throw Error("invalid source timestamp");
  if (
    !Number.isInteger(source.moderation_level) ||
    (source.moderation_level as number) < 0 ||
    (source.moderation_level as number) > 255 ||
    !Array.isArray(source.roommate_lots) ||
    source.roommate_lots.length > 256
  )
    throw Error("unsupported permission source");
  for (const raw of source.roommate_lots) {
    const row = fields(raw, ["lot_id", "permissions_level", "is_pending"]);
    nonzero(row.lot_id);
    if (
      ![0, 1, 2].includes(row.permissions_level as number) ||
      ![0, 1].includes(row.is_pending as number)
    )
      throw Error("invalid source membership");
  }
  if (source.mayor_nhood !== null) nonzero(source.mayor_nhood);
  return {
    schema_version: 2,
    authority_id: hash(v.authority_id),
    account_id: nonzero(v.account_id),
    avatar_id,
    lot_id: nonzero(v.lot_id),
    control_generation: nonzero(v.control_generation),
    admission_operation: operation(v.admission_operation),
    reservation_digest: hash(v.reservation_digest),
    session_ref: hash(v.session_ref),
    account_revision: nonzero(v.account_revision),
    content_release: hash(v.content_release),
    protocol_release: hash(v.protocol_release),
    expires_at: v.expires_at as number,
    permission_binding: permissionBinding(v.permission_binding),
    permission_source: source as ReservedAvatarExportV2["permission_source"],
    source_revisions: {
      owned_assets: 1,
      wardrobe: hash(revisions.wardrobe),
      ledger: hash(revisions.ledger),
      progression: hash(revisions.progression),
      permissions: hash(revisions.permissions),
    },
    portable_json: v.portable_json,
    portable_sha256: hash(v.portable_sha256),
    export_digest: hash(v.export_digest),
  };
}
export function actorEntry(raw: unknown) {
  const v = fields(raw, [
      "principal",
      "account_id",
      "avatar_id",
      "actor",
      "control_generation",
      "session_ref",
      "admission_operation",
      "reservation_digest",
      "export_digest",
    ]),
    actor = fields(v.actor, ["object_id", "generation"]);
  if (
    !Number.isInteger(actor.object_id) ||
    (actor.object_id as number) < 1 ||
    (actor.object_id as number) > 32767 ||
    !Number.isInteger(actor.generation) ||
    (actor.generation as number) < 1 ||
    (actor.generation as number) > 4294967295
  )
    throw Error("invalid controlled actor identity");
  const avatar_id = nonzero(v.avatar_id);
  if (BigInt(avatar_id) > 16777215n)
    throw Error("avatar identity outside source range");
  return {
    principal: nonzero(v.principal),
    account_id: nonzero(v.account_id),
    avatar_id,
    actor: {
      object_id: actor.object_id as number,
      generation: actor.generation as number,
    },
    control_generation: nonzero(v.control_generation),
    session_ref: hash(v.session_ref),
    admission_operation: operation(v.admission_operation),
    reservation_digest: hash(v.reservation_digest),
    export_digest: hash(v.export_digest),
  };
}
export type ActorEntry = ReturnType<typeof actorEntry>;
export function actorRegistry(raw: unknown): ActorEntry[] {
  if (!Array.isArray(raw) || raw.length > MAX_CONTROLS)
    throw Error("controlled registry capacity");
  const entries = Array.from(raw).map(actorEntry),
    avatars = new Set(),
    actors = new Set();
  let previous = 0n;
  for (const entry of entries) {
    const actor = entry.actor.object_id + ":" + entry.actor.generation;
    if (
      BigInt(entry.principal) <= previous ||
      avatars.has(entry.avatar_id) ||
      actors.has(actor)
    )
      throw Error("controlled registry order or duplicate identity");
    previous = BigInt(entry.principal);
    avatars.add(entry.avatar_id);
    actors.add(actor);
  }
  return entries;
}
export function controlledProposal(raw: unknown) {
  const v = fields(raw, ["kind", "status", "digest", "payload"]),
    p = fields(v.payload, [
      "binding",
      "batch_id",
      "predecessor",
      "result",
      "ticks_hex",
      "checkpoint_sha256",
      "effects_hex",
      "events_hex",
      "control_v2",
    ]);
  const { control_v2, ...base } = p;
  const parsed = proposal({ ...v, payload: base });
  const control = fields(control_v2, [
    "schema_version",
    "revision_before",
    "revision_after",
    "actor_registry_before_sha256",
    "actor_registry_after_sha256",
    "actors",
    "permission_before",
    "permission_after",
  ]);
  if (control.schema_version !== 2)
    throw Error("unsupported controlled proposal");
  return {
    ...parsed,
    payload: {
      ...parsed.payload,
      control_v2: {
        schema_version: 2 as const,
        revision_before: decimal(control.revision_before),
        revision_after: decimal(control.revision_after),
        actor_registry_before_sha256: hash(
          control.actor_registry_before_sha256,
        ),
        actor_registry_after_sha256: hash(control.actor_registry_after_sha256),
        actors: actorRegistry(control.actors),
        permission_before: permissionBinding(control.permission_before),
        permission_after: permissionBinding(control.permission_after),
      },
    },
  };
}
export function controlledSetup(raw: unknown) {
  const v = fields(raw, [
    "authority_id",
    "content_release",
    "protocol_release",
    "permission_binding",
    "actors",
  ]);
  const actors = actorRegistry(v.actors);
  if (actors.length)
    throw Error("new controlled lot must have empty actor registry");
  return {
    authority_id: hash(v.authority_id),
    content_release: hash(v.content_release),
    protocol_release: hash(v.protocol_release),
    permission_binding: permissionBinding(v.permission_binding),
    actors,
  };
}
export const controlKey = (e: ReservedAvatarExportV2) =>
  [
    e.authority_id,
    e.avatar_id,
    e.control_generation,
    e.admission_operation,
  ].join(":");
