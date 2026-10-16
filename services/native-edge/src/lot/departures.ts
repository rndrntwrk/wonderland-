/** Private native-host departure evidence. None of these parsers authenticates a
 * browser or turns a speculative host proposal into a committed receipt. */
import {
  committedAvatarDetachV1,
  committedAvatarSaveV1,
  departureIdentityV1,
  detachProofReferenceV1,
  saveProofReferenceV1,
  saveReference,
  sourceAvatarLotSaveV1,
  type CommittedAvatarDetachV1,
  type CommittedAvatarSaveV1,
  type DepartureIdentityV1,
  departureCommitOriginV1,
  type SaveProofReferenceV1,
  type DepartureCommitOriginV1,
} from "../account/lot-save";
import { operation } from "../account/contracts";
import {
  binding,
  cursor,
  canonical,
  decimal,
  digest,
  encode,
  equal,
  fields,
  hash,
  type Binding,
  type Cursor,
} from "./contracts";
import {
  actorRegistry,
  controlKey,
  exportV2,
  type ActorEntry,
  type controlledProposal,
  type controlledSetup,
} from "./owned";
import { controlFence } from "./control";
import { successor, takeoverMarker } from "./recovery";

type Proposal = ReturnType<typeof controlledProposal>;
type Configuration = ReturnType<typeof controlledSetup>;
const CAPACITY = 65536;
const nonzero = (raw: unknown) => {
  const n = decimal(raw);
  if (n === "0") throw Error("zero departure observation");
  return n;
};
const ordinal = (raw: unknown) => {
  if (!Number.isInteger(raw) || (raw as number) < 0 || (raw as number) > 65535)
    throw Error("invalid departure event ordinal");
  return raw as number;
};
function entityRef(raw: unknown) {
  const r = fields(raw, ["object_id", "generation"]);
  if (
    !Number.isInteger(r.object_id) ||
    (r.object_id as number) < 1 ||
    (r.object_id as number) > 32767 ||
    !Number.isInteger(r.generation) ||
    (r.generation as number) < 1 ||
    (r.generation as number) > 4294967295
  )
    throw Error("invalid departure entity reference");
  return {
    object_id: r.object_id as number,
    generation: r.generation as number,
  };
}
/** Private operational identity shared by user leave, logout and lease expiry.
 * The browser never chooses this ID. Canonical source avatar and u64 bounds keep
 * authority_id + "_" + avatar_id + "_" + control_generation at <= 94 characters. */
export function departureOperationV1(
  identity: Pick<
    DepartureIdentityV1,
    "authority_id" | "avatar_id" | "control_generation"
  >,
): string {
  const avatar = nonzero(identity.avatar_id);
  if (BigInt(avatar) > 16777215n)
    throw Error("avatar identity outside source range");
  return operation(
    `${hash(identity.authority_id)}_${avatar}_${nonzero(identity.control_generation)}`,
  );
}
export function departureRequestV1(raw: unknown) {
  const v = fields(raw, ["schema_version", "identity", "departure_operation"]);
  if (v.schema_version !== 1) throw Error("unsupported departure request");
  const identity = departureIdentityV1(v.identity),
    departure_operation = operation(v.departure_operation);
  if (departure_operation !== departureOperationV1(identity))
    throw Error("departure operation must be server-derived");
  return { schema_version: 1 as const, identity, departure_operation };
}

export type DepartureRequestV1 = ReturnType<typeof departureRequestV1>;
export const departureControlKey = (i: DepartureIdentityV1) =>
  [
    i.authority_id,
    i.avatar_id,
    i.control_generation,
    i.admission_operation,
  ].join(":");
export const departureFence = (i: DepartureIdentityV1) =>
  controlFence({
    version: 1,
    authority: i.authority_id,
    avatar_id: i.avatar_id,
    generation: i.control_generation,
    lot_id: i.lot_id,
    request_digest: i.reservation_digest,
  });
export function departureEvidenceV1(raw: unknown) {
  if (encode(raw).byteLength > 2 * 1024 * 1024)
    throw Error("departure evidence capacity");
  const v = fields(raw, [
    "schema_version",
    "identity",
    "departure_operation",
    "lifecycle",
    "save",
    "detach",
    "entity_refs_after",
  ]);
  const request = departureRequestV1({
    schema_version: v.schema_version,
    identity: v.identity,
    departure_operation: v.departure_operation,
  });
  const l = fields(v.lifecycle, ["observed_tick", "status", "blocker"]);
  if (
    !["leaving", "blocked", "detached"].includes(l.status as string) ||
    (l.status === "blocked"
      ? typeof l.blocker !== "string" || !/^[a-z0-9_]{1,96}$/.test(l.blocker)
      : l.blocker !== null)
  )
    throw Error("invalid departure lifecycle");
  const lifecycle = {
    observed_tick: nonzero(l.observed_tick),
    status: l.status as "leaving" | "blocked" | "detached",
    blocker: l.blocker as string | null,
  };
  const save =
    v.save === null
      ? null
      : (() => {
          const s = fields(v.save, [
            "save_record",
            "save_sha256",
            "event_ordinal",
          ]);
          const save_record = sourceAvatarLotSaveV1(s.save_record);
          if (
            !equal(save_record.identity, request.identity) ||
            save_record.departure_operation !== request.departure_operation
          )
            throw Error("departure save identity mismatch");
          return {
            save_record,
            save_sha256: hash(s.save_sha256),
            event_ordinal: ordinal(s.event_ordinal),
          };
        })();
  const detach =
    v.detach === null
      ? null
      : (() => {
          const d = fields(v.detach, [
            "detached_tick",
            "removal_reason",
            "event_ordinal",
          ]);
          if (
            !["source_leave", "forced_cleanup"].includes(
              d.removal_reason as string,
            )
          )
            throw Error("invalid departure removal reason");
          return {
            detached_tick: nonzero(d.detached_tick),
            removal_reason: d.removal_reason as
              | "source_leave"
              | "forced_cleanup",
            event_ordinal: ordinal(d.event_ordinal),
          };
        })();
  if (
    (lifecycle.status === "detached") !== (detach !== null) ||
    (detach && detach.detached_tick !== lifecycle.observed_tick) ||
    (save &&
      BigInt(save.save_record.captured_tick) >
        BigInt(lifecycle.observed_tick)) ||
    (save && detach && save.event_ordinal >= detach.event_ordinal)
  )
    throw Error("departure lifecycle observation mismatch");
  if (!Array.isArray(v.entity_refs_after) || v.entity_refs_after.length > 32767)
    throw Error("departure entity projection capacity");
  const entity_refs_after = Array.from(v.entity_refs_after, entityRef);
  let previous = 0;
  for (const ref of entity_refs_after) {
    if (ref.object_id <= previous)
      throw Error("departure entity projection order or duplicate");
    previous = ref.object_id;
  }
  const present = entity_refs_after.some((ref) =>
    equal(ref, request.identity.actor),
  );
  if (present === (detach !== null))
    throw Error("departure entity presence mismatch");
  return { ...request, lifecycle, save, detach, entity_refs_after };
}
export type DepartureEvidenceV1 = ReturnType<typeof departureEvidenceV1>;
export async function verifyDepartureEvidence(p: Proposal["payload"]) {
  const d = p.departure_v1;
  if (!d) return;
  if (p.action_v1)
    throw Error("player action cannot ride trusted departure drain");
  if (d.identity.lot_id !== p.result.lot_id)
    throw Error("departure evidence lot mismatch");
  for (const tick of [
    d.lifecycle.observed_tick,
    ...(d.save ? [d.save.save_record.captured_tick] : []),
    ...(d.detach ? [d.detach.detached_tick] : []),
  ])
    if (
      BigInt(tick) <= BigInt(p.predecessor.completed_tick) ||
      BigInt(tick) > BigInt(p.result.completed_tick)
    )
      throw Error("departure observation outside accepted batch");
  if (
    d.save &&
    (await digest(encode(d.save.save_record))) !== d.save.save_sha256
  )
    throw Error("departure save digest mismatch");
  for (const actor of p.control_v2.actors)
    if (!d.entity_refs_after.some((ref) => equal(ref, actor.actor)))
      throw Error("departure post-state missing controlled actor");
}
interface PreparedDeparture {
  request: DepartureRequestV1;
  evidence_sha256: string;
  save: CommittedAvatarSaveV1 | null;
  detach: CommittedAvatarDetachV1 | null;
  detach_sha256: string | null;
  lineage: DepartureLineageAttestation | null;
  lineage_sha256: string | null;
}
type OperationRow = {
  request_json: string;
  status: string;
  observation_json: string | null;
};
type ReceiptRow = {
  receipt_json: string;
  receipt_sha256: string;
  batch_key: string;
  evidence_sha256: string;
};

export interface DepartureLineageState {
  binding: Binding;
  cursor: Cursor;
  checkpoint_sha256: string;
}
interface DepartureLineageTip extends DepartureLineageState {
  schema_version: 1;
  source_save: SaveProofReferenceV1;
  source_save_receipt_sha256: string;
  transition_sequence: string;
}
type TipRow = {
  control_key: string;
  tip_json: string;
  tip_sha256: string;
  anchor_kind: string;
  anchor_key: string;
};
export interface PreparedLineageAdvance {
  before: TipRow[];
  after: TipRow[];
  anchor_kind: "batch" | "recovery";
  anchor_key: string;
}
interface DepartureLineageAttestation {
  schema_version: 1;
  source_save: SaveProofReferenceV1;
  source_save_receipt_sha256: string;
  predecessor_tip: DepartureLineageTip | null;
  origin: DepartureCommitOriginV1;
}
function lineageTip(raw: unknown): DepartureLineageTip {
  const v = fields(raw, [
    "schema_version",
    "source_save",
    "source_save_receipt_sha256",
    "binding",
    "cursor",
    "checkpoint_sha256",
    "transition_sequence",
  ]);
  if (v.schema_version !== 1) throw Error("unsupported departure lineage tip");
  const source_save = saveProofReferenceV1(v.source_save),
    current = cursor(v.cursor);
  if (current.lot_id !== source_save.identity.lot_id)
    throw Error("departure lineage tip lot mismatch");
  return {
    schema_version: 1,
    source_save,
    source_save_receipt_sha256: hash(v.source_save_receipt_sha256),
    binding: binding(v.binding),
    cursor: current,
    checkpoint_sha256: hash(v.checkpoint_sha256),
    transition_sequence: decimal(v.transition_sequence),
  };
}
function lineageAttestation(raw: unknown): DepartureLineageAttestation {
  const v = fields(raw, [
    "schema_version",
    "source_save",
    "source_save_receipt_sha256",
    "predecessor_tip",
    "origin",
  ]);
  if (v.schema_version !== 1)
    throw Error("unsupported departure lineage attestation");
  const result: DepartureLineageAttestation = {
    schema_version: 1,
    source_save: saveProofReferenceV1(v.source_save),
    source_save_receipt_sha256: hash(v.source_save_receipt_sha256),
    predecessor_tip:
      v.predecessor_tip === null ? null : lineageTip(v.predecessor_tip),
    origin: departureCommitOriginV1(v.origin),
  };
  const save = result.source_save.origin,
    detach = result.origin,
    tip = result.predecessor_tip;
  if (
    save.content_release !== detach.content_release ||
    save.protocol_release !== detach.protocol_release ||
    save.result.lot_id !== detach.result.lot_id
  )
    throw Error("departure source lineage realm mismatch");
  if (tip) {
    if (
      !equal(tip.source_save, result.source_save) ||
      tip.source_save_receipt_sha256 !== result.source_save_receipt_sha256 ||
      !equal(tip.binding, detach.binding) ||
      !equal(tip.cursor, detach.predecessor)
    )
      throw Error("departure source lineage predecessor mismatch");
  } else if (
    !equal(save.binding, detach.binding) ||
    save.batch_id !== detach.batch_id ||
    save.proposal_digest !== detach.proposal_digest ||
    !equal(save.predecessor, detach.predecessor) ||
    !equal(save.result, detach.result) ||
    save.checkpoint_sha256 !== detach.checkpoint_sha256 ||
    save.event_ordinal >= detach.event_ordinal
  )
    throw Error("departure same-batch lineage mismatch");
  return result;
}

/** Receipt rows, frozen detach lineage attestations and receipt-origin anchors
 * survive compaction. Exactly one mutable, independently anchored lineage tip is
 * retained per saved-but-not-detached avatar, never one row per ordinary tick. */
export class DepartureStore {
  private storage: DurableObjectStorage;
  constructor(storage: DurableObjectStorage) {
    this.storage = storage;
    storage.sql
      .exec(`CREATE TABLE IF NOT EXISTS owned_departures(control_key TEXT PRIMARY KEY,request_json TEXT NOT NULL,status TEXT NOT NULL,observation_json TEXT);
      CREATE TABLE IF NOT EXISTS departure_saves(control_key TEXT PRIMARY KEY,receipt_json TEXT NOT NULL,receipt_sha256 TEXT NOT NULL,batch_key TEXT NOT NULL,evidence_sha256 TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS departure_detaches(control_key TEXT PRIMARY KEY,receipt_json TEXT NOT NULL,receipt_sha256 TEXT NOT NULL,batch_key TEXT NOT NULL,evidence_sha256 TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS departure_outbox(control_key TEXT NOT NULL,lane TEXT NOT NULL,reference_json TEXT NOT NULL,PRIMARY KEY(control_key,lane));
      CREATE TABLE IF NOT EXISTS departure_anchors(batch_key TEXT PRIMARY KEY,binding_json TEXT NOT NULL,predecessor_json TEXT NOT NULL,result_json TEXT NOT NULL,checkpoint_sha256 TEXT NOT NULL,proposal_digest TEXT NOT NULL,evidence_sha256 TEXT);
      CREATE TABLE IF NOT EXISTS departure_lineage_tips(control_key TEXT PRIMARY KEY,tip_json TEXT NOT NULL,tip_sha256 TEXT NOT NULL,anchor_kind TEXT NOT NULL,anchor_key TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS departure_detach_lineages(control_key TEXT PRIMARY KEY,attestation_json TEXT NOT NULL,attestation_sha256 TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS actor_control_history(control_key TEXT PRIMARY KEY,record_json TEXT NOT NULL,release_json TEXT NOT NULL);`);
  }
  private bounded(table: "owned_departures") {
    if (
      this.storage.sql
        .exec<{ n: number }>(`SELECT COUNT(*) AS n FROM ${table}`)
        .one().n >= CAPACITY
    )
      throw Error("departure retention capacity requires repair");
  }
  private operation(request: DepartureRequestV1) {
    const row = this.storage.sql
      .exec<OperationRow>(
        "SELECT request_json,status,observation_json FROM owned_departures WHERE control_key=?",
        departureControlKey(request.identity),
      )
      .toArray()[0];
    if (row && !equal(JSON.parse(row.request_json), request))
      throw Error("departure operation identity reused");
    return row;
  }
  begin(request: DepartureRequestV1) {
    const prior = this.operation(request);
    if (!prior) {
      this.bounded("owned_departures");
      this.storage.sql.exec(
        "INSERT INTO owned_departures(control_key,request_json,status) VALUES(?,?,'leaving')",
        departureControlKey(request.identity),
        canonical(request),
      );
    }
    return this.lookup(request);
  }
  lookup(raw: unknown) {
    const request = departureRequestV1(raw),
      row = this.operation(request);
    if (!row) throw Error("unknown departure operation");
    return {
      ...request,
      status: row.status,
      observation: row.observation_json
        ? JSON.parse(row.observation_json)
        : null,
    };
  }
  /** Private authenticated supervisor lookup; all owner/session/admission fields
   * are required even after the live account claim/export has been retired. */
  find(raw: unknown) {
    const v = fields(raw, [
      "account_id",
      "avatar_id",
      "session_ref",
      "admission_operation",
    ]);
    const account = nonzero(v.account_id),
      avatar = nonzero(v.avatar_id),
      session = hash(v.session_ref),
      admission = operation(v.admission_operation);
    const rows = this.storage.sql
      .exec<{
        request_json: string;
      }>(
        "SELECT request_json FROM owned_departures WHERE json_extract(request_json,'$.identity.account_id')=? AND json_extract(request_json,'$.identity.avatar_id')=? AND json_extract(request_json,'$.identity.session_ref')=? AND json_extract(request_json,'$.identity.admission_operation')=? LIMIT 2",
        account,
        avatar,
        session,
        admission,
      )
      .toArray();
    if (rows.length > 1) throw Error("ambiguous departure admission identity");
    return rows.length
      ? this.lookup(departureRequestV1(JSON.parse(rows[0].request_json)))
      : null;
  }
  /** Durable work enumeration includes the interval before any save/outbox row.
   * Cursor pagination preserves discoverability; native execution still serializes
   * starts behind an already-observed departure, including a blocked one. */
  active(after: unknown = "") {
    if (
      typeof after !== "string" ||
      after.length > 256 ||
      !/^[a-zA-Z0-9_:\-]*$/.test(after)
    )
      throw Error("invalid departure cursor");
    const rows = this.storage.sql
      .exec<{
        control_key: string;
        request_json: string;
      }>(
        "SELECT control_key,request_json FROM owned_departures WHERE control_key>? AND status IN ('leaving','blocked') ORDER BY control_key LIMIT 129",
        after,
      )
      .toArray();
    const page = rows.slice(0, 128);
    return {
      departures: page.map((row) =>
        this.lookup(departureRequestV1(JSON.parse(row.request_json))),
      ),
      next: rows.length > 128 ? page[127].control_key : null,
    };
  }
  pending() {
    return this.storage.sql
      .exec<{ control_key: string; lane: string; reference_json: string }>(
        "SELECT control_key,lane,reference_json FROM departure_outbox ORDER BY control_key,CASE WHEN lane='save' THEN 0 ELSE 1 END LIMIT 256",
      )
      .toArray()
      .map((r) => ({
        control_key: r.control_key,
        lane: r.lane,
        reference: JSON.parse(r.reference_json),
      }));
  }
  requireInstallation(
    identity: DepartureIdentityV1,
    config: Configuration,
    lot: string,
  ) {
    const row = this.storage.sql
      .exec<{
        export_json: string;
        principal: string;
        state: string;
        record: string;
        expires_at: number;
      }>(
        "SELECT export_json,principal,state,record,expires_at FROM actor_controls WHERE control_key=?",
        departureControlKey(identity),
      )
      .toArray()[0];
    if (!row) throw Error("departure has no durable installation");
    const e = exportV2(JSON.parse(row.export_json));
    if (
      config.authority_id !== identity.authority_id ||
      lot !== identity.lot_id ||
      e.authority_id !== identity.authority_id ||
      e.lot_id !== identity.lot_id ||
      e.account_id !== identity.account_id ||
      e.avatar_id !== identity.avatar_id ||
      e.control_generation !== identity.control_generation ||
      e.session_ref !== identity.session_ref ||
      e.admission_operation !== identity.admission_operation ||
      e.reservation_digest !== identity.reservation_digest ||
      e.export_digest !== identity.export_digest ||
      row.principal !== identity.principal ||
      e.content_release !== config.content_release ||
      e.protocol_release !== config.protocol_release ||
      controlKey(e) !== departureControlKey(identity)
    )
      throw Error("departure installation identity mismatch");
    return { row, export: e };
  }
  private receipt(
    table: "departure_saves" | "departure_detaches",
    key: string,
  ) {
    return this.storage.sql
      .exec<ReceiptRow>(
        `SELECT receipt_json,receipt_sha256,batch_key,evidence_sha256 FROM ${table} WHERE control_key=?`,
        key,
      )
      .toArray()[0];
  }
  private anchor(
    origin: DepartureCommitOriginV1,
    evidence: string,
    lane: "save" | "detach",
    receiptHash: string,
  ) {
    const key = origin.binding.lot_incarnation + ":" + origin.batch_id;
    const row = this.storage.sql
      .exec<{
        digest: string;
        receipt: string;
      }>("SELECT digest,receipt FROM batches WHERE batch_key=?", key)
      .toArray()[0];
    const link = this.storage.sql
      .exec<{
        binding_json: string;
        predecessor_json: string;
        result_json: string;
        checkpoint_sha256: string;
        proposal_digest: string;
        evidence_sha256: string;
      }>(
        "SELECT binding_json,predecessor_json,result_json,checkpoint_sha256,proposal_digest,evidence_sha256 FROM departure_anchors WHERE batch_key=?",
        key,
      )
      .toArray()[0];
    if (
      !row ||
      !link ||
      row.digest !== origin.proposal_digest ||
      link.proposal_digest !== origin.proposal_digest ||
      link.evidence_sha256 !== evidence ||
      link.checkpoint_sha256 !== origin.checkpoint_sha256 ||
      !equal(JSON.parse(link.binding_json), origin.binding) ||
      !equal(JSON.parse(link.predecessor_json), origin.predecessor) ||
      !equal(JSON.parse(link.result_json), origin.result)
    )
      throw Error("departure receipt committed anchor missing or mismatched");
    const decision = JSON.parse(row.receipt);
    if (
      decision.status !== "committed" ||
      decision.digest !== origin.proposal_digest ||
      decision.batch_id !== origin.batch_id ||
      decision.departure_evidence_sha256 !== evidence ||
      decision[`departure_${lane}_receipt_sha256`] !== receiptHash ||
      !equal(decision.cursor, origin.result)
    )
      throw Error("departure receipt is not an accepted batch");
  }
  private async checkedSave(key: string) {
    const row = this.receipt("departure_saves", key);
    if (!row) throw Error("committed source save missing");
    const save = committedAvatarSaveV1(JSON.parse(row.receipt_json));
    if (
      departureControlKey(save.identity) !== key ||
      (await digest(encode(save))) !== row.receipt_sha256 ||
      (await digest(encode(save.save_record))) !== save.save_sha256 ||
      row.batch_key !==
        save.origin.binding.lot_incarnation + ":" + save.origin.batch_id
    )
      throw Error("retained source save digest mismatch");
    this.anchor(save.origin, row.evidence_sha256, "save", row.receipt_sha256);
    return save;
  }
  async lookupSave(raw: unknown) {
    const ref = saveProofReferenceV1(raw),
      save = await this.checkedSave(departureControlKey(ref.identity));
    if (!equal(saveReference(save), ref))
      throw Error("committed source save reference mismatch");
    return save;
  }
  async lookupDetach(raw: unknown) {
    const ref = detachProofReferenceV1(raw),
      key = departureControlKey(ref.identity),
      row = this.receipt("departure_detaches", key);
    if (!row) throw Error("committed actor detach missing");
    const receipt = committedAvatarDetachV1(JSON.parse(row.receipt_json));
    if (
      (await digest(encode(receipt))) !== row.receipt_sha256 ||
      row.receipt_sha256 !== ref.detach_sha256 ||
      !equal(receipt.identity, ref.identity) ||
      receipt.departure_operation !== ref.departure_operation ||
      !equal(receipt.origin, ref.origin) ||
      row.batch_key !==
        receipt.origin.binding.lot_incarnation + ":" + receipt.origin.batch_id
    )
      throw Error("committed detach reference mismatch");
    this.anchor(
      receipt.origin,
      row.evidence_sha256,
      "detach",
      row.receipt_sha256,
    );
    const save = await this.lookupSave(receipt.source_save);
    const lineage = this.storage.sql
      .exec<{
        attestation_json: string;
        attestation_sha256: string;
      }>(
        "SELECT attestation_json,attestation_sha256 FROM departure_detach_lineages WHERE control_key=?",
        key,
      )
      .toArray()[0];
    if (!lineage) throw Error("departure frozen lineage missing");
    const attestation = lineageAttestation(
      JSON.parse(lineage.attestation_json),
    );
    const decision = JSON.parse(
      this.storage.sql
        .exec<{
          receipt: string;
        }>("SELECT receipt FROM batches WHERE batch_key=?", row.batch_key)
        .one().receipt,
    );
    if (
      (await digest(encode(attestation))) !== lineage.attestation_sha256 ||
      decision.departure_lineage_sha256 !== lineage.attestation_sha256 ||
      !equal(attestation.source_save, receipt.source_save) ||
      !equal(attestation.origin, receipt.origin) ||
      attestation.source_save_receipt_sha256 !== (await digest(encode(save)))
    )
      throw Error("departure frozen lineage integrity mismatch");
    return receipt;
  }
  private tipRows(): TipRow[] {
    const rows = this.storage.sql
      .exec<TipRow>(
        "SELECT control_key,tip_json,tip_sha256,anchor_kind,anchor_key FROM departure_lineage_tips ORDER BY control_key LIMIT 257",
      )
      .toArray();
    if (rows.length > 256) throw Error("departure active lineage capacity");
    return rows;
  }
  private async checkedTip(row: TipRow) {
    const tip = lineageTip(JSON.parse(row.tip_json));
    if (
      departureControlKey(tip.source_save.identity) !== row.control_key ||
      (await digest(encode(tip))) !== row.tip_sha256
    )
      throw Error("departure lineage tip integrity mismatch");
    if (row.anchor_kind === "batch") {
      const stored = this.storage.sql
        .exec<{
          digest: string;
          receipt: string;
        }>(
          "SELECT digest,receipt FROM batches WHERE batch_key=?",
          row.anchor_key,
        )
        .toArray()[0];
      const decision = stored && JSON.parse(stored.receipt);
      if (
        !stored ||
        decision.status !== "committed" ||
        decision.digest !== stored.digest ||
        row.anchor_key !==
          tip.binding.lot_incarnation + ":" + decision.batch_id ||
        !equal(decision.cursor, tip.cursor) ||
        decision.departure_lineage_tips?.[row.control_key] !== row.tip_sha256
      )
        throw Error("departure lineage tip batch anchor mismatch");
    } else if (row.anchor_kind === "recovery") {
      const stored = this.storage.sql
        .exec<{
          status: string;
          marker_json: string | null;
          grant_json: string;
        }>(
          "SELECT status,marker_json,grant_json FROM recoveries WHERE old_incarnation=?",
          row.anchor_key,
        )
        .toArray()[0];
      const control = this.storage.sql
        .exec<{
          control_json: string;
        }>(
          "SELECT control_json FROM controlled_recovery_receipts WHERE old_incarnation=?",
          row.anchor_key,
        )
        .toArray()[0];
      if (
        !stored ||
        stored.status !== "committed" ||
        !stored.marker_json ||
        !control
      )
        throw Error("departure lineage tip takeover anchor missing");
      const marker = takeoverMarker(JSON.parse(stored.marker_json));
      if (
        !equal(marker.grant, JSON.parse(stored.grant_json)) ||
        marker.grant.old_binding.lot_incarnation !== row.anchor_key ||
        !equal(marker.grant.new_binding, tip.binding) ||
        !equal(marker.new_cursor, tip.cursor) ||
        marker.checkpoint_sha256 !== tip.checkpoint_sha256 ||
        JSON.parse(control.control_json).departure_lineage_tips?.[
          row.control_key
        ] !== row.tip_sha256
      )
        throw Error("departure lineage tip takeover anchor mismatch");
    } else throw Error("departure lineage tip anchor kind mismatch");
    return tip;
  }
  async prepareAdvance(
    previous: DepartureLineageState,
    next: DepartureLineageState,
    anchor_kind: "batch" | "recovery",
    anchor_key: string,
  ): Promise<PreparedLineageAdvance> {
    const before = this.tipRows(),
      after: TipRow[] = [];
    for (const row of before) {
      const old = await this.checkedTip(row);
      if (
        !equal(old.binding, previous.binding) ||
        !equal(old.cursor, previous.cursor) ||
        old.checkpoint_sha256 !== previous.checkpoint_sha256
      )
        throw Error(
          "departure lineage tip does not match committed predecessor",
        );
      const tip: DepartureLineageTip = {
        ...old,
        ...next,
        transition_sequence: successor(old.transition_sequence),
      };
      after.push({
        ...row,
        tip_json: canonical(tip),
        tip_sha256: await digest(encode(tip)),
        anchor_kind,
        anchor_key,
      });
    }
    return { before, after, anchor_kind, anchor_key };
  }
  lineageHeads(advance: PreparedLineageAdvance | null) {
    return Object.fromEntries(
      (advance?.after ?? []).map((row) => [row.control_key, row.tip_sha256]),
    );
  }
  recordAdvance(advance: PreparedLineageAdvance) {
    if (!equal(this.tipRows(), advance.before))
      throw Error("departure lineage tips changed before commit");
    for (const old of advance.before)
      if (!advance.after.some((row) => row.control_key === old.control_key))
        this.storage.sql.exec(
          "DELETE FROM departure_lineage_tips WHERE control_key=?",
          old.control_key,
        );
    for (const row of advance.after)
      this.storage.sql.exec(
        "INSERT INTO departure_lineage_tips(control_key,tip_json,tip_sha256,anchor_kind,anchor_key) VALUES(?,?,?,?,?) ON CONFLICT(control_key) DO UPDATE SET tip_json=excluded.tip_json,tip_sha256=excluded.tip_sha256,anchor_kind=excluded.anchor_kind,anchor_key=excluded.anchor_key",
        row.control_key,
        row.tip_json,
        row.tip_sha256,
        row.anchor_kind,
        row.anchor_key,
      );
  }
  async prepare(
    p: Proposal,
    config: Configuration,
    advance: PreparedLineageAdvance,
  ): Promise<PreparedDeparture | null> {
    const d = p.payload.departure_v1;
    if (!d) return null;
    await verifyDepartureEvidence(p.payload);
    const request = departureRequestV1({
      schema_version: 1,
      identity: d.identity,
      departure_operation: d.departure_operation,
    });
    const installed = this.requireInstallation(
      d.identity,
      config,
      p.payload.result.lot_id,
    );
    const origin = (tick: string, event: number): DepartureCommitOriginV1 => ({
      binding: p.payload.binding,
      content_release: config.content_release,
      protocol_release: config.protocol_release,
      batch_id: p.payload.batch_id,
      proposal_digest: p.digest,
      predecessor: p.payload.predecessor,
      result: p.payload.result,
      checkpoint_sha256: p.payload.checkpoint_sha256,
      captured_tick: tick,
      event_ordinal: event,
    });
    const save = d.save
      ? committedAvatarSaveV1({
          ...request,
          status: "committed_source_save",
          save_record: d.save.save_record,
          save_sha256: d.save.save_sha256,
          source_baseline: {
            progression: installed.export.source_revisions.progression,
            wardrobe: installed.export.source_revisions.wardrobe,
          },
          origin: origin(
            d.save.save_record.captured_tick,
            d.save.event_ordinal,
          ),
        })
      : null;
    const source = d.detach
      ? (save ?? (await this.checkedSave(departureControlKey(d.identity))))
      : null;
    const detach =
      d.detach && source
        ? committedAvatarDetachV1({
            ...request,
            status: "committed_actor_detach",
            source_save: saveReference(source),
            detached_tick: d.detach.detached_tick,
            removal_reason: d.detach.removal_reason,
            origin: origin(d.detach.detached_tick, d.detach.event_ordinal),
            control_revision_before: p.payload.control_v2.revision_before,
            control_revision_after: p.payload.control_v2.revision_after,
            actor_registry_before_sha256:
              p.payload.control_v2.actor_registry_before_sha256,
            actor_registry_after_sha256:
              p.payload.control_v2.actor_registry_after_sha256,
          })
        : null;
    const key = departureControlKey(d.identity),
      sourceReceiptHash = source ? await digest(encode(source)) : null;
    const oldTip = advance.before.find((row) => row.control_key === key);
    const lineage =
      detach && source
        ? lineageAttestation({
            schema_version: 1,
            source_save: saveReference(source),
            source_save_receipt_sha256: sourceReceiptHash,
            predecessor_tip: save
              ? null
              : oldTip
                ? JSON.parse(oldTip.tip_json)
                : undefined,
            origin: detach.origin,
          })
        : null;
    if (save && !detach) {
      if (oldTip) throw Error("source save already has a lineage tip");
      const tip: DepartureLineageTip = {
        schema_version: 1,
        source_save: saveReference(save),
        source_save_receipt_sha256: await digest(encode(save)),
        binding: p.payload.binding,
        cursor: p.payload.result,
        checkpoint_sha256: p.payload.checkpoint_sha256,
        transition_sequence: "0",
      };
      advance.after.push({
        control_key: key,
        tip_json: canonical(tip),
        tip_sha256: await digest(encode(tip)),
        anchor_kind: advance.anchor_kind,
        anchor_key: advance.anchor_key,
      });
      advance.after.sort((a, b) => a.control_key.localeCompare(b.control_key));
    }
    if (detach)
      advance.after = advance.after.filter((row) => row.control_key !== key);
    return {
      request,
      evidence_sha256: await digest(encode(d)),
      save,
      detach,
      detach_sha256: detach ? await digest(encode(detach)) : null,
      lineage,
      lineage_sha256: lineage ? await digest(encode(lineage)) : null,
    };
  }
  validate(
    p: Proposal,
    before: ActorEntry[],
    config: Configuration,
    prepared: PreparedDeparture,
  ) {
    const d = p.payload.departure_v1!,
      c = p.payload.control_v2,
      key = departureControlKey(d.identity);
    if (p.payload.action_v1)
      throw Error("player action cannot ride trusted departure drain");
    const op = this.operation(prepared.request);
    if (!op || op.status === "released" || op.status === "detached")
      throw Error("departure is not pending");
    const installed = this.requireInstallation(
      d.identity,
      config,
      p.payload.result.lot_id,
    );
    if (installed.row.state !== "fence_pending")
      throw Error("departure target is not fenced");
    const fence = this.storage.sql
      .exec<{
        receipt: string;
      }>(
        "SELECT receipt FROM control_fences WHERE authority=? AND avatar_id=? AND generation=?",
        d.identity.authority_id,
        d.identity.avatar_id,
        d.identity.control_generation,
      )
      .toArray()[0];
    if (
      !fence ||
      !equal(JSON.parse(fence.receipt).fence, departureFence(d.identity))
    )
      throw Error("departure exact fence missing");
    const { authority_id: _authority, lot_id: _lot, ...entry } = d.identity;
    if (!before.some((a) => equal(a, entry)))
      throw Error("departure target actor is not admitted");
    const expected = d.detach ? before.filter((a) => !equal(a, entry)) : before;
    if (!equal(expected, c.actors))
      throw Error("departure changed non-target controlled actor");
    if (
      c.revision_after !==
        (d.detach ? successor(c.revision_before) : c.revision_before) ||
      !equal(c.permission_after, c.permission_before)
    )
      throw Error("departure revision or permission transition mismatch");
    if (prepared.save && this.receipt("departure_saves", key))
      throw Error("source save one-shot identity already committed");
    if (prepared.detach) {
      if (this.receipt("departure_detaches", key))
        throw Error("actor detach already committed");
      if (!prepared.lineage) throw Error("departure frozen lineage missing");
      lineageAttestation(prepared.lineage);
    }
  }
  retire(raw: unknown, release: unknown) {
    const ref = detachProofReferenceV1(raw),
      key = departureControlKey(ref.identity);
    const old = this.storage.sql
      .exec<{
        release_json: string;
      }>(
        "SELECT release_json FROM actor_control_history WHERE control_key=?",
        key,
      )
      .toArray()[0];
    if (old) {
      if (!equal(JSON.parse(old.release_json), release))
        throw Error("departure release acknowledgment changed");
      return JSON.parse(old.release_json);
    }
    const op = this.operation({
      schema_version: 1,
      identity: ref.identity,
      departure_operation: ref.departure_operation,
    });
    if (!op || op.status !== "detached")
      throw Error("departure actor is not detached");
    const registry = this.storage.sql
      .exec<{
        value: string;
      }>("SELECT value FROM controlled_registry WHERE id=1")
      .one();
    if (
      actorRegistry(JSON.parse(registry.value)).some(
        (a) =>
          a.principal === ref.identity.principal ||
          equal(a.actor, ref.identity.actor),
      )
    )
      throw Error("departure actor is still in committed registry");
    const row = this.storage.sql
      .exec<{
        control_key: string;
        avatar_id: string;
        principal: string;
        export_digest: string;
        export_json: string;
        state: string;
        expires_at: number;
        record: string;
        receipt: string | null;
      }>(
        "SELECT control_key,avatar_id,principal,export_digest,export_json,state,expires_at,record,receipt FROM actor_controls WHERE control_key=?",
        key,
      )
      .toArray()[0];
    if (
      !row ||
      row.state !== "fence_pending" ||
      row.principal !== ref.identity.principal ||
      row.avatar_id !== ref.identity.avatar_id ||
      row.export_digest !== ref.identity.export_digest
    )
      throw Error("departure retired control identity mismatch");
    this.storage.sql.exec(
      "INSERT INTO actor_control_history(control_key,record_json,release_json) VALUES(?,?,?)",
      key,
      canonical(row),
      canonical(release),
    );
    this.storage.sql.exec(
      "DELETE FROM controlled_leases WHERE control_key=?",
      key,
    );
    this.storage.sql.exec(
      "DELETE FROM actor_controls WHERE control_key=?",
      key,
    );
    this.storage.sql.exec(
      "DELETE FROM departure_outbox WHERE control_key=?",
      key,
    );
    this.storage.sql.exec(
      "UPDATE owned_departures SET status='released' WHERE control_key=?",
      key,
    );
    return release;
  }
  recordAnchor(p: Proposal, prepared: PreparedDeparture | null) {
    if (!prepared?.save && !prepared?.detach) return;
    this.storage.sql.exec(
      "INSERT INTO departure_anchors(batch_key,binding_json,predecessor_json,result_json,checkpoint_sha256,proposal_digest,evidence_sha256) VALUES(?,?,?,?,?,?,?)",
      p.payload.binding.lot_incarnation + ":" + p.payload.batch_id,
      canonical(p.payload.binding),
      canonical(p.payload.predecessor),
      canonical(p.payload.result),
      p.payload.checkpoint_sha256,
      p.digest,
      prepared?.evidence_sha256 ?? null,
    );
  }
  async receiptHashes(prepared: PreparedDeparture | null) {
    return prepared?.save ? await digest(encode(prepared.save)) : null;
  }
  record(
    p: Proposal,
    prepared: PreparedDeparture,
    saveReceiptHash: string | null,
  ) {
    const d = p.payload.departure_v1!,
      key = departureControlKey(d.identity),
      batch = p.payload.binding.lot_incarnation + ":" + p.payload.batch_id;
    for (const [table, receipt, sha, lane, reference] of [
      [
        "departure_saves",
        prepared.save,
        saveReceiptHash,
        "save",
        prepared.save ? saveReference(prepared.save) : null,
      ],
      [
        "departure_detaches",
        prepared.detach,
        prepared.detach_sha256,
        "release",
        prepared.detach
          ? {
              ...prepared.request,
              detach_sha256: prepared.detach_sha256,
              origin: prepared.detach.origin,
            }
          : null,
      ],
    ] as const)
      if (receipt) {
        this.storage.sql.exec(
          `INSERT INTO ${table}(control_key,receipt_json,receipt_sha256,batch_key,evidence_sha256) VALUES(?,?,?,?,?)`,
          key,
          canonical(receipt),
          sha!,
          batch,
          prepared.evidence_sha256,
        );
        this.storage.sql.exec(
          "INSERT INTO departure_outbox(control_key,lane,reference_json) VALUES(?,?,?)",
          key,
          lane,
          canonical(reference),
        );
      }
    if (prepared.lineage)
      this.storage.sql.exec(
        "INSERT INTO departure_detach_lineages(control_key,attestation_json,attestation_sha256) VALUES(?,?,?)",
        key,
        canonical(prepared.lineage),
        prepared.lineage_sha256!,
      );
    this.storage.sql.exec(
      "UPDATE owned_departures SET status=?,observation_json=? WHERE control_key=?",
      d.lifecycle.status,
      canonical(d.lifecycle),
      key,
    );
  }
}
