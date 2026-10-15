import { decimal, fields, hash } from "../lot/contracts";

export const PORTABLE_LIMIT = 64 * 1024;
export const SOURCE_PIN = "4c6b3e8f5835b228723caea3c9f683c62f244f73";

/** Original ordinary CAS DB defaults, not AvatarState/Spawn defaults.
 * RegistrationHandler creates the selected appearance, zero budget and three suits.
 * SqlAvatars.Create omits motive/skill/job columns; fso_avatars.sql supplies them.
 * Its binary(32) motive literal starts with ASCII '0' followed by 31 NULs.
 * LotContainer.StateFromDB reads big-endian signed shorts: first motive is 12288.
 */
export function initialAvatarSource(
  createdAt: number,
  spec: { gender: string; body_key: string },
) {
  const gender = spec.gender;
  const person_data = Array<number>(27).fill(0);
  person_data[6] = gender === "female" ? 1 : 0;
  return {
    schema_version: 1 as const,
    source_pin: SOURCE_PIN,
    created_at_ms: createdAt,
    body_current: "0",
    default_suits: {
      daywear: spec.body_key,
      swimwear: gender === "female" ? "420906795021" : "5802500816909",
      sleepwear: gender === "female" ? "5587752452109" : "5789615915021",
    },
    motive_data: [12288, ...Array<number>(15).fill(0)],
    person_data,
    relationships: [] as { target: string; values: number[] }[],
    job_info: [] as {
      id: number;
      experience: number;
      level: number;
      sick_days: number;
      status_flags: number;
    }[],
    ignored_avatars: [] as string[],
    // These source tables have no row for a newly created ordinary avatar.
    // Persist the empty ownership/membership facts at creation, never infer them
    // from a missing migration row during admission.
    roommate_lots: [] as {
      lot_id: string;
      permissions_level: number;
      is_pending: number;
    }[],
    moderation_level: 0,
    mayor_nhood: null as string | null,
  };
}
export type AvatarSource = ReturnType<typeof initialAvatarSource>;

export function permissionBinding(value: unknown) {
  const v = fields(value, ["revision", "sha256"]);
  return { revision: decimal(v.revision), sha256: hash(v.sha256) };
}
export type PermissionBinding = ReturnType<typeof permissionBinding>;

export interface ReservedAvatarExportV2 {
  schema_version: 2;
  authority_id: string;
  account_id: string;
  avatar_id: string;
  lot_id: string;
  control_generation: string;
  admission_operation: string;
  reservation_digest: string;
  session_ref: string;
  account_revision: string;
  content_release: string;
  protocol_release: string;
  expires_at: number;
  permission_binding: PermissionBinding;
  permission_source: {
    registered_at_ms: number;
    created_at_ms: number;
    observed_at_ms: number;
    roommate_lots: AvatarSource["roommate_lots"];
    moderation_level: number;
    mayor_nhood: string | null;
  };
  source_revisions: {
    owned_assets: 1;
    wardrobe: string;
    ledger: string;
    progression: string;
    permissions: string;
  };
  portable_json: string;
  portable_sha256: string;
  export_digest: string;
}

export interface ControlLeaseV2 {
  schema_version: 2;
  status: "renewed";
  authority_id: string;
  account_id: string;
  avatar_id: string;
  control_generation: string;
  admission_operation: string;
  export_digest: string;
  session_ref: string;
  lease_revision: string;
  expires_at: number;
  renewal_operation: string;
}
