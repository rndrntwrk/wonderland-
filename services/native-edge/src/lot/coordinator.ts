import { DurableObject } from "cloudflare:workers";
import { permissionBinding } from "../account/portable";
import type { AccountAuthority } from "../account/authority";
import {
  actorRegistry,
  controlledProposal,
  controlledSetup,
  controlKey,
  EMPTY_REGISTRY_SHA256,
  exportV2,
  MAX_CONTROLS,
  type ActorEntry,
} from "./owned";
import {
  binding,
  fields,
  cursor,
  decimal,
  digest,
  encode,
  equal,
  hash,
  proposal,
  canonical,
  type Binding,
  type Cursor,
} from "./contracts";
import { recoveryChain } from "./journal";
import {
  ActionOutcomeStore,
  actionLookup,
  actionOutcomeHashes,
  decodeActionRequest,
  verifyActionEvidence,
  type ActionControl,
} from "./action-outcomes";
import { HostReceiptStore } from "./host-receipts";
import {
  DepartureStore,
  departureRequestV1,
  departureControlKey,
  departureFence,
  verifyDepartureEvidence,
} from "./departures";
import { detachProofReferenceV1 } from "../account/lot-save";
import { successor, takeoverMarker, type RecoveryGrant } from "./recovery";
import { controlFence, type ControlFence } from "./control";
import { controlStateV2 } from "./control-state";
interface Env {
  CHECKPOINTS: R2Bucket;
  ACCOUNTS?: DurableObjectNamespace<AccountAuthority>;
  ACCOUNT_REALM?: string;
  NATIVE_ACCOUNT_REALM?: string;
}
interface State {
  binding: Binding;
  cursor: Cursor;
  checkpoint_sha256: string;
}
const CHUNK = 256 * 1024;
const JOURNAL_LIMIT = 64 * 1024 * 1024;
const ARCHIVE_LIMIT = 16 * 1024 * 1024;
const ARCHIVE_BATCHES = 64;
export class LotCoordinator extends DurableObject<Env> {
  private hostReceipts: HostReceiptStore;
  private actionOutcomes: ActionOutcomeStore;
  private departures: DepartureStore;
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    ctx.storage.sql.exec(
      `CREATE TABLE IF NOT EXISTS controlled_config(id INTEGER PRIMARY KEY CHECK(id=1),value TEXT NOT NULL);CREATE TABLE IF NOT EXISTS controlled_registry(id INTEGER PRIMARY KEY CHECK(id=1),sha256 TEXT NOT NULL,value TEXT NOT NULL);CREATE TABLE IF NOT EXISTS controlled_sequence(id INTEGER PRIMARY KEY CHECK(id=1),principal TEXT NOT NULL);CREATE TABLE IF NOT EXISTS actor_controls(control_key TEXT PRIMARY KEY,avatar_id TEXT NOT NULL UNIQUE,principal TEXT NOT NULL UNIQUE,export_digest TEXT NOT NULL UNIQUE,export_json TEXT NOT NULL,state TEXT NOT NULL,expires_at INTEGER NOT NULL,record TEXT NOT NULL,receipt TEXT);CREATE INDEX IF NOT EXISTS actor_controls_expiry ON actor_controls(state,expires_at);CREATE TABLE IF NOT EXISTS admission_operations(control_key TEXT PRIMARY KEY,export_digest TEXT NOT NULL,receipt TEXT NOT NULL);CREATE TABLE IF NOT EXISTS controlled_leases(control_key TEXT PRIMARY KEY,revision TEXT NOT NULL);CREATE TABLE IF NOT EXISTS controlled_recovery_receipts(old_incarnation TEXT PRIMARY KEY,control_json TEXT NOT NULL);CREATE TABLE IF NOT EXISTS controlled_denials(batch_key TEXT PRIMARY KEY,digest TEXT NOT NULL,receipt TEXT NOT NULL);CREATE TABLE IF NOT EXISTS lot_control_state(id INTEGER PRIMARY KEY CHECK(id=1),revision TEXT NOT NULL);CREATE TABLE IF NOT EXISTS control_fences(authority TEXT NOT NULL, avatar_id TEXT NOT NULL, generation TEXT NOT NULL, receipt TEXT NOT NULL, PRIMARY KEY(authority,avatar_id,generation));CREATE TABLE IF NOT EXISTS lot_state(id INTEGER PRIMARY KEY CHECK(id=1),value TEXT NOT NULL);CREATE TABLE IF NOT EXISTS batches(batch_key TEXT PRIMARY KEY,digest TEXT NOT NULL,receipt TEXT NOT NULL,bytes INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS journal_chunks(batch_key TEXT NOT NULL,ordinal INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(batch_key,ordinal));CREATE TABLE IF NOT EXISTS outbox(batch_key TEXT NOT NULL,lane TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(batch_key,lane));CREATE TABLE IF NOT EXISTS journal_live(batch_key TEXT PRIMARY KEY,bytes INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS journal_layout(id INTEGER PRIMARY KEY CHECK(id=1));CREATE TABLE IF NOT EXISTS journal_archives(object_key TEXT PRIMARY KEY,sha256 TEXT NOT NULL,bytes INTEGER NOT NULL,checkpoint_key TEXT NOT NULL);CREATE TABLE IF NOT EXISTS archived_batches(batch_key TEXT PRIMARY KEY,object_key TEXT NOT NULL,byte_offset INTEGER NOT NULL,byte_length INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS checkpoints(object_key TEXT PRIMARY KEY,manifest TEXT NOT NULL);CREATE TABLE IF NOT EXISTS recoveries(old_incarnation TEXT PRIMARY KEY,status TEXT NOT NULL,grant_json TEXT NOT NULL,marker_json TEXT);`,
    );
    this.hostReceipts = new HostReceiptStore(ctx.storage, env.CHECKPOINTS);
    this.actionOutcomes = new ActionOutcomeStore(ctx.storage);
    this.departures = new DepartureStore(ctx.storage);
    // One-time, restart-safe migration. Receipt rows stay forever; only this
    // bounded live set is charged against the hot-journal profile.
    ctx.storage.transactionSync(() => {
      ctx.storage.sql.exec(
        "INSERT OR IGNORE INTO journal_live(batch_key,bytes) SELECT batch_key,bytes FROM batches WHERE NOT EXISTS (SELECT 1 FROM journal_layout WHERE id=1);INSERT OR IGNORE INTO journal_layout(id) VALUES(1)",
      );
    });
  }
  persistHostReceiptsV1(raw: unknown, sha: unknown) {
    return this.hostReceipts.persist(raw, sha);
  }
  lookupHostReceiptV1(incarnation: unknown, id: unknown) {
    return this.hostReceipts.lookup(incarnation, id);
  }
  /** Private authenticated-control lookup; historical reads do not grant mutation. */
  async lookupPlayerActionV1(raw: unknown) {
    const q = actionLookup(raw),
      config = this.controlledConfiguration(),
      state = this.state();
    this.requireActionInstallation(
      q.control,
      config.authority_id,
      state.cursor.lot_id,
    );
    return this.actionOutcomes.lookup(
      q,
      config.authority_id,
      state.cursor.lot_id,
    );
  }
  private requireActionInstallation(
    control: ActionControl,
    authority: string,
    lot: string,
  ) {
    const row = this.ctx.storage.sql
      .exec<{
        principal: string;
        export_json: string;
      }>(
        "SELECT principal,export_json FROM actor_controls WHERE avatar_id=?",
        control.avatar_id,
      )
      .toArray()[0];
    if (!row) throw Error("player action has no durable installation");
    const e = exportV2(JSON.parse(row.export_json));
    if (
      row.principal !== control.principal ||
      e.authority_id !== authority ||
      e.lot_id !== lot ||
      e.account_id !== control.account_id ||
      e.avatar_id !== control.avatar_id ||
      e.control_generation !== control.control_generation ||
      e.session_ref !== control.session_ref
    )
      throw Error("player action authenticated control mismatch");
  }
  /** Fence receipt acknowledges durable recording only, never avatar detach or custody release. */
  recordControlFence(raw: unknown) {
    const fence = controlFence(raw);
    return this.ctx.storage.transactionSync(() =>
      this.applyControlFence(fence),
    );
  }
  private applyControlFence(fence: ControlFence) {
    const state = this.state();
    if (state.cursor.lot_id !== fence.lot_id)
      throw Error("control fence lot mismatch");
    const existing = this.ctx.storage.sql
      .exec<{
        receipt: string;
      }>(
        "SELECT receipt FROM control_fences WHERE authority=? AND avatar_id=? AND generation=?",
        fence.authority,
        fence.avatar_id,
        fence.generation,
      )
      .toArray()[0];
    if (existing) {
      const receipt = JSON.parse(existing.receipt);
      if (!equal(receipt.fence, fence))
        throw Error("control fence identity reused");
      return receipt;
    }
    if (
      this.ctx.storage.sql
        .exec<{
          count: number;
        }>("SELECT COUNT(*) AS count FROM control_fences")
        .one().count >= 16384
    )
      throw Error("control fence capacity");
    const receipt = {
      status: "fenced",
      fence,
      observed_cursor: state.cursor,
    };
    this.ctx.storage.sql.exec(
      "INSERT INTO control_fences(authority,avatar_id,generation,receipt) VALUES(?,?,?,?)",
      fence.authority,
      fence.avatar_id,
      fence.generation,
      canonical(receipt),
    );
    const control = this.controlState();
    if (control)
      this.ctx.storage.sql.exec(
        "UPDATE lot_control_state SET revision=? WHERE id=1",
        successor(control.revision),
      );
    const config = this.ctx.storage.sql
      .exec<{ value: string }>("SELECT value FROM controlled_config WHERE id=1")
      .toArray()[0];
    if (config && JSON.parse(config.value).authority_id === fence.authority) {
      const row = this.ctx.storage.sql
        .exec<{
          export_json: string;
        }>(
          "SELECT export_json FROM actor_controls WHERE avatar_id=?",
          fence.avatar_id,
        )
        .toArray()[0];
      if (
        row &&
        BigInt(JSON.parse(row.export_json).control_generation) <=
          BigInt(fence.generation)
      )
        this.ctx.storage.sql.exec(
          "UPDATE actor_controls SET state='fence_pending' WHERE avatar_id=? AND state IN ('installed','active')",
          fence.avatar_id,
        );
    }
    return receipt;
  }
  protected now() {
    return Date.now();
  }
  private controlledConfiguration() {
    if (!this.controlState()) throw Error("lot is not controlled_v2");
    const row = this.ctx.storage.sql
      .exec<{ value: string }>("SELECT value FROM controlled_config WHERE id=1")
      .toArray()[0];
    if (!row) throw Error("controlled admission initialization required");
    return JSON.parse(row.value) as ReturnType<typeof controlledSetup>;
  }
  private coveringFence(authority: string, avatar: string, generation: string) {
    return (
      this.ctx.storage.sql
        .exec(
          "SELECT generation FROM control_fences WHERE authority=? AND avatar_id=? AND (length(generation)>length(?) OR (length(generation)=length(?) AND generation>=?)) LIMIT 1",
          authority,
          avatar,
          generation,
          generation,
          generation,
        )
        .toArray().length !== 0
    );
  }
  private expireControls(now = this.now()) {
    const expired = this.ctx.storage.sql
      .exec<{
        export_json: string;
      }>(
        "SELECT export_json FROM actor_controls WHERE state IN ('installed','active') AND expires_at<=? LIMIT ?",
        now,
        MAX_CONTROLS,
      )
      .toArray();
    for (const row of expired) {
      const e = exportV2(JSON.parse(row.export_json));
      this.applyControlFence({
        version: 1,
        authority: e.authority_id,
        avatar_id: e.avatar_id,
        generation: e.control_generation,
        lot_id: e.lot_id,
        request_digest: e.reservation_digest,
      });
    }
    return expired.length > 0;
  }
  inspectControlledV2() {
    const config = this.controlledConfiguration();
    return this.ctx.storage.transactionSync(() => {
      this.expireControls();
      const registry = this.ctx.storage.sql
        .exec<{
          sha256: string;
          value: string;
        }>("SELECT sha256,value FROM controlled_registry WHERE id=1")
        .one();
      return {
        ...this.inspect(),
        control_revision: this.controlState()!.revision,
        permission_binding: config.permission_binding,
        actor_registry_sha256: registry.sha256,
        actors: actorRegistry(JSON.parse(registry.value)),
        controls: this.ctx.storage.sql
          .exec<{
            state: string;
            expires_at: number;
            record: string;
            receipt: string | null;
          }>(
            "SELECT state,expires_at,record,receipt FROM actor_controls ORDER BY length(principal),principal LIMIT ?",
            MAX_CONTROLS,
          )
          .toArray()
          .map((row) => ({
            ...JSON.parse(row.record),
            state: row.state,
            expires_at: row.expires_at,
            receipt: row.receipt ? JSON.parse(row.receipt) : null,
          })),
      };
    });
  }
  async installReservedAvatarV2(raw: unknown) {
    const e = exportV2(raw),
      key = controlKey(e);
    if (!this.env.ACCOUNTS || !this.env.ACCOUNT_REALM)
      throw Error("private account authority binding unavailable");
    const account = this.env.ACCOUNTS.get(
      this.env.ACCOUNTS.idFromName(this.env.ACCOUNT_REALM),
    );
    const validated = await account.revalidateReservedAvatarV2(e);
    if (validated.status !== "validated" || !equal(validated.export, e))
      throw Error("account validation mismatch");
    const { export_digest, ...payload } = e;
    if (
      (await digest(encode(payload))) !== export_digest ||
      (await digest(new TextEncoder().encode(e.portable_json))) !==
        e.portable_sha256
    )
      throw Error("avatar export digest mismatch");
    return this.ctx.storage.transactionSync(() => {
      const config = this.controlledConfiguration(),
        state = this.state();
      this.assertActive();
      const expired = this.expireControls();
      if (expired)
        return {
          status: "rejected" as const,
          reason: "lease_expired",
          control_revision: this.controlState()!.revision,
        };
      if (
        config.authority_id !== e.authority_id ||
        config.content_release !== e.content_release ||
        config.protocol_release !== e.protocol_release ||
        state.cursor.lot_id !== e.lot_id
      )
        throw Error("avatar export realm, release or destination mismatch");
      if (!equal(config.permission_binding, e.permission_binding))
        throw Error("avatar permission binding mismatch");
      if (this.coveringFence(e.authority_id, e.avatar_id, e.control_generation))
        return {
          status: "rejected" as const,
          reason: "control_fenced",
          control_revision: this.controlState()!.revision,
        };
      const prior = this.ctx.storage.sql
        .exec<{
          export_digest: string;
          state: string;
          record: string;
        }>(
          "SELECT export_digest,state,record FROM actor_controls WHERE control_key=?",
          key,
        )
        .toArray()[0];
      if (prior) {
        if (prior.export_digest !== e.export_digest)
          throw Error("admission identity reused");
        if (prior.state !== "installed" && prior.state !== "active")
          return {
            status: "rejected" as const,
            reason: "control_fenced",
            control_revision: this.controlState()!.revision,
          };
        return {
          ...JSON.parse(prior.record),
          state: prior.state,
          control_revision: this.controlState()!.revision,
        };
      }
      if (
        this.ctx.storage.sql
          .exec(
            "SELECT avatar_id FROM actor_controls WHERE avatar_id=?",
            e.avatar_id,
          )
          .toArray().length
      )
        throw Error("avatar controller already installed");
      if (
        this.ctx.storage.sql
          .exec<{ n: number }>("SELECT COUNT(*) AS n FROM actor_controls")
          .one().n >= MAX_CONTROLS
      )
        throw Error("controlled actor capacity");
      const now = this.now(),
        expires_at = validated.lease.expires_at;
      // Account/lot clocks must be qualified together. No rounding or grace can
      // extend the authority's signed deadline; excessive skew fails closed.
      if (
        expires_at <= now ||
        expires_at - now > 60000 ||
        validated.validated_at > now + 1000 ||
        now - validated.validated_at > 5000
      )
        return {
          status: "rejected" as const,
          reason: "lease_expired",
          control_revision: this.controlState()!.revision,
        };
      const principal = successor(
          this.ctx.storage.sql
            .exec<{
              principal: string;
            }>("SELECT principal FROM controlled_sequence WHERE id=1")
            .one().principal,
        ),
        revision = successor(this.controlState()!.revision);
      const record = {
        schema_version: 2,
        status: "installed",
        control_key: key,
        principal,
        account_id: e.account_id,
        avatar_id: e.avatar_id,
        control_generation: e.control_generation,
        session_ref: e.session_ref,
        admission_operation: e.admission_operation,
        reservation_digest: e.reservation_digest,
        export_digest: e.export_digest,
        expires_at,
      };
      this.ctx.storage.sql.exec(
        "INSERT INTO actor_controls(control_key,avatar_id,principal,export_digest,export_json,state,expires_at,record) VALUES(?,?,?,?,?,'installed',?,?)",
        key,
        e.avatar_id,
        principal,
        e.export_digest,
        canonical(e),
        expires_at,
        canonical(record),
      );
      this.ctx.storage.sql.exec(
        "INSERT INTO controlled_leases(control_key,revision) VALUES(?,?)",
        key,
        validated.lease.revision,
      );
      this.ctx.storage.sql.exec(
        "UPDATE controlled_sequence SET principal=? WHERE id=1",
        principal,
      );
      this.ctx.storage.sql.exec(
        "UPDATE lot_control_state SET revision=? WHERE id=1",
        revision,
      );
      return { ...record, state: "installed", control_revision: revision };
    });
  }
  async renewControlLeaseV2(raw: unknown) {
    const v = fields(raw, [
      "schema_version",
      "status",
      "authority_id",
      "account_id",
      "avatar_id",
      "control_generation",
      "admission_operation",
      "export_digest",
      "session_ref",
      "lease_revision",
      "expires_at",
      "renewal_operation",
    ]);
    if (
      v.schema_version !== 2 ||
      v.status !== "renewed" ||
      !Number.isSafeInteger(v.expires_at)
    )
      throw Error("invalid control renewal");
    const lease_revision = decimal(v.lease_revision),
      export_digest = hash(v.export_digest);
    if (!this.env.ACCOUNTS || !this.env.ACCOUNT_REALM)
      throw Error("private account authority binding unavailable");
    const account = this.env.ACCOUNTS.get(
        this.env.ACCOUNTS.idFromName(this.env.ACCOUNT_REALM),
      ),
      validated = await account.revalidateControlLeaseV2(raw);
    if (validated.status !== "validated" || !equal(validated.lease, raw))
      throw Error("account lease validation mismatch");
    return this.ctx.storage.transactionSync(() => {
      const now = this.now();
      this.controlledConfiguration();
      this.assertActive();
      if (this.expireControls(now))
        return {
          status: "rejected",
          reason: "lease_expired",
          control_revision: this.controlState()!.revision,
        };
      const row = this.ctx.storage.sql
        .exec<{
          control_key: string;
          export_json: string;
          state: string;
          expires_at: number;
          record: string;
        }>(
          "SELECT control_key,export_json,state,expires_at,record FROM actor_controls WHERE export_digest=?",
          export_digest,
        )
        .toArray()[0];
      if (!row) throw Error("unknown installed lease");
      const e = exportV2(JSON.parse(row.export_json));
      if (
        e.authority_id !== v.authority_id ||
        e.account_id !== v.account_id ||
        e.avatar_id !== v.avatar_id ||
        e.control_generation !== v.control_generation ||
        e.session_ref !== v.session_ref ||
        e.admission_operation !== v.admission_operation
      )
        throw Error("control renewal identity mismatch");
      if (
        (row.state !== "active" && row.state !== "installed") ||
        this.coveringFence(e.authority_id, e.avatar_id, e.control_generation)
      )
        return {
          status: "rejected",
          reason: "control_fenced",
          control_revision: this.controlState()!.revision,
        };
      const old = this.ctx.storage.sql
        .exec<{
          revision: string;
        }>(
          "SELECT revision FROM controlled_leases WHERE control_key=?",
          row.control_key,
        )
        .one();
      if (lease_revision === old.revision) {
        if (v.expires_at !== row.expires_at)
          throw Error("control lease revision reused");
        return {
          status: "renewed",
          control_revision: this.controlState()!.revision,
          expires_at: row.expires_at,
          lease_revision,
        };
      }
      if (BigInt(lease_revision) <= BigInt(old.revision))
        throw Error("stale control lease revision");
      const expiry = v.expires_at as number;
      if (
        expiry <= now ||
        expiry - now > 60000 ||
        expiry < row.expires_at ||
        validated.validated_at > now + 1000 ||
        now - validated.validated_at > 5000
      )
        return {
          status: "rejected",
          reason: "lease_expired",
          control_revision: this.controlState()!.revision,
        };
      const revision = successor(this.controlState()!.revision);
      this.ctx.storage.sql.exec(
        "UPDATE controlled_leases SET revision=? WHERE control_key=?",
        lease_revision,
        row.control_key,
      );
      this.ctx.storage.sql.exec(
        "UPDATE actor_controls SET expires_at=?,record=? WHERE control_key=?",
        expiry,
        canonical({ ...JSON.parse(row.record), expires_at: expiry }),
        row.control_key,
      );
      this.ctx.storage.sql.exec(
        "UPDATE lot_control_state SET revision=? WHERE id=1",
        revision,
      );
      return {
        status: "renewed",
        control_revision: revision,
        expires_at: expiry,
        lease_revision,
      };
    });
  }
  /** Ordinary actions/admissions/ticks retain the fence and actor-removal guard. */
  async commitControlledBatchV2(raw: unknown) {
    const p = controlledProposal(raw);
    if (p.payload.departure_v1)
      throw Error("departure evidence requires trusted drain lane");
    return this.commitControlledTransition(p, false);
  }
  /** Private native-host only. Never exposed by browser RPC dispatch. */
  async commitOwnedDrainV1(raw: unknown) {
    const p = controlledProposal(raw);
    if (!p.payload.departure_v1 || p.payload.action_v1)
      throw Error(
        "trusted drain requires departure evidence without player action",
      );
    return this.commitControlledTransition(p, true);
  }
  private async commitControlledTransition(
    p: ReturnType<typeof controlledProposal>,
    drain: boolean,
  ) {
    const bytes = encode(p.payload),
      c = p.payload.control_v2;
    if (bytes.length > 16 * 1024 * 1024) throw Error("proposal too large");
    if (
      (await digest(bytes)) !== p.digest ||
      (await digest(encode(c.actors))) !== c.actor_registry_after_sha256
    )
      throw Error("controlled proposal digest mismatch");
    await verifyActionEvidence(p.payload);
    await verifyDepartureEvidence(p.payload);
    const departureEvidenceHash = p.payload.departure_v1
      ? await digest(encode(p.payload.departure_v1))
      : null;
    const actionHashes = await actionOutcomeHashes(p.payload, p.digest);
    const key = p.payload.binding.lot_incarnation + ":" + p.payload.batch_id,
      serialized = new TextDecoder().decode(bytes);
    // Exact terminal retries must survive actor-controls retirement. A proposal
    // without a terminal decision still needs the current installation below.
    const decided =
      this.ctx.storage.sql
        .exec(
          "SELECT batch_key FROM batches WHERE batch_key=? UNION ALL SELECT batch_key FROM controlled_denials WHERE batch_key=?",
          key,
          key,
        )
        .toArray().length > 0;
    const lineageAdvance = !decided
      ? await this.departures.prepareAdvance(
          this.state(),
          {
            binding: p.payload.binding,
            cursor: p.payload.result,
            checkpoint_sha256: p.payload.checkpoint_sha256,
          },
          "batch",
          key,
        )
      : null;
    const departure =
      drain && lineageAdvance
        ? await this.departures.prepare(
            p,
            this.controlledConfiguration(),
            lineageAdvance,
          )
        : null;
    const saveReceiptHash = await this.departures.receiptHashes(departure);
    const lineageHeads = this.departures.lineageHeads(lineageAdvance);
    return this.ctx.storage.transactionSync(() => {
      const actionAuthority = this.controlledConfiguration().authority_id;
      const previous = this.ctx.storage.sql
        .exec<{
          digest: string;
          receipt: string;
        }>("SELECT digest,receipt FROM batches WHERE batch_key=?", key)
        .toArray()[0];
      if (previous) {
        if (previous.digest !== p.digest) throw Error("batch identity reused");
        if (
          actionHashes &&
          JSON.parse(previous.receipt).action_outcome_sha256 !==
            actionHashes.committed
        )
          throw Error("player action durable decision mismatch");
        if (
          (JSON.parse(previous.receipt).departure_evidence_sha256 ?? null) !==
          departureEvidenceHash
        )
          throw Error("departure durable decision mismatch");
        this.actionOutcomes.assertReplay(p.payload, p.digest, "committed");
        return JSON.parse(previous.receipt);
      }
      const denied = this.ctx.storage.sql
        .exec<{
          digest: string;
          receipt: string;
        }>(
          "SELECT digest,receipt FROM controlled_denials WHERE batch_key=?",
          key,
        )
        .toArray()[0];
      if (denied) {
        if (denied.digest !== p.digest) throw Error("batch identity reused");
        if (
          actionHashes &&
          JSON.parse(denied.receipt).action_outcome_sha256 !==
            actionHashes.rejected
        )
          throw Error("player action durable decision mismatch");
        this.actionOutcomes.assertReplay(p.payload, p.digest, "rejected");
        return JSON.parse(denied.receipt);
      }
      this.assertActive();
      const state = this.state(),
        transactionNow = this.now();
      if (
        !equal(state.binding, p.payload.binding) ||
        !equal(state.cursor, p.payload.predecessor)
      )
        throw Error("stale binding or predecessor");
      this.actionOutcomes.assertFresh(
        actionAuthority,
        state.cursor.lot_id,
        p.payload,
      );
      if (p.payload.action_v1) {
        const action = p.payload.action_v1,
          request = decodeActionRequest(action.request_hex),
          registry = this.ctx.storage.sql
            .exec<{
              value: string;
            }>("SELECT value FROM controlled_registry WHERE id=1")
            .one(),
          before = actorRegistry(JSON.parse(registry.value)),
          actor = before.find((a) => a.principal === action.control.principal);
        this.requireActionInstallation(
          action.control,
          actionAuthority,
          state.cursor.lot_id,
        );
        if (
          !actor ||
          !equal(request.actor, actor.actor) ||
          !equal(c.actors, before) ||
          actor.account_id !== action.control.account_id ||
          actor.avatar_id !== action.control.avatar_id ||
          actor.control_generation !== action.control.control_generation ||
          actor.session_ref !== action.control.session_ref
        )
          throw Error("player action registry or actor mismatch");
      }
      if (this.expireControls(transactionNow)) {
        const receipt = {
          status: "rejected",
          ...(actionHashes
            ? { action_outcome_sha256: actionHashes.rejected }
            : {}),
          reason: "lease_expired",
          batch_id: p.payload.batch_id,
          digest: p.digest,
          control_revision: this.controlState()!.revision,
        };
        this.ctx.storage.sql.exec(
          "INSERT INTO controlled_denials(batch_key,digest,receipt) VALUES(?,?,?)",
          key,
          p.digest,
          canonical(receipt),
        );
        this.actionOutcomes.record(
          actionAuthority,
          state.cursor.lot_id,
          p.payload,
          p.digest,
          "rejected",
        );
        return receipt; // Never throw: the expiry tombstone must survive rejection.
      }
      if (
        !drain &&
        this.ctx.storage.sql
          .exec(
            "SELECT control_key FROM actor_controls WHERE state='fence_pending' LIMIT 1",
          )
          .toArray().length
      ) {
        const receipt = {
          status: "rejected",
          ...(actionHashes
            ? { action_outcome_sha256: actionHashes.rejected }
            : {}),
          reason: "fence_drain_required",
          batch_id: p.payload.batch_id,
          digest: p.digest,
          control_revision: this.controlState()!.revision,
        };
        this.ctx.storage.sql.exec(
          "INSERT INTO controlled_denials(batch_key,digest,receipt) VALUES(?,?,?)",
          key,
          p.digest,
          canonical(receipt),
        );
        this.actionOutcomes.record(
          actionAuthority,
          state.cursor.lot_id,
          p.payload,
          p.digest,
          "rejected",
        );
        return receipt;
      }
      const revision = this.controlState()!.revision,
        config = this.controlledConfiguration(),
        registry = this.ctx.storage.sql
          .exec<{
            sha256: string;
            value: string;
          }>("SELECT sha256,value FROM controlled_registry WHERE id=1")
          .one();
      if (
        c.revision_before !== revision ||
        c.actor_registry_before_sha256 !== registry.sha256 ||
        !equal(c.permission_before, config.permission_binding)
      )
        throw Error("stale control revision, registry or permissions");
      const before = actorRegistry(JSON.parse(registry.value)),
        known = new Map(before.map((actor) => [actor.principal, actor]));
      for (const actor of before)
        if (!drain && !c.actors.some((next) => equal(actor, next)))
          throw Error(
            "ordinary controlled transition removed or changed an actor",
          );
      const additions = c.actors.filter((actor) => !known.has(actor.principal));
      if (additions.length > (drain ? 0 : 1))
        throw Error("admission transition bound");
      if (drain) {
        if (!departure) throw Error("departure preparation missing");
        this.departures.validate(p, before, config, departure);
      }
      if (
        !drain &&
        c.revision_after !== (additions.length ? successor(revision) : revision)
      )
        throw Error("controlled revision transition mismatch");
      if (!additions.length && !equal(c.permission_after, c.permission_before))
        throw Error("ordinary tick changed permission metadata");
      if (
        c.permission_after.sha256 === c.permission_before.sha256
          ? c.permission_after.revision !== c.permission_before.revision
          : c.permission_after.revision !==
            successor(c.permission_before.revision)
      )
        throw Error("permission revision transition mismatch");
      for (const actor of c.actors) {
        const row = this.ctx.storage.sql
          .exec<{
            export_json: string;
            state: string;
            principal: string;
            expires_at: number;
          }>(
            "SELECT export_json,state,principal,expires_at FROM actor_controls WHERE avatar_id=?",
            actor.avatar_id,
          )
          .toArray()[0];
        if (!row) throw Error("actor has no durable installation");
        const e = exportV2(JSON.parse(row.export_json));
        if (
          row.principal !== actor.principal ||
          e.account_id !== actor.account_id ||
          e.control_generation !== actor.control_generation ||
          e.session_ref !== actor.session_ref ||
          e.admission_operation !== actor.admission_operation ||
          e.reservation_digest !== actor.reservation_digest ||
          e.export_digest !== actor.export_digest ||
          (!drain &&
            (row.expires_at <= transactionNow ||
              this.coveringFence(
                e.authority_id,
                e.avatar_id,
                e.control_generation,
              )))
        )
          throw Error("actor reservation, session, lease or fence mismatch");
        if (
          known.has(actor.principal)
            ? drain
              ? !["active", "fence_pending"].includes(row.state)
              : row.state !== "active"
            : row.state !== "installed"
        )
          throw Error("invalid actor installation state");
        if (
          !known.has(actor.principal) &&
          !equal(e.permission_binding, c.permission_before)
        )
          throw Error("admission permission source stale");
      }
      // Count/bytes, admission receipt, registry, cursor, journal and outboxes are
      // in this same SQLite transaction, after final lease/fence checks.
      const totals = this.ctx.storage.sql
        .exec<{
          count: number;
          bytes: number;
        }>(
          "SELECT COUNT(*) AS count,COALESCE(SUM(bytes),0) AS bytes FROM journal_live",
        )
        .one();
      if (totals.count >= 4096 || totals.bytes + bytes.length > JOURNAL_LIMIT)
        throw Error(
          "journal capacity requires qualified checkpoint compaction",
        );
      const receipt = {
        status: "committed",
        ...(Object.keys(lineageHeads).length
          ? { departure_lineage_tips: lineageHeads }
          : {}),
        ...(departure
          ? {
              departure_evidence_sha256: departure.evidence_sha256,
              ...(departure.lineage_sha256
                ? { departure_lineage_sha256: departure.lineage_sha256 }
                : {}),
              ...(saveReceiptHash
                ? { departure_save_receipt_sha256: saveReceiptHash }
                : {}),
              ...(departure.detach_sha256
                ? { departure_detach_receipt_sha256: departure.detach_sha256 }
                : {}),
            }
          : {}),
        ...(actionHashes
          ? { action_outcome_sha256: actionHashes.committed }
          : {}),
        batch_id: p.payload.batch_id,
        digest: p.digest,
        cursor: p.payload.result,
        control_revision: c.revision_after,
        actor_registry_sha256: c.actor_registry_after_sha256,
      };
      this.ctx.storage.sql.exec(
        "INSERT INTO batches(batch_key,digest,receipt,bytes) VALUES(?,?,?,?)",
        key,
        p.digest,
        canonical(receipt),
        bytes.length,
      );
      this.ctx.storage.sql.exec(
        "INSERT INTO journal_live(batch_key,bytes) VALUES(?,?)",
        key,
        bytes.length,
      );
      for (
        let offset = 0, ordinal = 0;
        offset < serialized.length;
        offset += CHUNK, ordinal++
      )
        this.ctx.storage.sql.exec(
          "INSERT INTO journal_chunks(batch_key,ordinal,payload) VALUES(?,?,?)",
          key,
          ordinal,
          serialized.slice(offset, offset + CHUNK),
        );
      for (const [lane, data] of [
        ["effects", p.payload.effects_hex],
        ["events", p.payload.events_hex],
      ])
        if (data !== "0000000000000000")
          this.ctx.storage.sql.exec(
            "INSERT INTO outbox(batch_key,lane,digest) VALUES(?,?,?)",
            key,
            lane,
            p.digest,
          );
      for (const actor of additions) {
        const row = this.ctx.storage.sql
          .exec<{
            control_key: string;
            export_digest: string;
          }>(
            "SELECT control_key,export_digest FROM actor_controls WHERE principal=?",
            actor.principal,
          )
          .one();
        const admission = { ...receipt, actor };
        this.ctx.storage.sql.exec(
          "INSERT INTO admission_operations(control_key,export_digest,receipt) VALUES(?,?,?)",
          row.control_key,
          row.export_digest,
          canonical(admission),
        );
        this.ctx.storage.sql.exec(
          "UPDATE actor_controls SET state='active',receipt=? WHERE control_key=?",
          canonical(admission),
          row.control_key,
        );
      }
      this.ctx.storage.sql.exec(
        "UPDATE controlled_registry SET sha256=?,value=? WHERE id=1",
        c.actor_registry_after_sha256,
        canonical(c.actors),
      );
      this.ctx.storage.sql.exec(
        "UPDATE controlled_config SET value=? WHERE id=1",
        canonical({ ...config, permission_binding: c.permission_after }),
      );
      this.ctx.storage.sql.exec(
        "UPDATE lot_control_state SET revision=? WHERE id=1",
        c.revision_after,
      );
      this.ctx.storage.sql.exec(
        "UPDATE lot_state SET value=? WHERE id=1",
        canonical({
          binding: state.binding,
          cursor: p.payload.result,
          checkpoint_sha256: p.payload.checkpoint_sha256,
        }),
      );
      if (!lineageAdvance) throw Error("departure lineage advance missing");
      this.departures.recordAdvance(lineageAdvance);
      this.departures.recordAnchor(p, departure);
      if (departure) this.departures.record(p, departure, saveReceiptHash);
      this.actionOutcomes.record(
        actionAuthority,
        state.cursor.lot_id,
        p.payload,
        p.digest,
        "committed",
      );
      return receipt;
    });
  }
  /** Private service request; this acknowledges fencing, not successful leave. */
  beginOwnedDepartureV1(raw: unknown) {
    const request = departureRequestV1(raw);
    return this.ctx.storage.transactionSync(() => {
      const prior = this.ctx.storage.sql
        .exec(
          "SELECT control_key FROM owned_departures WHERE control_key=?",
          departureControlKey(request.identity),
        )
        .toArray()[0];
      if (prior) return this.departures.lookup(request);
      this.assertActive();
      const config = this.controlledConfiguration(),
        state = this.state();
      const installed = this.departures.requireInstallation(
        request.identity,
        config,
        state.cursor.lot_id,
      );
      if (!["active", "fence_pending"].includes(installed.row.state))
        throw Error("departure actor was never admitted");
      const registry = this.ctx.storage.sql
        .exec<{
          value: string;
        }>("SELECT value FROM controlled_registry WHERE id=1")
        .one();
      const {
        authority_id: _authority,
        lot_id: _lot,
        ...entry
      } = request.identity;
      if (
        !actorRegistry(JSON.parse(registry.value)).some((actor) =>
          equal(actor, entry),
        )
      )
        throw Error("departure actor is not in committed registry");
      this.applyControlFence(departureFence(request.identity));
      return this.departures.begin(request);
    });
  }
  lookupOwnedDepartureV1(raw: unknown) {
    return this.departures.lookup(raw);
  }
  findOwnedDepartureV1(raw: unknown) {
    return this.departures.find(raw);
  }
  activeOwnedDeparturesV1(after: unknown = "") {
    return this.departures.active(after);
  }
  pendingOwnedDeparturesV1() {
    return this.departures.pending();
  }
  lookupCommittedAvatarSaveV1(raw: unknown) {
    return this.departures.lookupSave(raw);
  }
  lookupCommittedAvatarDetachV1(raw: unknown) {
    return this.departures.lookupDetach(raw);
  }

  /** Retire only after querying the configured account authority. A raw caller
   * release receipt is not accepted as proof and cannot free a newer control. */
  async retireReleasedAvatarControlV1(raw: unknown) {
    const reference = detachProofReferenceV1(raw);
    const detach = await this.departures.lookupDetach(reference);
    const realm = this.env.ACCOUNT_REALM ?? this.env.NATIVE_ACCOUNT_REALM;
    if (!this.env.ACCOUNTS || !realm)
      throw Error("private account authority binding unavailable");
    const account = this.env.ACCOUNTS.get(this.env.ACCOUNTS.idFromName(realm));
    const release = await account.lookupReleasedAvatarControlV1({
      identity: reference.identity,
      departure_operation: reference.departure_operation,
    });
    const r = fields(release, [
      "schema_version",
      "status",
      "identity",
      "departure_operation",
      "save_sha256",
      "account_save_receipt_sha256",
      "detach_sha256",
      "committed_detach",
    ]);
    if (
      r.schema_version !== 1 ||
      r.status !== "released_saved_control" ||
      !equal(r.identity, reference.identity) ||
      r.departure_operation !== reference.departure_operation ||
      r.detach_sha256 !== reference.detach_sha256 ||
      r.save_sha256 !== detach.source_save.save_sha256 ||
      !equal(r.committed_detach, detach)
    )
      throw Error("account departure release acknowledgment mismatch");
    hash(r.account_save_receipt_sha256);
    return this.ctx.storage.transactionSync(() =>
      this.departures.retire(reference, release),
    );
  }

  getAdmissionV2(raw: unknown) {
    const e = exportV2(raw),
      key = controlKey(e);
    return this.ctx.storage.transactionSync(() => {
      this.controlledConfiguration();
      this.expireControls();
      const row = this.ctx.storage.sql
        .exec<{
          export_digest: string;
          state: string;
          receipt: string | null;
        }>(
          "SELECT export_digest,state,receipt FROM actor_controls WHERE control_key=?",
          key,
        )
        .toArray()[0];
      if (!row) return { status: "unknown" };
      if (row.export_digest !== e.export_digest)
        throw Error("admission identity reused");
      return {
        status: row.state,
        receipt: row.receipt ? JSON.parse(row.receipt) : null,
        control_revision: this.controlState()!.revision,
      };
    });
  }
  private controlState() {
    return this.ctx.storage.sql
      .exec<{
        revision: string;
      }>("SELECT revision FROM lot_control_state WHERE id=1")
      .toArray()[0];
  }
  /** Private read-only staged guard state; this is never an actor grant. */
  controlStateV2() {
    this.state();
    const state = this.controlState();
    if (!state) throw Error("lot is not controlled_v2");
    return controlStateV2({
      schema_version: 2,
      mode: "controlled_v2",
      revision: state.revision,
    });
  }
  private requireMutationMode(controlled: boolean) {
    if (!!this.controlState() !== controlled)
      throw Error(
        controlled
          ? "lot is not controlled_v2"
          : "controlled lot requires V2; lane unavailable",
      );
  }
  private requireLegacyMutation() {
    if (this.controlState())
      throw Error("controlled lot requires V2; lane unavailable");
  }
  private state(): State {
    const row = this.ctx.storage.sql
      .exec<{ value: string }>("SELECT value FROM lot_state WHERE id=1")
      .toArray()[0];
    if (!row) throw Error("lot not initialized");
    return JSON.parse(row.value);
  }
  private pendingRecovery() {
    return this.ctx.storage.sql
      .exec<{
        grant_json: string;
      }>("SELECT grant_json FROM recoveries WHERE status='pending' LIMIT 1")
      .toArray()[0];
  }
  private assertActive() {
    if (this.pendingRecovery())
      throw Error("recovery in progress; old owner fenced");
  }
  private controlledRecoveryState() {
    const config = this.controlledConfiguration(),
      registry = this.ctx.storage.sql
        .exec<{
          sha256: string;
          value: string;
        }>("SELECT sha256,value FROM controlled_registry WHERE id=1")
        .one();
    return {
      control_revision: this.controlState()!.revision,
      actor_registry_sha256: registry.sha256,
      actors: actorRegistry(JSON.parse(registry.value)),
      permission_binding: config.permission_binding,
    };
  }
  private parseRecoveryControl(raw: unknown) {
    const value = fields(raw, [
      "control_revision",
      "actor_registry_sha256",
      "actors",
      "permission_binding",
    ]);
    return {
      control_revision: decimal(value.control_revision),
      actor_registry_sha256: hash(value.actor_registry_sha256),
      actors: actorRegistry(value.actors),
      permission_binding: permissionBinding(value.permission_binding),
    };
  }
  beginControlledRecoveryV2(expectedIncarnation: unknown) {
    return this.ctx.storage.transactionSync(() => {
      this.controlledConfiguration();
      this.expireControls();
      const grant = this.beginRecoveryMode(expectedIncarnation, true);
      return {
        schema_version: 2,
        grant,
        control: this.controlledRecoveryState(),
      };
    });
  }
  async controlledRecoveryBundleV2(expectedIncarnation: unknown) {
    const control = this.ctx.storage.transactionSync(() => {
      this.expireControls();
      return this.controlledRecoveryState();
    });
    const bundle = await this.recoveryBundleMode(expectedIncarnation, true);
    if (!equal(control, this.controlledRecoveryState()))
      throw Error("controlled recovery changed during read");
    return { ...bundle, schema_version: 2, control };
  }
  async finishControlledRecoveryV2(raw: unknown, bytes: Uint8Array) {
    const v = fields(raw, ["schema_version", "marker", "control"]);
    if (v.schema_version !== 2) throw Error("unsupported controlled recovery");
    const control = this.parseRecoveryControl(v.control);
    if (
      (await digest(encode(control.actors))) !== control.actor_registry_sha256
    )
      throw Error("recovered actor registry digest mismatch");
    const result = await this.finishRecoveryMode(
      v.marker,
      bytes,
      true,
      control,
    );
    return { schema_version: 2, marker: result, control };
  }
  async publishControlledCheckpointV2(
    rawBinding: unknown,
    rawCursor: unknown,
    expected: unknown,
    bytes: Uint8Array,
    rawControl: unknown,
  ) {
    const control = this.parseRecoveryControl(rawControl);
    if (
      (await digest(encode(control.actors))) !== control.actor_registry_sha256
    )
      throw Error("checkpoint actor registry digest mismatch");
    return this.publishCheckpointMode(
      rawBinding,
      rawCursor,
      expected,
      bytes,
      true,
      control,
    );
  }
  beginRecovery(expectedIncarnation: unknown): RecoveryGrant {
    return this.beginRecoveryMode(expectedIncarnation, false);
  }
  private beginRecoveryMode(
    expectedIncarnation: unknown,
    controlled: boolean,
  ): RecoveryGrant {
    this.requireMutationMode(controlled);
    const incarnation = decimal(expectedIncarnation);
    return this.ctx.storage.transactionSync(() => {
      const prior = this.ctx.storage.sql
        .exec<{
          grant_json: string;
        }>(
          "SELECT grant_json FROM recoveries WHERE old_incarnation=?",
          incarnation,
        )
        .toArray()[0];
      if (prior) return JSON.parse(prior.grant_json);
      this.assertActive();
      const state = this.state();
      if (state.binding.lot_incarnation !== incarnation)
        throw Error("stale recovery incarnation");
      const grant: RecoveryGrant = {
        schema_version: 1,
        old_binding: state.binding,
        old_cursor: state.cursor,
        old_checkpoint_sha256: state.checkpoint_sha256,
        new_binding: {
          ...state.binding,
          lot_incarnation: successor(incarnation),
        },
        new_authority_epoch: successor(state.cursor.authority_epoch),
      };
      this.ctx.storage.sql.exec(
        "INSERT INTO recoveries(old_incarnation,status,grant_json) VALUES(?,'pending',?)",
        incarnation,
        canonical(grant),
      );
      return grant;
    });
  }
  async recoveryBundle(expectedIncarnation: unknown) {
    return this.recoveryBundleMode(expectedIncarnation, false);
  }
  private async recoveryBundleMode(
    expectedIncarnation: unknown,
    controlled: boolean,
  ) {
    this.requireMutationMode(controlled);
    const incarnation = decimal(expectedIncarnation);
    const pending = this.pendingRecovery();
    if (!pending) throw Error("no pending recovery");
    const grant = JSON.parse(pending.grant_json) as RecoveryGrant;
    if (grant.old_binding.lot_incarnation !== incarnation)
      throw Error("recovery incarnation mismatch");
    const manifests = this.listCheckpoints();
    const checkpoint = manifests.find(
      (m) =>
        equal(m.binding, grant.old_binding) &&
        m.cursor.lot_id === grant.old_cursor.lot_id &&
        m.cursor.authority_epoch === grant.old_cursor.authority_epoch &&
        BigInt(m.cursor.completed_tick) <=
          BigInt(grant.old_cursor.completed_tick),
    );
    if (!checkpoint) throw Error("no compatible retained checkpoint");
    if (controlled && !checkpoint.control_v2)
      throw Error("controlled checkpoint lacks registry manifest");
    if (
      !Number.isSafeInteger(checkpoint.byte_length) ||
      checkpoint.byte_length < 1 ||
      checkpoint.byte_length > 8 * 1024 * 1024
    )
      throw Error("checkpoint manifest bound");
    const object = await this.env.CHECKPOINTS.get(checkpoint.object_key);
    if (!object || object.size !== checkpoint.byte_length)
      throw Error("recovery checkpoint missing or wrong size");
    const bytes = new Uint8Array(await object.arrayBuffer());
    if ((await digest(bytes)) !== checkpoint.sha256)
      throw Error("recovery checkpoint digest mismatch");
    const current = this.pendingRecovery();
    if (!current || current.grant_json !== pending.grant_json)
      throw Error("recovery changed during bundle read");
    if (
      BigInt(grant.old_cursor.completed_tick) -
        BigInt(checkpoint.cursor.completed_tick) >
      64n
    )
      throw Error("recovery tail exceeds admitted profile");
    const rows = this.ctx.storage.sql
      .exec<{
        batch_key: string;
        digest: string;
        receipt: string;
      }>(
        "SELECT batch_key,digest,receipt FROM batches WHERE batch_key>=? AND batch_key<? AND json_extract(receipt,'$.cursor.authority_epoch')=? AND (length(json_extract(receipt,'$.cursor.completed_tick'))>length(?) OR (length(json_extract(receipt,'$.cursor.completed_tick'))=length(?) AND json_extract(receipt,'$.cursor.completed_tick')>?)) ORDER BY rowid LIMIT 65",
        incarnation + ":",
        incarnation + ";",
        grant.old_cursor.authority_epoch,
        checkpoint.cursor.completed_tick,
        checkpoint.cursor.completed_tick,
        checkpoint.cursor.completed_tick,
      )
      .toArray();
    if (rows.length > 64) throw Error("recovery tail exceeds admitted profile");
    const candidates = [];
    let tailBytes = 0;
    for (const row of rows) {
      const receipt = JSON.parse(row.receipt);
      if (
        !receipt.cursor ||
        receipt.cursor.authority_epoch !== grant.old_cursor.authority_epoch ||
        BigInt(receipt.cursor.completed_tick) <=
          BigInt(checkpoint.cursor.completed_tick)
      )
        continue;
      const [owner, id] = row.batch_key.split(":");
      if (owner !== incarnation) continue;
      let p;
      try {
        p = await this.readBatch(owner, id);
      } catch {
        throw Error("recovery journal integrity mismatch");
      }
      if (
        !p ||
        (await digest(encode(p))) !== row.digest ||
        !equal(p.result, receipt.cursor) ||
        receipt.digest !== row.digest ||
        receipt.batch_id !== id ||
        receipt.status !== "committed" ||
        p.batch_id !== id
      )
        throw Error("recovery journal integrity mismatch");
      try {
        p = (controlled ? controlledProposal : proposal)({
          kind: "proposal",
          status: "pending",
          digest: row.digest,
          payload: p,
        }).payload;
      } catch {
        throw Error("recovery journal contract mismatch");
      }
      tailBytes += p.ticks_hex.length / 2;
      if (tailBytes > 8 * 1024 * 1024)
        throw Error("recovery tail exceeds admitted profile");
      // Keep only accepted replay data; large effect/event lanes stay in the journal.
      candidates.push({ ...p, effects_hex: "", events_hex: "" });
    }
    // Hashing journal rows yields; ensure no completed/replaced recovery is advertised.
    const final = this.pendingRecovery();
    if (!final || final.grant_json !== pending.grant_json)
      throw Error("recovery changed during journal read");
    const chain = recoveryChain(
      grant.old_binding,
      checkpoint.cursor,
      grant.old_cursor,
      candidates,
    );
    return {
      schema_version: 1,
      grant,
      checkpoint,
      checkpoint_bytes: bytes,
      batches: chain.map((p) => ({
        batch_id: p.batch_id,
        ticks_hex: p.ticks_hex,
      })),
    };
  }
  async finishRecovery(rawMarker: unknown, bytes: Uint8Array) {
    return this.finishRecoveryMode(rawMarker, bytes, false);
  }
  private async finishRecoveryMode(
    rawMarker: unknown,
    bytes: Uint8Array,
    controlled: boolean,
    expectedControl?: ReturnType<LotCoordinator["controlledRecoveryState"]>,
  ) {
    this.requireMutationMode(controlled);
    const marker = takeoverMarker(rawMarker),
      grant = marker.grant;
    if (
      !(bytes instanceof Uint8Array) ||
      bytes.byteLength > 8 * 1024 * 1024 ||
      (await digest(bytes)) !== marker.checkpoint_sha256
    )
      throw Error("takeover checkpoint digest mismatch");
    const readRecord = () => {
      const row = this.ctx.storage.sql
        .exec<{
          status: string;
          grant_json: string;
          marker_json: string | null;
        }>(
          "SELECT status,grant_json,marker_json FROM recoveries WHERE old_incarnation=?",
          grant.old_binding.lot_incarnation,
        )
        .toArray()[0];
      if (!row || !equal(JSON.parse(row.grant_json), grant))
        throw Error("unknown or changed recovery grant");
      if (
        row.status === "committed" &&
        !equal(JSON.parse(row.marker_json!), marker)
      )
        throw Error("changed recovery completion");
      if (controlled && row.status === "committed") {
        const saved = this.ctx.storage.sql
          .exec<{
            control_json: string;
          }>(
            "SELECT control_json FROM controlled_recovery_receipts WHERE old_incarnation=?",
            grant.old_binding.lot_incarnation,
          )
          .toArray()[0];
        if (!saved) throw Error("changed controlled recovery completion");
        const { departure_lineage_tips: _tips, ...savedControl } = JSON.parse(
          saved.control_json,
        );
        if (!equal(savedControl, expectedControl))
          throw Error("changed controlled recovery completion");
      }
      return row;
    };
    if (readRecord().status === "committed") {
      await this.compactRecoveredJournal(marker);
      return marker;
    }
    const current = this.state();
    if (
      !equal(current.binding, grant.old_binding) ||
      !equal(current.cursor, grant.old_cursor)
    )
      throw Error("recovery predecessor changed");
    const c = marker.new_cursor,
      key = `lots/${c.lot_id}/${c.authority_epoch}/${c.completed_tick}/${marker.checkpoint_sha256}`;
    const existing = await this.env.CHECKPOINTS.get(key);
    if (existing) {
      if (
        existing.size !== bytes.byteLength ||
        (await digest(new Uint8Array(await existing.arrayBuffer()))) !==
          marker.checkpoint_sha256
      )
        throw Error("takeover checkpoint object corrupted");
    } else await this.env.CHECKPOINTS.put(key, bytes);
    const confirmed = await this.env.CHECKPOINTS.get(key);
    if (
      !confirmed ||
      confirmed.size !== bytes.byteLength ||
      (await digest(new Uint8Array(await confirmed.arrayBuffer()))) !==
        marker.checkpoint_sha256
    )
      throw Error("takeover checkpoint readback failed");
    const lineageAdvance = controlled
      ? await this.departures.prepareAdvance(
          current,
          {
            binding: grant.new_binding,
            cursor: marker.new_cursor,
            checkpoint_sha256: marker.checkpoint_sha256,
          },
          "recovery",
          grant.old_binding.lot_incarnation,
        )
      : null;
    const lineageHeads = this.departures.lineageHeads(lineageAdvance);
    const finished = this.ctx.storage.transactionSync(() => {
      if (readRecord().status === "committed") return marker;
      if (controlled) {
        if (this.expireControls())
          return {
            status: "rejected",
            reason: "lease_expired",
            control_revision: this.controlState()!.revision,
          };
        if (
          !expectedControl ||
          !equal(this.controlledRecoveryState(), expectedControl)
        )
          throw Error("controlled recovery revision or registry changed");
      }
      const now = this.state();
      if (
        !equal(now.binding, grant.old_binding) ||
        !equal(now.cursor, grant.old_cursor)
      )
        throw Error("recovery predecessor changed");
      const manifest = {
        ...(controlled ? { control_v2: expectedControl } : {}),
        schema_version: 1,
        binding: grant.new_binding,
        cursor: c,
        object_key: key,
        sha256: marker.checkpoint_sha256,
        byte_length: bytes.byteLength,
      };
      this.ctx.storage.sql.exec(
        "INSERT INTO checkpoints(object_key,manifest) VALUES(?,?)",
        key,
        canonical(manifest),
      );
      this.ctx.storage.sql.exec(
        "UPDATE lot_state SET value=? WHERE id=1",
        canonical({
          binding: grant.new_binding,
          cursor: c,
          checkpoint_sha256: marker.checkpoint_sha256,
        }),
      );
      if (controlled)
        this.ctx.storage.sql.exec(
          "INSERT INTO controlled_recovery_receipts(old_incarnation,control_json) VALUES(?,?)",
          grant.old_binding.lot_incarnation,
          canonical({
            ...expectedControl,
            ...(Object.keys(lineageHeads).length
              ? { departure_lineage_tips: lineageHeads }
              : {}),
          }),
        );
      if (lineageAdvance) this.departures.recordAdvance(lineageAdvance);
      this.ctx.storage.sql.exec(
        "UPDATE recoveries SET status='committed',marker_json=? WHERE old_incarnation=?",
        canonical(marker),
        grant.old_binding.lot_incarnation,
      );
      return marker;
    });
    if (!("status" in finished)) await this.compactRecoveredJournal(marker);
    return finished;
  }
  private async compactRecoveredJournal(
    marker: ReturnType<typeof takeoverMarker>,
  ) {
    const c = marker.new_cursor;
    const key = `lots/${c.lot_id}/${c.authority_epoch}/${c.completed_tick}/${marker.checkpoint_sha256}`;
    const row = this.ctx.storage.sql
      .exec<{
        manifest: string;
      }>("SELECT manifest FROM checkpoints WHERE object_key=?", key)
      .toArray()[0];
    if (!row) throw Error("recovery checkpoint manifest missing");
    const manifest = JSON.parse(row.manifest);
    if (
      !equal(manifest.binding, marker.grant.new_binding) ||
      !equal(manifest.cursor, c) ||
      manifest.sha256 !== marker.checkpoint_sha256
    )
      throw Error("recovery checkpoint manifest changed");
    // A committed takeover proves that this exact old cursor was restored.
    // Readback again on retry before tiering its old-incarnation journal tail.
    await this.readArchive(key, manifest.sha256, manifest.byte_length);
    await this.compactJournal(
      manifest,
      marker.grant.old_binding,
      marker.grant.old_cursor,
    );
  }
  initialize(rawBinding: unknown, rawCursor: unknown, checkpointHash: unknown) {
    return this.initializeMode(rawBinding, rawCursor, checkpointHash, false);
  }
  /** Explicit new-lot boundary; old guard-only callers cannot install actors. */
  initializeControlledV2(
    rawBinding: unknown,
    rawCursor: unknown,
    checkpointHash: unknown,
    rawSetup?: unknown,
  ) {
    const setup = rawSetup === undefined ? null : controlledSetup(rawSetup);
    return this.ctx.storage.transactionSync(() => {
      const initialized = this.initializeMode(
        rawBinding,
        rawCursor,
        checkpointHash,
        true,
      );
      if (setup) {
        const prior = this.ctx.storage.sql
          .exec<{
            value: string;
          }>("SELECT value FROM controlled_config WHERE id=1")
          .toArray()[0];
        if (prior && !equal(JSON.parse(prior.value), setup))
          throw Error("controlled setup mismatch");
        if (!prior && this.controlState()!.revision !== "0")
          throw Error("controlled setup boundary changed");
        this.ctx.storage.sql.exec(
          "INSERT OR IGNORE INTO controlled_config(id,value) VALUES(1,?)",
          canonical(setup),
        );
        this.ctx.storage.sql.exec(
          "INSERT OR IGNORE INTO controlled_registry(id,sha256,value) VALUES(1,?,'[]')",
          EMPTY_REGISTRY_SHA256,
        );
        this.ctx.storage.sql.exec(
          "INSERT OR IGNORE INTO controlled_sequence(id,principal) VALUES(1,'0')",
        );
      }
      return initialized;
    });
  }
  private initializeMode(
    rawBinding: unknown,
    rawCursor: unknown,
    checkpointHash: unknown,
    controlled: boolean,
  ) {
    const next = {
      binding: binding(rawBinding),
      cursor: cursor(rawCursor),
      checkpoint_sha256: hash(checkpointHash),
    };
    if (next.cursor.lot_id === "0" || next.cursor.authority_epoch === "0")
      throw Error("zero authority identity");
    return this.ctx.storage.transactionSync(() => {
      const old = this.ctx.storage.sql
        .exec<{ value: string }>("SELECT value FROM lot_state WHERE id=1")
        .toArray()[0];
      if (old) {
        if (!!this.controlState() !== controlled)
          throw Error("lot mode mismatch");
        if (!equal(JSON.parse(old.value), next))
          throw Error("already initialized differently");
        return next;
      }
      this.ctx.storage.sql.exec(
        "INSERT INTO lot_state(id,value) VALUES(1,?)",
        canonical(next),
      );
      if (controlled)
        this.ctx.storage.sql.exec(
          "INSERT INTO lot_control_state(id,revision) VALUES(1,'0')",
        );
      return next;
    });
  }
  inspect() {
    const state = this.state();
    const totals = this.ctx.storage.sql
      .exec<{
        count: number;
        bytes: number;
      }>(
        "SELECT COUNT(*) AS count,COALESCE(SUM(bytes),0) AS bytes FROM journal_live",
      )
      .one();
    return {
      ...state,
      journal: totals,
      retained_receipts: this.ctx.storage.sql
        .exec<{ count: number }>("SELECT COUNT(*) AS count FROM batches")
        .one().count,
      archived_batches: this.ctx.storage.sql
        .exec<{
          count: number;
        }>("SELECT COUNT(*) AS count FROM archived_batches")
        .one().count,
      recovery_pending: !!this.pendingRecovery(),
      pending_outbox: this.ctx.storage.sql
        .exec<{ count: number }>("SELECT COUNT(*) AS count FROM outbox")
        .one().count,
    };
  }
  async commitBatch(raw: unknown) {
    this.requireLegacyMutation();
    const p = proposal(raw);
    const bytes = encode(p.payload);
    if (bytes.length > 16 * 1024 * 1024) throw Error("proposal too large");
    if ((await digest(bytes)) !== p.digest)
      throw Error("proposal digest mismatch");
    const key = p.payload.binding.lot_incarnation + ":" + p.payload.batch_id;
    const serialized = new TextDecoder().decode(bytes);
    return this.ctx.storage.transactionSync(() => {
      this.requireLegacyMutation();
      const prior = this.ctx.storage.sql
        .exec<{
          digest: string;
          receipt: string;
        }>("SELECT digest,receipt FROM batches WHERE batch_key=?", key)
        .toArray()[0];
      if (prior) {
        if (prior.digest !== p.digest) throw Error("batch identity reused");
        return JSON.parse(prior.receipt);
      }
      this.assertActive();
      const state = this.state();
      if (
        !equal(state.binding, p.payload.binding) ||
        !equal(state.cursor, p.payload.predecessor)
      )
        throw Error("stale binding or predecessor");
      const totals = this.ctx.storage.sql
        .exec<{
          count: number;
          bytes: number;
        }>(
          "SELECT COUNT(*) AS count,COALESCE(SUM(bytes),0) AS bytes FROM journal_live",
        )
        .one();
      if (totals.count >= 4096 || totals.bytes + bytes.length > JOURNAL_LIMIT)
        throw Error(
          "journal capacity requires qualified checkpoint compaction",
        );
      const receipt = {
        status: "committed",
        batch_id: p.payload.batch_id,
        digest: p.digest,
        cursor: p.payload.result,
      };
      this.ctx.storage.sql.exec(
        "INSERT INTO batches(batch_key,digest,receipt,bytes) VALUES(?,?,?,?)",
        key,
        p.digest,
        canonical(receipt),
        bytes.length,
      );
      this.ctx.storage.sql.exec(
        "INSERT INTO journal_live(batch_key,bytes) VALUES(?,?)",
        key,
        bytes.length,
      );
      for (
        let offset = 0, ordinal = 0;
        offset < serialized.length;
        offset += CHUNK, ordinal++
      )
        this.ctx.storage.sql.exec(
          "INSERT INTO journal_chunks(batch_key,ordinal,payload) VALUES(?,?,?)",
          key,
          ordinal,
          serialized.slice(offset, offset + CHUNK),
        );
      for (const [lane, data] of [
        ["effects", p.payload.effects_hex],
        ["events", p.payload.events_hex],
      ])
        if (data !== "0000000000000000")
          this.ctx.storage.sql.exec(
            "INSERT INTO outbox(batch_key,lane,digest) VALUES(?,?,?)",
            key,
            lane,
            p.digest,
          );
      this.ctx.storage.sql.exec(
        "UPDATE lot_state SET value=? WHERE id=1",
        canonical({
          binding: state.binding,
          cursor: p.payload.result,
          checkpoint_sha256: p.payload.checkpoint_sha256,
        }),
      );
      return receipt;
    });
  }
  getBatch(incarnation: unknown, id: unknown) {
    const key = decimal(incarnation) + ":" + decimal(id);
    const row = this.ctx.storage.sql
      .exec<{
        receipt: string;
      }>("SELECT receipt FROM batches WHERE batch_key=?", key)
      .toArray()[0];
    if (row) return JSON.parse(row.receipt);
    const denied = this.ctx.storage.sql
      .exec<{
        receipt: string;
      }>("SELECT receipt FROM controlled_denials WHERE batch_key=?", key)
      .toArray()[0];
    return denied ? JSON.parse(denied.receipt) : { status: "unknown" };
  }
  async readBatch(incarnation: unknown, id: unknown) {
    const key = decimal(incarnation) + ":" + decimal(id);
    const live = this.readLiveBatch(key);
    if (live !== null) return live;
    const archived = this.ctx.storage.sql
      .exec<{
        object_key: string;
        sha256: string;
        bytes: number;
        byte_offset: number;
        byte_length: number;
        digest: string;
        receipt: string;
        payload_bytes: number;
      }>(
        "SELECT a.object_key,a.sha256,a.bytes,p.byte_offset,p.byte_length,b.digest,b.receipt,b.bytes AS payload_bytes FROM archived_batches p JOIN journal_archives a ON a.object_key=p.object_key JOIN batches b ON b.batch_key=p.batch_key WHERE p.batch_key=?",
        key,
      )
      .toArray()[0];
    if (!archived) {
      if (
        this.ctx.storage.sql
          .exec("SELECT 1 FROM batches WHERE batch_key=?", key)
          .toArray().length
      )
        throw Error("journal payload missing");
      return null;
    }
    if (
      !Number.isSafeInteger(archived.byte_offset) ||
      archived.byte_offset < 0 ||
      !Number.isSafeInteger(archived.byte_length) ||
      archived.byte_length < 1 ||
      archived.byte_length !== archived.payload_bytes ||
      archived.byte_offset + archived.byte_length > archived.bytes
    )
      throw Error("journal archive bounds mismatch");
    const bytes = await this.readArchive(
      archived.object_key,
      archived.sha256,
      archived.bytes,
    );
    const payload = bytes.subarray(
      archived.byte_offset,
      archived.byte_offset + archived.byte_length,
    );
    if ((await digest(payload)) !== archived.digest)
      throw Error("journal archived batch digest mismatch");
    const result = JSON.parse(new TextDecoder().decode(payload));
    this.validateArchivedBatch(key, archived.digest, archived.receipt, result);
    return result;
  }
  private readLiveBatch(key: string) {
    const shape = this.ctx.storage.sql
      .exec<{
        count: number;
        total: number;
        largest: number;
        first: number | null;
        last: number | null;
      }>(
        "SELECT COUNT(*) AS count,COALESCE(SUM(length(CAST(payload AS BLOB))),0) AS total,COALESCE(MAX(length(CAST(payload AS BLOB))),0) AS largest,MIN(ordinal) AS first,MAX(ordinal) AS last FROM journal_chunks WHERE batch_key=?",
        key,
      )
      .one();
    if (shape.count === 0) return null;
    const stored = this.ctx.storage.sql
      .exec<{
        bytes: number;
      }>("SELECT bytes FROM batches WHERE batch_key=?", key)
      .toArray()[0];
    if (
      !stored ||
      shape.count > 64 ||
      shape.total > 16 * 1024 * 1024 ||
      shape.largest > CHUNK ||
      shape.first !== 0 ||
      shape.last !== shape.count - 1 ||
      shape.total !== stored.bytes
    )
      throw Error("journal chunk bounds or integrity mismatch");
    const rows = this.ctx.storage.sql
      .exec<{
        payload: string;
      }>(
        "SELECT payload FROM journal_chunks WHERE batch_key=? ORDER BY ordinal",
        key,
      )
      .toArray();
    return rows.length ? JSON.parse(rows.map((r) => r.payload).join("")) : null;
  }
  private validateArchivedBatch(
    key: string,
    sha: string,
    receiptJson: string,
    raw: any,
  ) {
    const p = (raw?.control_v2 ? controlledProposal : proposal)({
      kind: "proposal",
      status: "pending",
      digest: sha,
      payload: raw,
    }).payload;
    const receipt = JSON.parse(receiptJson);
    if (
      key !== p.binding.lot_incarnation + ":" + p.batch_id ||
      receipt.status !== "committed" ||
      receipt.digest !== sha ||
      receipt.batch_id !== p.batch_id ||
      !equal(receipt.cursor, p.result) ||
      (raw.control_v2 &&
        (receipt.control_revision !== raw.control_v2.revision_after ||
          receipt.actor_registry_sha256 !==
            raw.control_v2.actor_registry_after_sha256))
    )
      throw Error("journal archive receipt mismatch");
    return p;
  }
  private async readArchive(key: string, sha: string, length: number) {
    if (!Number.isSafeInteger(length) || length < 1 || length > ARCHIVE_LIMIT)
      throw Error("journal archive size bound");
    hash(sha);
    const object = await this.env.CHECKPOINTS.get(key);
    if (!object || object.size !== length)
      throw Error("journal archive missing or wrong size");
    const bytes = new Uint8Array(await object.arrayBuffer());
    if ((await digest(bytes)) !== sha)
      throw Error("journal archive digest mismatch");
    return bytes;
  }
  /** Retention tier change only: no receipt, outbox or payload is discarded. */
  private async compactJournal(manifest: any, b: Binding, c: Cursor) {
    for (;;) {
      const rows = this.ctx.storage.sql
        .exec<{
          batch_key: string;
          digest: string;
          receipt: string;
          bytes: number;
        }>(
          "SELECT b.batch_key,b.digest,b.receipt,b.bytes FROM journal_live l JOIN batches b ON b.batch_key=l.batch_key WHERE b.batch_key>=? AND b.batch_key<? AND json_extract(b.receipt,'$.cursor.authority_epoch')=? AND (length(json_extract(b.receipt,'$.cursor.completed_tick'))<length(?) OR (length(json_extract(b.receipt,'$.cursor.completed_tick'))=length(?) AND json_extract(b.receipt,'$.cursor.completed_tick')<=?)) ORDER BY b.rowid LIMIT ?",
          b.lot_incarnation + ":",
          b.lot_incarnation + ";",
          c.authority_epoch,
          c.completed_tick,
          c.completed_tick,
          c.completed_tick,
          ARCHIVE_BATCHES,
        )
        .toArray();
      if (!rows.length) return;
      // Capture live bytes synchronously before any external I/O. A competing
      // compactor may later win, but it cannot change these immutable receipts.
      const entries: {
        batch_key: string;
        digest: string;
        receipt: string;
        bytes: Uint8Array;
        offset: number;
      }[] = [];
      let length = 0;
      for (const row of rows) {
        if (entries.length && length + row.bytes > ARCHIVE_LIMIT) break;
        if (
          !Number.isSafeInteger(row.bytes) ||
          row.bytes < 1 ||
          row.bytes > ARCHIVE_LIMIT
        )
          throw Error("journal archive batch size bound");
        const raw = this.readLiveBatch(row.batch_key);
        if (!raw) throw Error("journal compaction payload missing");
        const p = this.validateArchivedBatch(
          row.batch_key,
          row.digest,
          row.receipt,
          raw,
        );
        if (!equal(p.binding, b) || p.result.lot_id !== c.lot_id)
          throw Error("journal compaction checkpoint binding mismatch");
        const bytes = encode(raw);
        if (bytes.length !== row.bytes)
          throw Error("journal compaction size mismatch");
        entries.push({ ...row, bytes, offset: length });
        length += bytes.length;
      }
      const bytes = new Uint8Array(length);
      for (const entry of entries) {
        if ((await digest(entry.bytes)) !== entry.digest)
          throw Error("journal compaction digest mismatch");
        bytes.set(entry.bytes, entry.offset);
      }
      const sha = await digest(bytes),
        key = `lots/${c.lot_id}/journal/${sha}`;
      const existing = await this.env.CHECKPOINTS.get(key);
      if (existing) {
        if (
          existing.size !== bytes.length ||
          (await digest(new Uint8Array(await existing.arrayBuffer()))) !== sha
        )
          throw Error("immutable journal archive corrupted");
      } else await this.env.CHECKPOINTS.put(key, bytes);
      await this.readArchive(key, sha, bytes.length);
      this.ctx.storage.transactionSync(() => {
        const pinned = this.ctx.storage.sql
          .exec<{
            manifest: string;
          }>(
            "SELECT manifest FROM checkpoints WHERE object_key=?",
            manifest.object_key,
          )
          .toArray()[0];
        if (!pinned || !equal(JSON.parse(pinned.manifest), manifest))
          throw Error("journal compaction checkpoint changed");
        // SQLite publishes references and removes the hot copy atomically,
        // only after the complete immutable archive passed readback verification.
        this.ctx.storage.sql.exec(
          "INSERT OR IGNORE INTO journal_archives(object_key,sha256,bytes,checkpoint_key) VALUES(?,?,?,?)",
          key,
          sha,
          bytes.length,
          manifest.object_key,
        );
        for (const entry of entries) {
          const current = this.ctx.storage.sql
            .exec<{
              digest: string;
              receipt: string;
              bytes: number;
            }>(
              "SELECT b.digest,b.receipt,b.bytes FROM journal_live l JOIN batches b ON b.batch_key=l.batch_key WHERE l.batch_key=?",
              entry.batch_key,
            )
            .toArray()[0];
          if (!current) continue; // Another verified compactor already won.
          if (
            current.digest !== entry.digest ||
            current.receipt !== entry.receipt ||
            current.bytes !== entry.bytes.length
          )
            throw Error("journal compaction batch changed");
          this.ctx.storage.sql.exec(
            "INSERT INTO archived_batches(batch_key,object_key,byte_offset,byte_length) VALUES(?,?,?,?)",
            entry.batch_key,
            key,
            entry.offset,
            entry.bytes.length,
          );
          this.ctx.storage.sql.exec(
            "DELETE FROM journal_chunks WHERE batch_key=?",
            entry.batch_key,
          );
          this.ctx.storage.sql.exec(
            "DELETE FROM journal_live WHERE batch_key=?",
            entry.batch_key,
          );
        }
      });
    }
  }
  async publishCheckpoint(
    rawBinding: unknown,
    rawCursor: unknown,
    expected: unknown,
    bytes: Uint8Array,
  ) {
    return this.publishCheckpointMode(
      rawBinding,
      rawCursor,
      expected,
      bytes,
      false,
    );
  }
  private async publishCheckpointMode(
    rawBinding: unknown,
    rawCursor: unknown,
    expected: unknown,
    bytes: Uint8Array,
    controlled: boolean,
    expectedControl?: ReturnType<LotCoordinator["controlledRecoveryState"]>,
  ) {
    this.requireMutationMode(controlled);
    const b = binding(rawBinding),
      c = cursor(rawCursor),
      sha = hash(expected);
    if (
      !(bytes instanceof Uint8Array) ||
      bytes.byteLength > 8 * 1024 * 1024 ||
      (await digest(bytes)) !== sha
    )
      throw Error("checkpoint bytes/digest mismatch");
    // Initialization can race the asynchronous digest on a previously empty DO.
    this.requireMutationMode(controlled);
    this.assertActive();
    const state = this.state();
    if (
      !equal(state.binding, b) ||
      !equal(state.cursor, c) ||
      state.checkpoint_sha256 !== sha
    )
      throw Error("checkpoint is not current committed state");
    const key = `lots/${c.lot_id}/${c.authority_epoch}/${c.completed_tick}/${sha}`;
    const existing = await this.env.CHECKPOINTS.get(key);
    if (existing) {
      if (existing.size !== bytes.byteLength)
        throw Error("immutable checkpoint object size mismatch");
      if ((await digest(new Uint8Array(await existing.arrayBuffer()))) !== sha)
        throw Error("immutable checkpoint object corrupted");
    } else await this.env.CHECKPOINTS.put(key, bytes);
    const confirmed = await this.env.CHECKPOINTS.get(key);
    if (
      !confirmed ||
      confirmed.size !== bytes.byteLength ||
      (await digest(new Uint8Array(await confirmed.arrayBuffer()))) !== sha
    )
      throw Error("checkpoint readback failed");
    const manifest = {
      ...(controlled ? { control_v2: expectedControl } : {}),
      schema_version: 1,
      binding: b,
      cursor: c,
      object_key: key,
      sha256: sha,
      byte_length: bytes.byteLength,
    };
    const published = this.ctx.storage.transactionSync(() => {
      this.requireMutationMode(controlled);
      this.assertActive();
      if (
        controlled &&
        (!expectedControl ||
          !equal(this.controlledRecoveryState(), expectedControl))
      )
        throw Error("controlled checkpoint revision or registry changed");
      const now = this.state();
      if (!equal(now.binding, b) || !equal(now.cursor, c))
        throw Error("checkpoint superseded before publication");
      this.ctx.storage.sql.exec(
        "INSERT OR IGNORE INTO checkpoints(object_key,manifest) VALUES(?,?)",
        key,
        canonical(manifest),
      );
      // The checkpoint object key is immutable. A lease-only revision can
      // change without changing VM bytes; retries must return the stored
      // manifest rather than claim that a new wrapper was published.
      return JSON.parse(
        this.ctx.storage.sql
          .exec<{
            manifest: string;
          }>("SELECT manifest FROM checkpoints WHERE object_key=?", key)
          .one().manifest,
      );
    });
    // Publication is already durable. Archive/readback failure leaves every
    // unarchived SQL payload intact, and retry resumes from the remaining set.
    await this.compactJournal(published, b, c);
    return published;
  }
  listCheckpoints() {
    return this.ctx.storage.sql
      .exec<{ manifest: string }>(
        "SELECT manifest FROM checkpoints ORDER BY rowid DESC LIMIT 2",
      )
      .toArray()
      .map((r) => JSON.parse(r.manifest));
  }
}
