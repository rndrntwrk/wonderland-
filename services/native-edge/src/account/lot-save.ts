/** Source-pinned, private lot-departure persistence. This is not a portable write API.
 * LotContainer.StateToDb/SaveAvatar and SqlAvatars.UpdateAvatarLotSave define the
 * whitelist below. Missing source rows require migration, never CAS constructors.
 * AvatarSource remains schema 1; all departure metadata lives in new tables.
 */
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
} from "../lot/contracts";
import { avatar as avatarSpec, operation } from "./contracts";
import {
  SOURCE_PIN,
  type AvatarSource,
  type ReservedAvatarExportV2,
} from "./portable";
import type { ControlClaim } from "./control";

export const SOURCE_LOT_SAVE_LIMIT = 64 * 1024;
export const SOURCE_RELATIONSHIP_LIMIT = 256;
const RECEIPT_CAPACITY = 65536;
const nonzero = (v: unknown) => {
  const n = decimal(v);
  if (n === "0") throw Error("zero departure identity");
  return n;
};
function integer(v: unknown, min: number, max: number): number {
  if (!Number.isSafeInteger(v) || (v as number) < min || (v as number) > max)
    throw Error("invalid source integer");
  return v as number;
}
const u16 = (v: unknown) => integer(v, 0, 65535);
const short = (v: unknown) => integer(v, -32768, 32767);
export const sourceI16 = (value: number) => (value << 16) >> 16;
export const sourceU16 = (value: number) => value & 65535;
export const sourceLockColumn = (value: number) =>
  sourceU16(Math.trunc(short(value) / 100));
export const sourceLockReload = (value: number) => sourceI16(u16(value) * 100);
function bounded(raw: unknown, max = SOURCE_LOT_SAVE_LIMIT) {
  if (encode(raw).byteLength > max)
    throw Error("source lot-save byte capacity");
}
function targetId(v: unknown) {
  const id = nonzero(v);
  // Persistent relationship targets can be objects, not only source avatars.
  if (BigInt(id) > 4294967295n)
    throw Error("relationship target outside source range");
  return id;
}

export interface DepartureIdentityV1 {
  authority_id: string;
  account_id: string;
  avatar_id: string;
  lot_id: string;
  control_generation: string;
  session_ref: string;
  admission_operation: string;
  reservation_digest: string;
  export_digest: string;
  principal: string;
  actor: { object_id: number; generation: number };
}
export function departureIdentityV1(raw: unknown): DepartureIdentityV1 {
  const v = fields(raw, [
    "authority_id",
    "account_id",
    "avatar_id",
    "lot_id",
    "control_generation",
    "session_ref",
    "admission_operation",
    "reservation_digest",
    "export_digest",
    "principal",
    "actor",
  ]);
  const a = fields(v.actor, ["object_id", "generation"]);
  const avatar_id = nonzero(v.avatar_id);
  if (BigInt(avatar_id) > 16777215n)
    throw Error("avatar identity outside source range");
  return {
    authority_id: hash(v.authority_id),
    account_id: nonzero(v.account_id),
    avatar_id,
    lot_id: nonzero(v.lot_id),
    control_generation: nonzero(v.control_generation),
    session_ref: hash(v.session_ref),
    admission_operation: operation(v.admission_operation),
    reservation_digest: hash(v.reservation_digest),
    export_digest: hash(v.export_digest),
    principal: nonzero(v.principal),
    actor: {
      object_id: integer(a.object_id, 1, 32767),
      generation: integer(a.generation, 1, 4294967295),
    },
  };
}
export interface SourceAvatarColumnsV1 {
  motive_data_hex: string;
  skilllock: number;
  lock_body: number;
  lock_charisma: number;
  lock_cooking: number;
  lock_creativity: number;
  lock_logic: number;
  lock_mechanical: number;
  skill_cooking: number;
  skill_charisma: number;
  skill_mechanical: number;
  skill_creativity: number;
  skill_body: number;
  skill_logic: number;
  body: string;
  body_sleepwear: string;
  body_swimwear: string;
  body_current: string;
  current_job: number;
  is_ghost: number;
  ticker_death: number;
  ticker_gardener: number;
  ticker_maid: number;
  ticker_repairman: number;
}
const LOCKS = [
  "lock_body",
  "lock_charisma",
  "lock_cooking",
  "lock_creativity",
  "lock_logic",
  "lock_mechanical",
] as const;
const SKILLS = [
  "skill_cooking",
  "skill_charisma",
  "skill_mechanical",
  "skill_creativity",
  "skill_body",
  "skill_logic",
] as const;
const TICKERS = [
  "ticker_death",
  "ticker_gardener",
  "ticker_maid",
  "ticker_repairman",
] as const;
const SUITS = [
  "body",
  "body_sleepwear",
  "body_swimwear",
  "body_current",
] as const;
export function sourceAvatarColumnsV1(raw: unknown): SourceAvatarColumnsV1 {
  const v = fields(raw, [
    "motive_data_hex",
    "skilllock",
    ...LOCKS,
    ...SKILLS,
    ...SUITS,
    "current_job",
    "is_ghost",
    ...TICKERS,
  ]);
  if (
    typeof v.motive_data_hex !== "string" ||
    !/^[0-9a-f]{64}$/.test(v.motive_data_hex)
  )
    throw Error("invalid source motive bytes");
  if (v.motive_data_hex.slice(44, 48) !== "0000")
    throw Error("source SleepState must be zero");
  const result: Record<string, unknown> = {
    motive_data_hex: v.motive_data_hex,
    skilllock: integer(v.skilllock, 0, 255),
  };
  for (const k of [
    ...LOCKS,
    ...SKILLS,
    ...TICKERS,
    "current_job",
    "is_ghost",
  ] as const)
    result[k] = u16(v[k]);
  for (const k of SUITS) result[k] = decimal(v[k]);
  return result as unknown as SourceAvatarColumnsV1;
}
export interface SourceJobRowV1 {
  job_type: number;
  job_experience: number;
  job_level: number;
  job_sickdays: number;
  job_statusflags: number;
}
export function sourceJobRowV1(raw: unknown): SourceJobRowV1 {
  const v = fields(raw, [
    "job_type",
    "job_experience",
    "job_level",
    "job_sickdays",
    "job_statusflags",
  ]);
  return {
    job_type: integer(v.job_type, 1, 65535),
    job_experience: u16(v.job_experience),
    job_level: u16(v.job_level),
    job_sickdays: u16(v.job_sickdays),
    job_statusflags: u16(v.job_statusflags),
  };
}
function relationships(
  raw: unknown,
  sorted: boolean,
): AvatarSource["relationships"] {
  if (!Array.isArray(raw) || raw.length > SOURCE_RELATIONSHIP_LIMIT)
    throw Error("source relationship capacity");
  const seen = new Set<string>();
  let previous = 0n;
  return Array.from(raw, (entry) => {
    const v = fields(entry, ["target", "values"]),
      target = targetId(v.target);
    if (seen.has(target) || (sorted && BigInt(target) <= previous))
      throw Error("source relationship order or duplicate");
    seen.add(target);
    previous = BigInt(target);
    if (!Array.isArray(v.values) || v.values.length > SOURCE_RELATIONSHIP_LIMIT)
      throw Error("source relationship value capacity");
    return { target, values: Array.from(v.values, short) };
  });
}
export interface SourceAvatarLotSaveV1 {
  schema_version: 1;
  source_pin: string;
  identity: DepartureIdentityV1;
  departure_operation: string;
  save_sequence: "1";
  captured_tick: string;
  capture_reason: "first_leave_tick";
  avatar_columns: SourceAvatarColumnsV1;
  current_job_row: SourceJobRowV1 | null;
  changed_relationships: AvatarSource["relationships"];
}
export function sourceAvatarLotSaveV1(raw: unknown): SourceAvatarLotSaveV1 {
  bounded(raw);
  const v = fields(raw, [
    "schema_version",
    "source_pin",
    "identity",
    "departure_operation",
    "save_sequence",
    "captured_tick",
    "capture_reason",
    "avatar_columns",
    "current_job_row",
    "changed_relationships",
  ]);
  if (
    v.schema_version !== 1 ||
    v.source_pin !== SOURCE_PIN ||
    v.save_sequence !== "1" ||
    v.capture_reason !== "first_leave_tick"
  )
    throw Error("unsupported source lot-save profile");
  const avatar_columns = sourceAvatarColumnsV1(v.avatar_columns);
  const current_job_row =
    v.current_job_row === null ? null : sourceJobRowV1(v.current_job_row);
  if (
    current_job_row &&
    current_job_row.job_type !== avatar_columns.current_job
  )
    throw Error("source current-job mismatch");
  return {
    schema_version: 1,
    source_pin: SOURCE_PIN,
    identity: departureIdentityV1(v.identity),
    departure_operation: operation(v.departure_operation),
    save_sequence: "1",
    captured_tick: nonzero(v.captured_tick),
    capture_reason: "first_leave_tick",
    avatar_columns,
    current_job_row,
    changed_relationships: relationships(v.changed_relationships, true),
  };
}
/** Exact unchecked C# casts. Call with actual persistent_state() data, not CAS or
 * displayed body state. Native has already normalized dynamic/naked outfits to
 * body_for_persistence. This helper intentionally has no appearance fallback.
 */
export function sourceLotSaveColumns(input: {
  motive_data: number[];
  person_data: number[];
  default_suits: AvatarSource["default_suits"];
  body_for_persistence: string;
}): SourceAvatarColumnsV1 {
  if (input.motive_data.length !== 16 || input.person_data.length !== 27)
    throw Error("invalid portable source lengths");
  const motives = Array.from(input.motive_data, short),
    person = Array.from(input.person_data, short);
  motives[11] = 0; // VMNetAvatarPersistState.Save, not an admission default.
  const v: Record<string, unknown> = {
    motive_data_hex: motives
      .map((n) => sourceU16(n).toString(16).padStart(4, "0"))
      .join(""),
    skilllock: person[10] & 255,
    body: decimal(input.default_suits.daywear),
    body_sleepwear: decimal(input.default_suits.sleepwear),
    body_swimwear: decimal(input.default_suits.swimwear),
    body_current: decimal(input.body_for_persistence),
    current_job: sourceU16(person[21]),
    is_ghost: sourceU16(person[7]),
  };
  LOCKS.forEach((k, i) => (v[k] = sourceLockColumn(person[i + 11])));
  SKILLS.forEach((k, i) => (v[k] = sourceU16(person[i])));
  TICKERS.forEach((k, i) => (v[k] = sourceU16(person[i + 17])));
  return sourceAvatarColumnsV1(v);
}
export function sourceCurrentJobRow(
  personData: number[],
  jobInfo: AvatarSource["job_info"],
): SourceJobRowV1 | null {
  if (personData.length !== 27) throw Error("invalid portable source lengths");
  const signed = short(personData[21]),
    current = sourceU16(signed);
  const jobs = sourceJobs(jobInfo),
    info = jobs.find((row) => row.id === signed);
  if (current === 0 || !info) return null;
  return {
    job_type: current,
    job_experience: sourceU16(info.experience),
    job_level: sourceU16(info.level),
    job_sickdays: sourceU16(info.sick_days),
    job_statusflags: sourceU16(info.status_flags),
  };
}

/** Origin is fixed by coordinator journal commit, never a URL supplied by caller.
 * captured_tick identifies the observation tick (detached_tick for detach proof).
 */
export interface DepartureCommitOriginV1 {
  binding: Binding;
  content_release: string;
  protocol_release: string;
  batch_id: string;
  proposal_digest: string;
  predecessor: Cursor;
  result: Cursor;
  checkpoint_sha256: string;
  captured_tick: string;
  event_ordinal: number;
}
export function departureCommitOriginV1(raw: unknown): DepartureCommitOriginV1 {
  const v = fields(raw, [
    "binding",
    "content_release",
    "protocol_release",
    "batch_id",
    "proposal_digest",
    "predecessor",
    "result",
    "checkpoint_sha256",
    "captured_tick",
    "event_ordinal",
  ]);
  const predecessor = cursor(v.predecessor),
    result = cursor(v.result),
    captured_tick = nonzero(v.captured_tick);
  const delta =
    BigInt(result.completed_tick) - BigInt(predecessor.completed_tick);
  if (
    predecessor.lot_id !== result.lot_id ||
    predecessor.authority_epoch !== result.authority_epoch ||
    delta < 1n ||
    delta > 16n ||
    BigInt(captured_tick) <= BigInt(predecessor.completed_tick) ||
    BigInt(captured_tick) > BigInt(result.completed_tick)
  )
    throw Error("departure origin cursor mismatch");
  return {
    binding: binding(v.binding),
    content_release: hash(v.content_release),
    protocol_release: hash(v.protocol_release),
    batch_id: nonzero(v.batch_id),
    proposal_digest: hash(v.proposal_digest),
    predecessor,
    result,
    checkpoint_sha256: hash(v.checkpoint_sha256),
    captured_tick,
    event_ordinal: integer(v.event_ordinal, 0, 65535),
  };
}
export interface SaveProofReferenceV1 {
  schema_version: 1;
  identity: DepartureIdentityV1;
  departure_operation: string;
  save_sha256: string;
  origin: DepartureCommitOriginV1;
}
export function saveProofReferenceV1(raw: unknown): SaveProofReferenceV1 {
  const v = fields(raw, [
    "schema_version",
    "identity",
    "departure_operation",
    "save_sha256",
    "origin",
  ]);
  if (v.schema_version !== 1) throw Error("unsupported save reference");
  const identity = departureIdentityV1(v.identity),
    origin = departureCommitOriginV1(v.origin);
  if (identity.lot_id !== origin.result.lot_id)
    throw Error("departure origin lot mismatch");
  return {
    schema_version: 1,
    identity,
    departure_operation: operation(v.departure_operation),
    save_sha256: hash(v.save_sha256),
    origin,
  };
}
export interface CommittedAvatarSaveV1 extends SaveProofReferenceV1 {
  status: "committed_source_save";
  save_record: SourceAvatarLotSaveV1;
  source_baseline: { progression: string; wardrobe: string };
}
export function committedAvatarSaveV1(raw: unknown): CommittedAvatarSaveV1 {
  bounded(raw, SOURCE_LOT_SAVE_LIMIT + 16 * 1024);
  const v = fields(raw, [
    "schema_version",
    "status",
    "identity",
    "departure_operation",
    "save_sha256",
    "origin",
    "save_record",
    "source_baseline",
  ]);
  const {
    status,
    save_record: rawRecord,
    source_baseline: rawBaseline,
    ...ref
  } = v;
  if (status !== "committed_source_save")
    throw Error("source save is not committed");
  const reference = saveProofReferenceV1(ref),
    save_record = sourceAvatarLotSaveV1(rawRecord);
  const baseline = fields(rawBaseline, ["progression", "wardrobe"]);
  if (
    !equal(reference.identity, save_record.identity) ||
    reference.departure_operation !== save_record.departure_operation ||
    reference.origin.captured_tick !== save_record.captured_tick
  )
    throw Error("committed save identity mismatch");
  return {
    ...reference,
    status,
    save_record,
    source_baseline: {
      progression: hash(baseline.progression),
      wardrobe: hash(baseline.wardrobe),
    },
  };
}
export interface DetachProofReferenceV1 {
  schema_version: 1;
  identity: DepartureIdentityV1;
  departure_operation: string;
  detach_sha256: string;
  origin: DepartureCommitOriginV1;
}
export function detachProofReferenceV1(raw: unknown): DetachProofReferenceV1 {
  const v = fields(raw, [
    "schema_version",
    "identity",
    "departure_operation",
    "detach_sha256",
    "origin",
  ]);
  if (v.schema_version !== 1) throw Error("unsupported detach reference");
  const identity = departureIdentityV1(v.identity),
    origin = departureCommitOriginV1(v.origin);
  if (identity.lot_id !== origin.result.lot_id)
    throw Error("departure origin lot mismatch");
  return {
    schema_version: 1,
    identity,
    departure_operation: operation(v.departure_operation),
    detach_sha256: hash(v.detach_sha256),
    origin,
  };
}
/** detach_sha256 hashes this complete receipt. It is carried only in its reference,
 * avoiding a circular self-hash. Actor absence and retained save ancestry across
 * host takeover are established by the coordinator, not by matching host epochs.
 */
export interface CommittedAvatarDetachV1 {
  schema_version: 1;
  status: "committed_actor_detach";
  identity: DepartureIdentityV1;
  departure_operation: string;
  source_save: SaveProofReferenceV1;
  detached_tick: string;
  removal_reason: "source_leave" | "forced_cleanup";
  origin: DepartureCommitOriginV1;
  control_revision_before: string;
  control_revision_after: string;
  actor_registry_before_sha256: string;
  actor_registry_after_sha256: string;
}
export function committedAvatarDetachV1(raw: unknown): CommittedAvatarDetachV1 {
  bounded(raw, 16 * 1024);
  const v = fields(raw, [
    "schema_version",
    "status",
    "identity",
    "departure_operation",
    "source_save",
    "detached_tick",
    "removal_reason",
    "origin",
    "control_revision_before",
    "control_revision_after",
    "actor_registry_before_sha256",
    "actor_registry_after_sha256",
  ]);
  if (
    v.schema_version !== 1 ||
    v.status !== "committed_actor_detach" ||
    !["source_leave", "forced_cleanup"].includes(v.removal_reason as string)
  )
    throw Error("unsupported committed detach");
  const identity = departureIdentityV1(v.identity),
    source_save = saveProofReferenceV1(v.source_save),
    origin = departureCommitOriginV1(v.origin),
    detached_tick = nonzero(v.detached_tick),
    departure_operation = operation(v.departure_operation);
  const before = decimal(v.control_revision_before),
    after = nonzero(v.control_revision_after);
  if (
    !equal(identity, source_save.identity) ||
    departure_operation !== source_save.departure_operation ||
    identity.lot_id !== origin.result.lot_id ||
    detached_tick !== origin.captured_tick ||
    BigInt(detached_tick) < BigInt(source_save.origin.captured_tick) ||
    BigInt(after) <= BigInt(before) ||
    origin.binding.source_pin !== source_save.origin.binding.source_pin ||
    origin.content_release !== source_save.origin.content_release ||
    origin.protocol_release !== source_save.origin.protocol_release ||
    v.actor_registry_before_sha256 === v.actor_registry_after_sha256
  )
    throw Error("committed detach identity mismatch");
  return {
    schema_version: 1,
    status: "committed_actor_detach",
    identity,
    departure_operation,
    source_save,
    detached_tick,
    removal_reason:
      v.removal_reason as CommittedAvatarDetachV1["removal_reason"],
    origin,
    control_revision_before: before,
    control_revision_after: after,
    actor_registry_before_sha256: hash(v.actor_registry_before_sha256),
    actor_registry_after_sha256: hash(v.actor_registry_after_sha256),
  };
}
export function saveReference(
  receipt: CommittedAvatarSaveV1,
): SaveProofReferenceV1 {
  const {
    status: _status,
    save_record: _record,
    source_baseline: _baseline,
    ...reference
  } = receipt;
  return reference;
}
/** Required trusted dependency. Production must resolve the configured LOTS DO
 * namespace with its deterministic lot key and return the committed journal
 * receipt, or throw. The reference contains no URL, namespace or service choice.
 * An echo of the supplied reference is never sufficient evidence.
 */
export interface TrustedDepartureReceiptReader {
  verifyCommittedSave(reference: SaveProofReferenceV1): Promise<unknown>;
  verifyCommittedDetach(reference: DetachProofReferenceV1): Promise<unknown>;
}
export interface AppliedAvatarLotSaveReceiptV1 {
  schema_version: 1;
  status: "applied_source_save";
  committed_save: CommittedAvatarSaveV1;
  source_progression_before: string;
  source_progression_after: string;
}
export interface ReleasedSavedAvatarControlReceiptV1 {
  schema_version: 1;
  status: "released_saved_control";
  identity: DepartureIdentityV1;
  departure_operation: string;
  save_sha256: string;
  account_save_receipt_sha256: string;
  detach_sha256: string;
  committed_detach: CommittedAvatarDetachV1;
}

function sourceJobs(raw: unknown): AvatarSource["job_info"] {
  if (!Array.isArray(raw) || raw.length > 256)
    throw Error("source job capacity");
  const seen = new Set<number>();
  return Array.from(raw, (entry) => {
    const v = fields(entry, [
        "id",
        "experience",
        "level",
        "sick_days",
        "status_flags",
      ]),
      id = short(v.id);
    if (seen.has(id)) throw Error("duplicate source job");
    seen.add(id);
    return {
      id,
      experience: short(v.experience),
      level: short(v.level),
      sick_days: short(v.sick_days),
      status_flags: short(v.status_flags),
    };
  });
}
/** Validate existing persisted schema 1 completely before a selective merge. A
 * malformed/missing migration cannot be repaired by creating default avatars.
 */
export function existingAvatarSourceV1(raw: unknown): AvatarSource {
  bounded(raw);
  const v = fields(raw, [
    "schema_version",
    "source_pin",
    "created_at_ms",
    "body_current",
    "default_suits",
    "motive_data",
    "person_data",
    "relationships",
    "job_info",
    "ignored_avatars",
    "roommate_lots",
    "moderation_level",
    "mayor_nhood",
  ]);
  if (v.schema_version !== 1 || v.source_pin !== SOURCE_PIN)
    throw Error("source avatar migration required");
  integer(v.created_at_ms, 0, Number.MAX_SAFE_INTEGER);
  decimal(v.body_current);
  const suits = fields(v.default_suits, ["daywear", "swimwear", "sleepwear"]);
  for (const value of Object.values(suits)) decimal(value);
  if (
    !Array.isArray(v.motive_data) ||
    v.motive_data.length !== 16 ||
    !Array.isArray(v.person_data) ||
    v.person_data.length !== 27
  )
    throw Error("invalid source avatar lengths");
  Array.from(v.motive_data, short);
  Array.from(v.person_data, short);
  relationships(v.relationships, false);
  sourceJobs(v.job_info);
  if (
    !Array.isArray(v.ignored_avatars) ||
    v.ignored_avatars.length > 256 ||
    new Set(v.ignored_avatars).size !== v.ignored_avatars.length
  )
    throw Error("invalid ignored-avatar source");
  Array.from(v.ignored_avatars, targetId);
  if (!Array.isArray(v.roommate_lots) || v.roommate_lots.length > 256)
    throw Error("invalid source memberships");
  const lots = new Set<string>();
  for (const raw of v.roommate_lots) {
    const row = fields(raw, ["lot_id", "permissions_level", "is_pending"]),
      id = nonzero(row.lot_id);
    if (lots.has(id)) throw Error("duplicate source membership");
    lots.add(id);
    integer(row.permissions_level, 0, 2);
    integer(row.is_pending, 0, 1);
  }
  integer(v.moderation_level, 0, 255);
  if (v.mayor_nhood !== null) nonzero(v.mayor_nhood);
  return JSON.parse(canonical(raw)) as AvatarSource;
}
export function mergeSourceAvatarLotSaveV1(
  existing: unknown,
  rawSave: unknown,
): AvatarSource {
  const source = existingAvatarSourceV1(existing),
    save = sourceAvatarLotSaveV1(rawSave),
    columns = save.avatar_columns;
  source.motive_data = Array.from({ length: 16 }, (_, i) =>
    sourceI16(parseInt(columns.motive_data_hex.slice(i * 4, i * 4 + 4), 16)),
  );
  SKILLS.forEach((key, i) => (source.person_data[i] = sourceI16(columns[key])));
  source.person_data[7] = sourceI16(columns.is_ghost);
  source.person_data[10] = columns.skilllock; // Export still derives available locks from age.
  LOCKS.forEach(
    (key, i) => (source.person_data[i + 11] = sourceLockReload(columns[key])),
  );
  TICKERS.forEach(
    (key, i) => (source.person_data[i + 17] = sourceI16(columns[key])),
  );
  source.person_data[21] = sourceI16(columns.current_job);
  source.default_suits = {
    daywear: columns.body,
    swimwear: columns.body_swimwear,
    sleepwear: columns.body_sleepwear,
  };
  source.body_current = columns.body_current;
  if (save.current_job_row) {
    const row = save.current_job_row,
      id = sourceI16(row.job_type),
      index = source.job_info.findIndex((j) => j.id === id);
    const job = {
      id,
      experience: sourceI16(row.job_experience),
      level: sourceI16(row.job_level),
      sick_days: sourceI16(row.job_sickdays),
      status_flags: sourceI16(row.job_statusflags),
    };
    if (index < 0) source.job_info.push(job);
    else source.job_info[index] = job;
  }
  // SqlRelationships.UpdateMany upserts individual indexes. A shorter vector
  // cannot erase trailing columns, and an empty vector writes no relationship.
  for (const changed of save.changed_relationships) {
    if (!changed.values.length) continue;
    let row = source.relationships.find((r) => r.target === changed.target);
    if (!row) {
      row = { target: changed.target, values: [] };
      source.relationships.push(row);
    }
    changed.values.forEach((value, index) => (row!.values[index] = value));
  }
  return existingAvatarSourceV1(source); // Capacity is fail-closed, never truncation.
}

type Storage = Pick<DurableObjectStorage, "sql" | "transactionSync">;
type ClaimRow = {
  avatar_id: string;
  account_id: string;
  session_hash: string;
  session_revision: string;
  operation: string;
  digest: string;
  claim: string;
};
type ExportRow = {
  operation: string;
  source_json: string;
  export_json: string;
};
type OutfitRow = {
  id: string;
  asset_key: string;
  suit_type: number;
  source: string;
  purchase_price: string;
  sale_price: string;
};
type Snapshot = {
  authority: string;
  claim: ClaimRow;
  frozen: ExportRow;
  source_json: string;
  spec: string;
  initialized: number;
  registered_at_ms: number;
  outfits: OutfitRow[];
};
function controlKey(identity: DepartureIdentityV1) {
  return canonical([
    identity.authority_id,
    identity.avatar_id,
    identity.control_generation,
  ]);
}
function parsedClaim(raw: unknown): ControlClaim {
  const v = fields(raw, [
    "version",
    "account_id",
    "avatar_id",
    "lot_id",
    "generation",
    "content_release",
    "protocol_release",
    "expires_at",
    "state",
  ]);
  if (
    v.version !== 1 ||
    !["reserved", "fence_pending"].includes(v.state as string)
  )
    throw Error("invalid departure control claim");
  return {
    version: 1,
    account_id: nonzero(v.account_id),
    avatar_id: nonzero(v.avatar_id),
    lot_id: nonzero(v.lot_id),
    generation: nonzero(v.generation),
    content_release: hash(v.content_release),
    protocol_release: hash(v.protocol_release),
    expires_at: integer(v.expires_at, 1, Number.MAX_SAFE_INTEGER),
    state: v.state as ControlClaim["state"],
  };
}

/** Uses AccountAuthority's actual SQLite source/claim/export tables. New metadata
 * is append-only. No public route should be wired without the required trusted
 * coordinator reader; constructor refusal makes an absent binding fail closed.
 */
export class AccountLotSaveStore {
  private readonly storage: Storage;
  private readonly reader: TrustedDepartureReceiptReader;
  constructor(storage: Storage, reader: TrustedDepartureReceiptReader) {
    this.storage = storage;
    this.reader = reader;
    if (
      !reader ||
      typeof reader.verifyCommittedSave !== "function" ||
      typeof reader.verifyCommittedDetach !== "function"
    )
      throw Error("trusted departure receipt reader required");
    storage.sql.exec(`
      CREATE TABLE IF NOT EXISTS avatar_lot_save_receipts(
        control_key TEXT PRIMARY KEY,account_id TEXT NOT NULL,avatar_id TEXT NOT NULL,control_generation TEXT NOT NULL,
        departure_operation TEXT NOT NULL,reference_json TEXT NOT NULL,receipt_json TEXT NOT NULL,
        UNIQUE(account_id,departure_operation));
      CREATE TABLE IF NOT EXISTS avatar_control_release_receipts(
        control_key TEXT PRIMARY KEY,account_id TEXT NOT NULL,avatar_id TEXT NOT NULL,control_generation TEXT NOT NULL,
        departure_operation TEXT NOT NULL,reference_json TEXT NOT NULL,receipt_json TEXT NOT NULL,
        UNIQUE(account_id,departure_operation));
      CREATE TABLE IF NOT EXISTS avatar_control_operation_tombstones(
        account_id TEXT NOT NULL,operation TEXT NOT NULL,avatar_id TEXT NOT NULL,control_generation TEXT NOT NULL,
        reservation_digest TEXT NOT NULL,departure_operation TEXT NOT NULL,PRIMARY KEY(account_id,operation));
    `);
  }
  private saved(
    reference: SaveProofReferenceV1,
  ): AppliedAvatarLotSaveReceiptV1 | null {
    const row = this.storage.sql
      .exec<{
        reference_json: string;
        receipt_json: string;
      }>("SELECT reference_json,receipt_json FROM avatar_lot_save_receipts WHERE control_key=?", controlKey(reference.identity))
      .toArray()[0];
    if (!row) return null;
    if (row.reference_json !== canonical(reference))
      throw Error("source save identity reused");
    return JSON.parse(row.receipt_json) as AppliedAvatarLotSaveReceiptV1;
  }
  private released(
    reference: DetachProofReferenceV1,
  ): ReleasedSavedAvatarControlReceiptV1 | null {
    const row = this.storage.sql
      .exec<{
        reference_json: string;
        receipt_json: string;
      }>("SELECT reference_json,receipt_json FROM avatar_control_release_receipts WHERE control_key=?", controlKey(reference.identity))
      .toArray()[0];
    if (!row) return null;
    if (row.reference_json !== canonical(reference))
      throw Error("control release identity reused");
    return JSON.parse(row.receipt_json) as ReleasedSavedAvatarControlReceiptV1;
  }
  lookupSavedAvatarControlV1(
    rawIdentity: unknown,
    rawOperation: unknown,
  ): AppliedAvatarLotSaveReceiptV1 | null {
    const identity = departureIdentityV1(rawIdentity),
      op = operation(rawOperation);
    const row = this.storage.sql
      .exec<{
        receipt_json: string;
      }>("SELECT receipt_json FROM avatar_lot_save_receipts WHERE control_key=? AND departure_operation=?", controlKey(identity), op)
      .toArray()[0];
    if (!row) return null;
    const receipt = JSON.parse(
      row.receipt_json,
    ) as AppliedAvatarLotSaveReceiptV1;
    if (!equal(receipt.committed_save.identity, identity))
      throw Error("source save lookup identity mismatch");
    return receipt;
  }
  lookupReleasedAvatarControlV1(
    rawIdentity: unknown,
    rawOperation: unknown,
  ): ReleasedSavedAvatarControlReceiptV1 | null {
    const identity = departureIdentityV1(rawIdentity),
      op = operation(rawOperation);
    const row = this.storage.sql
      .exec<{
        receipt_json: string;
      }>("SELECT receipt_json FROM avatar_control_release_receipts WHERE control_key=? AND departure_operation=?", controlKey(identity), op)
      .toArray()[0];
    if (!row) return null;
    const receipt = JSON.parse(
      row.receipt_json,
    ) as ReleasedSavedAvatarControlReceiptV1;
    if (!equal(receipt.identity, identity))
      throw Error("control release lookup identity mismatch");
    return receipt;
  }
  /** Wire inside reserveAvatarControl's transaction before creating a new claim.
   * Active exact retries are still served by the existing current-claim branch.
   */
  assertReservationOperationUnused(rawAccount: unknown, rawOperation: unknown) {
    const account = nonzero(rawAccount),
      op = operation(rawOperation);
    if (
      this.storage.sql
        .exec(
          "SELECT operation FROM avatar_control_operation_tombstones WHERE account_id=? AND operation=?",
          account,
          op,
        )
        .toArray().length
    )
      throw Error("reservation operation already terminal");
  }
  private readSnapshot(identity: DepartureIdentityV1): Snapshot {
    const authority = this.storage.sql
      .exec<{
        value: string;
      }>("SELECT value FROM account_meta WHERE key='admission_authority_v2'")
      .toArray()[0];
    const claim = this.storage.sql
      .exec<ClaimRow>(
        "SELECT * FROM avatar_control_claims WHERE avatar_id=?",
        identity.avatar_id,
      )
      .toArray()[0];
    const frozen = this.storage.sql
      .exec<ExportRow>(
        "SELECT operation,source_json,export_json FROM avatar_control_exports WHERE avatar_id=?",
        identity.avatar_id,
      )
      .toArray()[0];
    const source = this.storage.sql
      .exec<{
        source_json: string;
      }>("SELECT source_json FROM avatar_source_state WHERE avatar_id=?", identity.avatar_id)
      .toArray()[0];
    const owned = this.storage.sql
      .exec<{
        spec: string;
      }>("SELECT spec FROM avatars WHERE id=? AND account_id=?", identity.avatar_id, identity.account_id)
      .toArray()[0];
    const initialized = this.storage.sql
      .exec<{
        version: number;
      }>("SELECT version FROM avatar_ownership_initializations WHERE avatar_id=?", identity.avatar_id)
      .toArray()[0];
    const accountSource = this.storage.sql
      .exec<{
        registered_at_ms: number;
      }>("SELECT registered_at_ms FROM account_source_state WHERE account_id=?", identity.account_id)
      .toArray()[0];
    const lease = this.storage.sql
      .exec(
        "SELECT avatar_id FROM avatar_control_leases WHERE avatar_id=?",
        identity.avatar_id,
      )
      .toArray()[0];
    if (
      !authority ||
      !claim ||
      !frozen ||
      !source ||
      !owned ||
      initialized?.version !== 1 ||
      !accountSource ||
      !lease
    )
      throw Error("departure source or control migration required");
    const outfits = this.storage.sql
      .exec<OutfitRow>(
        "SELECT id,asset_key,suit_type,source,purchase_price,sale_price FROM avatar_outfits WHERE avatar_id=? ORDER BY length(id),id LIMIT 257",
        identity.avatar_id,
      )
      .toArray();
    if (outfits.length > 256) throw Error("departure wardrobe capacity");
    return {
      authority: authority.value,
      claim,
      frozen,
      source_json: source.source_json,
      spec: owned.spec,
      initialized: initialized.version,
      registered_at_ms: accountSource.registered_at_ms,
      outfits,
    };
  }
  private validateCurrent(snapshot: Snapshot, reference: SaveProofReferenceV1) {
    const { identity: id, origin } = reference,
      row = snapshot.claim,
      claim = parsedClaim(JSON.parse(row.claim));
    const exported = JSON.parse(
      snapshot.frozen.export_json,
    ) as ReservedAvatarExportV2;
    const spec = avatarSpec(JSON.parse(snapshot.spec));
    const frozen = JSON.parse(snapshot.frozen.source_json) as {
      source: unknown;
      outfits: unknown;
      spec: unknown;
      claim: unknown;
    };
    if (
      snapshot.authority !== id.authority_id ||
      row.avatar_id !== id.avatar_id ||
      row.account_id !== id.account_id ||
      row.session_hash !== id.session_ref ||
      row.operation !== id.admission_operation ||
      row.digest !== id.reservation_digest ||
      claim.account_id !== id.account_id ||
      claim.avatar_id !== id.avatar_id ||
      claim.lot_id !== id.lot_id ||
      claim.generation !== id.control_generation ||
      claim.content_release !== origin.content_release ||
      claim.protocol_release !== origin.protocol_release ||
      spec.release !== origin.content_release ||
      snapshot.frozen.operation !== id.admission_operation ||
      exported.schema_version !== 2 ||
      row.session_revision !== exported.account_revision ||
      exported.authority_id !== id.authority_id ||
      exported.account_id !== id.account_id ||
      exported.avatar_id !== id.avatar_id ||
      exported.lot_id !== id.lot_id ||
      exported.control_generation !== id.control_generation ||
      exported.session_ref !== id.session_ref ||
      exported.admission_operation !== id.admission_operation ||
      exported.reservation_digest !== id.reservation_digest ||
      exported.export_digest !== id.export_digest ||
      exported.content_release !== origin.content_release ||
      exported.protocol_release !== origin.protocol_release
    )
      throw Error("departure current control or export mismatch");
    existingAvatarSourceV1(frozen.source);
    const source = existingAvatarSourceV1(JSON.parse(snapshot.source_json));
    // Export source_json is the frozen sourceIdentityV2, not portable_json. It
    // preserves the DB skilllock baseline instead of the derived admission age.
    if (!equal(source, frozen.source) || !equal(spec, frozen.spec))
      throw Error("departure source baseline changed");
    hash(exported.source_revisions.progression);
    hash(exported.source_revisions.wardrobe);
    return { claim, exported, frozen, source, spec };
  }
  private capacity(
    table:
      | "avatar_lot_save_receipts"
      | "avatar_control_release_receipts"
      | "avatar_control_operation_tombstones",
  ) {
    if (
      this.storage.sql
        .exec<{ n: number }>(`SELECT COUNT(*) AS n FROM ${table}`)
        .one().n >= RECEIPT_CAPACITY
    )
      throw Error("departure receipt retention capacity");
  }
  private retainOperation(id: DepartureIdentityV1, op: string) {
    const existing = this.storage.sql
      .exec<{
        avatar_id: string;
        control_generation: string;
        reservation_digest: string;
        departure_operation: string;
      }>("SELECT avatar_id,control_generation,reservation_digest,departure_operation FROM avatar_control_operation_tombstones WHERE account_id=? AND operation=?", id.account_id, id.admission_operation)
      .toArray()[0];
    const value = {
      avatar_id: id.avatar_id,
      control_generation: id.control_generation,
      reservation_digest: id.reservation_digest,
      departure_operation: op,
    };
    if (existing) {
      if (!equal(existing, value))
        throw Error("reservation operation identity reused");
      return;
    }
    this.capacity("avatar_control_operation_tombstones");
    this.storage.sql.exec(
      "INSERT INTO avatar_control_operation_tombstones(account_id,operation,avatar_id,control_generation,reservation_digest,departure_operation) VALUES(?,?,?,?,?,?)",
      id.account_id,
      id.admission_operation,
      id.avatar_id,
      id.control_generation,
      id.reservation_digest,
      op,
    );
  }
  async applyCommittedAvatarLotSaveV1(
    rawReference: unknown,
  ): Promise<AppliedAvatarLotSaveReceiptV1> {
    const reference = saveProofReferenceV1(rawReference);
    // An exact old ack retry is valid after release and even after re-admission.
    const old = this.saved(reference);
    if (old) return old;
    const committed = committedAvatarSaveV1(
      await this.reader.verifyCommittedSave(reference),
    );
    if (
      !equal(saveReference(committed), reference) ||
      (await digest(encode(committed.save_record))) !== reference.save_sha256
    )
      throw Error("trusted committed save mismatch");
    const snapshot = this.readSnapshot(reference.identity),
      current = this.validateCurrent(snapshot, reference);
    const before = await digest(encode(current.source)),
      wardrobe = await digest(encode(current.frozen.outfits));
    if (
      before !== committed.source_baseline.progression ||
      before !== current.exported.source_revisions.progression ||
      wardrobe !== committed.source_baseline.wardrobe ||
      wardrobe !== current.exported.source_revisions.wardrobe
    )
      throw Error("committed save baseline mismatch");
    const columns = committed.save_record.avatar_columns;
    for (const [asset, suit] of [
      [columns.body, 0],
      [columns.body_swimwear, 2],
      [columns.body_sleepwear, 5],
    ] as const)
      if (
        !snapshot.outfits.some(
          (outfit) => outfit.asset_key === asset && outfit.suit_type === suit,
        )
      )
        throw Error("saved default outfit is not owned");
    // body_current can be a source content outfit outside purchased wardrobe.
    const merged = mergeSourceAvatarLotSaveV1(
      current.source,
      committed.save_record,
    );
    const receipt: AppliedAvatarLotSaveReceiptV1 = {
      schema_version: 1,
      status: "applied_source_save",
      committed_save: committed,
      source_progression_before: before,
      source_progression_after: await digest(encode(merged)),
    };
    return this.storage.transactionSync(() => {
      const prior = this.saved(reference);
      if (prior) return prior;
      const latest = this.readSnapshot(reference.identity);
      // Every value used above is re-read synchronously after all awaited work.
      // No live session is required and expiry is deliberately not authorization.
      if (!equal(snapshot, latest))
        throw Error(
          "departure source or control changed during proof verification",
        );
      this.validateCurrent(latest, reference);
      this.capacity("avatar_lot_save_receipts");
      this.retainOperation(reference.identity, reference.departure_operation);
      this.storage.sql.exec(
        "UPDATE avatar_source_state SET source_json=? WHERE avatar_id=?",
        canonical(merged),
        reference.identity.avatar_id,
      );
      this.storage.sql.exec(
        "UPDATE avatars SET spec=? WHERE id=? AND account_id=?",
        canonical({ ...current.spec, body_key: columns.body }),
        reference.identity.avatar_id,
        reference.identity.account_id,
      );
      this.storage.sql.exec(
        "UPDATE avatar_control_claims SET claim=? WHERE avatar_id=?",
        canonical({ ...current.claim, state: "fence_pending" }),
        reference.identity.avatar_id,
      );
      this.storage.sql.exec(
        "INSERT INTO avatar_lot_save_receipts(control_key,account_id,avatar_id,control_generation,departure_operation,reference_json,receipt_json) VALUES(?,?,?,?,?,?,?)",
        controlKey(reference.identity),
        reference.identity.account_id,
        reference.identity.avatar_id,
        reference.identity.control_generation,
        reference.departure_operation,
        canonical(reference),
        canonical(receipt),
      );
      return receipt;
    });
  }
  async releaseSavedAvatarControlV1(
    rawReference: unknown,
  ): Promise<ReleasedSavedAvatarControlReceiptV1> {
    const reference = detachProofReferenceV1(rawReference),
      old = this.released(reference);
    if (old) return old;
    const committed = committedAvatarDetachV1(
      await this.reader.verifyCommittedDetach(reference),
    );
    if (
      !equal(committed.identity, reference.identity) ||
      committed.departure_operation !== reference.departure_operation ||
      !equal(committed.origin, reference.origin) ||
      (await digest(encode(committed))) !== reference.detach_sha256
    )
      throw Error("trusted committed detach mismatch");
    const saved = this.saved(committed.source_save);
    if (!saved) throw Error("committed account save required before release");
    const receipt: ReleasedSavedAvatarControlReceiptV1 = {
      schema_version: 1,
      status: "released_saved_control",
      identity: reference.identity,
      departure_operation: reference.departure_operation,
      save_sha256: committed.source_save.save_sha256,
      account_save_receipt_sha256: await digest(encode(saved)),
      detach_sha256: reference.detach_sha256,
      committed_detach: committed,
    };
    return this.storage.transactionSync(() => {
      const prior = this.released(reference);
      if (prior) return prior;
      // Independent durable account proof, re-read after trusted detach lookup.
      if (!equal(this.saved(committed.source_save), saved))
        throw Error("account save receipt changed during release");
      const snapshot = this.readSnapshot(reference.identity),
        row = snapshot.claim,
        claim = parsedClaim(JSON.parse(row.claim));
      const exported = JSON.parse(
          snapshot.frozen.export_json,
        ) as ReservedAvatarExportV2,
        id = reference.identity;
      // The saved source has intentionally changed; never call admission's old
      // baseline/session revalidator here, and never compare today's session.
      if (
        snapshot.authority !== id.authority_id ||
        row.avatar_id !== id.avatar_id ||
        row.account_id !== id.account_id ||
        row.session_hash !== id.session_ref ||
        row.operation !== id.admission_operation ||
        row.digest !== id.reservation_digest ||
        row.session_revision !== exported.account_revision ||
        claim.account_id !== id.account_id ||
        claim.avatar_id !== id.avatar_id ||
        claim.generation !== id.control_generation ||
        claim.lot_id !== id.lot_id ||
        claim.state !== "fence_pending" ||
        claim.content_release !== reference.origin.content_release ||
        claim.protocol_release !== reference.origin.protocol_release ||
        snapshot.frozen.operation !== id.admission_operation ||
        exported.export_digest !== id.export_digest ||
        exported.authority_id !== id.authority_id ||
        exported.account_id !== id.account_id ||
        exported.avatar_id !== id.avatar_id ||
        exported.control_generation !== id.control_generation ||
        exported.session_ref !== id.session_ref ||
        exported.lot_id !== id.lot_id ||
        exported.admission_operation !== id.admission_operation ||
        exported.reservation_digest !== id.reservation_digest ||
        exported.content_release !== reference.origin.content_release ||
        exported.protocol_release !== reference.origin.protocol_release
      )
        throw Error("release current control or export mismatch");
      this.capacity("avatar_control_release_receipts");
      this.retainOperation(id, reference.departure_operation);
      this.storage.sql.exec(
        "INSERT INTO avatar_control_release_receipts(control_key,account_id,avatar_id,control_generation,departure_operation,reference_json,receipt_json) VALUES(?,?,?,?,?,?,?)",
        controlKey(id),
        id.account_id,
        id.avatar_id,
        id.control_generation,
        reference.departure_operation,
        canonical(reference),
        canonical(receipt),
      );
      this.storage.sql.exec(
        "DELETE FROM avatar_control_leases WHERE avatar_id=?",
        id.avatar_id,
      );
      this.storage.sql.exec(
        "DELETE FROM avatar_control_exports WHERE avatar_id=? AND operation=?",
        id.avatar_id,
        id.admission_operation,
      );
      this.storage.sql.exec(
        "DELETE FROM avatar_control_claims WHERE avatar_id=? AND account_id=? AND session_hash=? AND operation=? AND digest=?",
        id.avatar_id,
        id.account_id,
        id.session_ref,
        id.admission_operation,
        id.reservation_digest,
      );
      return receipt;
    });
  }
}
