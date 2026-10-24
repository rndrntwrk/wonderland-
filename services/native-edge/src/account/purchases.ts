/** Private single-object financial prerequisite. No public purchase API, native
 * completion encoder, geometry acknowledgement, outbox retirement or refund.
 * Source: DurableBuildRequest/ServerBuildConfirmation; VMNetBuyObjectCmd pays
 * the system sink before forwarding verified geometry. Durable success remains
 * pending geometry/reconciliation, never a fabricated placed object. */
import {
  canonical,
  decimal,
  digest,
  encode,
  equal,
  fields,
  hash,
} from "../lot/contracts";
import { controlledProposal } from "../lot/owned";
const U32 = 4294967295n,
  MAX_AVATAR = 16777215n;
const nonzero = (v: unknown) => {
  const s = decimal(v);
  if (s === "0") throw Error("zero purchase identity");
  return s;
};
export function purchaseReference(raw: unknown) {
  const v = fields(raw, ["lot_id", "batch_key", "digest", "tick", "ordinal"]);
  if (
    typeof v.batch_key !== "string" ||
    !/^\d{1,20}:\d{1,20}$/.test(v.batch_key)
  )
    throw Error("purchase batch key");
  const parts = v.batch_key.split(":");
  parts.forEach(nonzero);
  if (
    !Number.isInteger(v.ordinal) ||
    Number(v.ordinal) < 0 ||
    Number(v.ordinal) > 0xffffffff
  )
    throw Error("purchase ordinal");
  return {
    lot_id: nonzero(v.lot_id),
    batch_key: parts.join(":"),
    digest: hash(v.digest),
    tick: nonzero(v.tick),
    ordinal: Number(v.ordinal),
  };
}
/** Fixed bincode tag 0 only. Unknown/mixed lanes fail closed and stay pending. */
export function decodePurchaseEvents(hex: unknown) {
  if (
    typeof hex !== "string" ||
    hex.length > 512 * 1024 ||
    !/^(?:[0-9a-f]{2})+$/.test(hex)
  )
    throw Error("purchase event bytes");
  const bytes = Uint8Array.from(hex.match(/../g)!, (h) => parseInt(h, 16)),
    v = new DataView(bytes.buffer);
  let at = 0;
  const take = (n: number) => {
    if (at + n > bytes.length) throw Error("truncated purchase event");
    const p = at;
    at += n;
    return p;
  };
  const u64 = () => v.getBigUint64(take(8), true).toString(),
    u32 = () => v.getUint32(take(4), true);
  const entity = () => {
    const e = { object_id: v.getInt16(take(2), true), generation: u32() };
    if (e.object_id <= 0 || !e.generation) throw Error("purchase entity");
    return e;
  };
  const count = BigInt(u64());
  if (count < 1n || count > 128n) throw Error("purchase event count");
  const result = [];
  let previousTick = 0n,
    previousOrdinal = -1;
  for (let n = 0; n < Number(count); n++) {
    const tick = nonzero(u64()),
      ordinal = u32();
    if (u32() !== 0) throw Error("unsupported purchase event lane");
    if (
      BigInt(tick) < previousTick ||
      (BigInt(tick) === previousTick && ordinal <= previousOrdinal)
    )
      throw Error("purchase event order");
    previousTick = BigInt(tick);
    previousOrdinal = ordinal;
    const operation = nonzero(u64()),
      actor = entity(),
      owner = String(u32());
    if (BigInt(owner) < 1n || BigInt(owner) > MAX_AVATAR)
      throw Error("purchase owner");
    const pos = take(32),
      preview_hash = Array.from(bytes.slice(pos, pos + 32), (b) =>
        b.toString(16).padStart(2, "0"),
      ).join("");
    const cost = v.getBigInt64(take(8), true);
    if (cost < 0n) throw Error("negative purchase charge");
    const permissions_revision = u64(),
      account_revision = u64(),
      catalog_revision = u64();
    if (u64() !== "1") throw Error("single-object purchase required");
    const purchase = {
      entity: entity(),
      catalog_id: u32(),
      owner: String(u32()),
    };
    if (
      !purchase.catalog_id ||
      purchase.owner !== owner ||
      equal(purchase.entity, actor)
    )
      throw Error("purchase object identity");
    result.push({
      tick,
      ordinal,
      request: {
        operation,
        actor,
        owner,
        preview_hash,
        cost: String(cost),
        permissions_revision,
        account_revision,
        catalog_revision,
        purchases: [purchase],
      },
    });
  }
  if (at !== bytes.length) throw Error("trailing purchase bytes");
  return result;
}
export interface PurchaseJournalEnv {
  COORD?: {
    idFromName(name: string): DurableObjectId;
    get(id: DurableObjectId): {
      getBatch(incarnation: string, id: string): Promise<unknown>;
      readBatch(incarnation: string, id: string): Promise<unknown>;
    };
  };
}
export function configuredPurchaseReader(env: PurchaseJournalEnv) {
  return async (raw: unknown) => {
    const reference = purchaseReference(raw);
    if (!env.COORD) throw Error("trusted purchase journal unavailable");
    const stub = env.COORD.get(env.COORD.idFromName(reference.lot_id)),
      [incarnation, id] = reference.batch_key.split(":");
    const decision: any = await stub.getBatch(incarnation, id);
    if (
      decision?.status !== "committed" ||
      decision.digest !== reference.digest ||
      decision.batch_id !== id
    )
      throw Error("purchase origin not committed");
    const p = controlledProposal({
      kind: "proposal",
      status: "pending",
      digest: reference.digest,
      payload: await stub.readBatch(incarnation, id),
    }).payload;
    if (
      (await digest(encode(p))) !== reference.digest ||
      p.binding.lot_incarnation !== incarnation ||
      p.result.lot_id !== reference.lot_id ||
      !equal(decision.cursor, p.result)
    )
      throw Error("purchase journal mismatch");
    const all = decodePurchaseEvents(p.events_hex),
      member = all.find(
        (e) => e.tick === reference.tick && e.ordinal === reference.ordinal,
      );
    if (
      !member ||
      all.some(
        (e) =>
          BigInt(e.tick) <= BigInt(p.predecessor.completed_tick) ||
          BigInt(e.tick) > BigInt(p.result.completed_tick),
      )
    )
      throw Error("purchase event outside origin");
    const actor = p.control_v2.actors.find(
      (a) =>
        equal(a.actor, member.request.actor) &&
        a.avatar_id === member.request.owner,
    );
    if (
      !actor ||
      !equal(p.control_v2.permission_before, p.control_v2.permission_after) ||
      member.request.permissions_revision !==
        p.control_v2.permission_after.revision
    )
      throw Error("purchase source ownership/permission mismatch");
    if (
      (await digest(encode(p.control_v2.actors))) !==
      p.control_v2.actor_registry_after_sha256
    )
      throw Error("purchase registry digest");
    return {
      reference,
      binding: p.binding,
      actor,
      request: member.request,
      permission: p.control_v2.permission_after,
    };
  };
}
export class AccountPurchaseStore {
  constructor(
    private storage: DurableObjectStorage,
    private read: ReturnType<typeof configuredPurchaseReader>,
    private now: () => number,
  ) {
    storage.sql
      .exec(`CREATE TABLE IF NOT EXISTS native_purchase_receipts(purchase_key TEXT PRIMARY KEY,request_sha256 TEXT NOT NULL,receipt_id TEXT NOT NULL UNIQUE,lot_id TEXT NOT NULL,origin_batch_key TEXT NOT NULL,origin_digest TEXT NOT NULL,event_tick TEXT NOT NULL,event_ordinal INTEGER NOT NULL,receipt_json TEXT NOT NULL,UNIQUE(lot_id,origin_batch_key,origin_digest,event_tick,event_ordinal));
CREATE TABLE IF NOT EXISTS native_owned_objects(persistent_id TEXT PRIMARY KEY,purchase_key TEXT NOT NULL UNIQUE,account_id TEXT NOT NULL,avatar_id TEXT NOT NULL,lot_id TEXT NOT NULL,content_release TEXT NOT NULL,catalog_id INTEGER NOT NULL,entity_object_id INTEGER NOT NULL,entity_generation INTEGER NOT NULL,state TEXT NOT NULL,charged_cost TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS native_objects_owner ON native_owned_objects(avatar_id,length(persistent_id),persistent_id);`);
  }
  /** Explicit empty-cohort allocator intake, never a read/CAS default or legacy migration. */
  initializeEmpty(raw: unknown) {
    const v = fields(raw, [
      "schema_version",
      "allocator_scope",
      "source_high_water",
      "source_receipt_sha256",
      "existing_owned_objects",
    ]);
    const config = {
      schema_version: 1,
      allocator_scope: hash(v.allocator_scope),
      source_high_water: decimal(v.source_high_water),
      source_receipt_sha256: hash(v.source_receipt_sha256),
      existing_owned_objects: 0,
    };
    if (
      v.schema_version !== 1 ||
      v.existing_owned_objects !== 0 ||
      BigInt(config.source_high_water) < MAX_AVATAR ||
      BigInt(config.source_high_water) >= U32
    )
      throw Error("explicit empty object allocator intake required");
    return this.storage.transactionSync(() => {
      const prior = this.storage.sql
        .exec<{
          value: string;
        }>("SELECT value FROM account_meta WHERE key='purchase_allocator_v1'")
        .toArray()[0];
      if (prior) {
        if (prior.value !== canonical(config))
          throw Error("purchase allocator already pinned");
        return config;
      }
      if (
        this.storage.sql
          .exec<{ n: number }>("SELECT COUNT(*) AS n FROM native_owned_objects")
          .one().n ||
        this.storage.sql
          .exec<{
            n: number;
          }>("SELECT COUNT(*) AS n FROM native_purchase_receipts")
          .one().n
      )
        throw Error("purchase inventory migration required");
      this.storage.sql.exec(
        "INSERT INTO account_meta(key,value) VALUES('purchase_allocator_v1',?)",
        canonical(config),
      );
      this.storage.sql.exec(
        "INSERT INTO account_meta(key,value) VALUES('purchase_object_sequence',?)",
        config.source_high_water,
      );
      return config;
    });
  }
  private next(key: string, max = 18446744073709551615n) {
    const old = this.storage.sql
      .exec<{
        value: string;
      }>("SELECT value FROM account_meta WHERE key=?", key)
      .toArray()[0];
    const n = BigInt(old ? decimal(old.value) : "0") + 1n;
    if (n > max) throw Error("purchase identity exhausted");
    this.storage.sql.exec(
      "INSERT INTO account_meta(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
      key,
      String(n),
    );
    return String(n);
  }
  async apply(raw: unknown) {
    const proof = await this.read(raw),
      { actor, request, binding } = proof;
    const key = [
      proof.reference.lot_id,
      actor.avatar_id,
      actor.control_generation,
      request.operation,
    ].join(":");
    const fingerprint = await digest(
      encode({
        lot_id: proof.reference.lot_id,
        binding: {
          protocol: binding.protocol,
          source_pin: binding.source_pin,
          binding_sha256: binding.binding_sha256,
        },
        actor,
        request,
        permission: proof.permission,
      }),
    );
    return this.storage.transactionSync(() => {
      const prior = this.storage.sql
        .exec<{
          request_sha256: string;
          receipt_json: string;
        }>(
          "SELECT request_sha256,receipt_json FROM native_purchase_receipts WHERE purchase_key=?",
          key,
        )
        .toArray()[0];
      if (prior) {
        if (prior.request_sha256 !== fingerprint)
          throw Error("purchase operation identity reused");
        return JSON.parse(prior.receipt_json);
      }
      if (
        !this.storage.sql
          .exec(
            "SELECT value FROM account_meta WHERE key='purchase_allocator_v1'",
          )
          .toArray().length
      )
        throw Error("owned object allocator intake required");
      const account = this.storage.sql
        .exec<{
          revision: string;
        }>("SELECT revision FROM accounts WHERE id=?", actor.account_id)
        .toArray()[0];
      const owned = this.storage.sql
        .exec(
          "SELECT id FROM avatars WHERE id=? AND account_id=?",
          actor.avatar_id,
          actor.account_id,
        )
        .toArray()[0];
      const initialized = this.storage.sql
        .exec<{
          version: number;
        }>(
          "SELECT version FROM avatar_ownership_initializations WHERE avatar_id=?",
          actor.avatar_id,
        )
        .toArray()[0];
      const c = this.storage.sql
        .exec<any>(
          "SELECT * FROM avatar_control_claims WHERE avatar_id=?",
          actor.avatar_id,
        )
        .toArray()[0];
      const f = this.storage.sql
        .exec<{
          export_json: string;
          source_json: string;
        }>(
          "SELECT export_json,source_json FROM avatar_control_exports WHERE avatar_id=?",
          actor.avatar_id,
        )
        .toArray()[0];
      const lease = this.storage.sql
        .exec<{
          expires_at: number;
        }>(
          "SELECT expires_at FROM avatar_control_leases WHERE avatar_id=?",
          actor.avatar_id,
        )
        .toArray()[0];
      const session = this.storage.sql
        .exec<any>(
          "SELECT * FROM sessions WHERE token_hash=?",
          actor.session_ref,
        )
        .toArray()[0];
      if (
        !account ||
        !owned ||
        initialized?.version !== 1 ||
        !c ||
        !f ||
        !lease ||
        !session
      )
        throw Error("purchase source/control migration required");
      const e = JSON.parse(f.export_json),
        claim = JSON.parse(c.claim),
        now = this.now();
      if (
        c.account_id !== actor.account_id ||
        c.session_hash !== actor.session_ref ||
        c.operation !== actor.admission_operation ||
        c.digest !== actor.reservation_digest ||
        claim.state !== "reserved" ||
        claim.generation !== actor.control_generation ||
        claim.lot_id !== proof.reference.lot_id ||
        claim.expires_at <= now ||
        lease.expires_at <= now ||
        session.account_id !== actor.account_id ||
        session.revision !== account.revision ||
        session.expires <= now ||
        session.idle <= now ||
        e.export_digest !== actor.export_digest ||
        e.avatar_id !== actor.avatar_id ||
        e.content_release !== binding.binding_sha256 ||
        e.account_revision !== request.account_revision ||
        account.revision !== request.account_revision ||
        !equal(e.permission_binding, proof.permission)
      )
        throw Error("stale purchase owner/control");
      const b = this.storage.sql
        .exec<{
          balance: string;
        }>(
          "SELECT balance FROM avatar_balances WHERE avatar_id=?",
          actor.avatar_id,
        )
        .toArray()[0];
      if (!b || BigInt(decimal(b.balance)) > U32)
        throw Error("purchase budget migration required");
      const before = BigInt(b.balance),
        cost = BigInt(request.cost),
        rejected = before < cost;
      if (!rejected && JSON.parse(f.source_json).balance !== b.balance)
        throw Error("stale frozen purchase budget");
      const id = this.next("purchase_receipt_sequence"),
        after = rejected ? before : before - cost;
      let objects: { entity: typeof request.actor; persistent_id: string }[] =
        [];
      if (!rejected) {
        const objectId = this.next("purchase_object_sequence", U32 - 1n);
        if (BigInt(objectId) <= MAX_AVATAR)
          throw Error("purchase object namespace");
        this.storage.sql.exec(
          "UPDATE avatar_balances SET balance=? WHERE avatar_id=?",
          String(after),
          actor.avatar_id,
        );
        const purchase = request.purchases[0];
        this.storage.sql.exec(
          "INSERT INTO native_owned_objects(persistent_id,purchase_key,account_id,avatar_id,lot_id,content_release,catalog_id,entity_object_id,entity_generation,state,charged_cost) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
          objectId,
          key,
          actor.account_id,
          actor.avatar_id,
          proof.reference.lot_id,
          binding.binding_sha256,
          purchase.catalog_id,
          purchase.entity.object_id,
          purchase.entity.generation,
          "pending_geometry",
          request.cost,
        );
        objects = [{ entity: purchase.entity, persistent_id: objectId }];
      }
      const receipt = {
        schema_version: 1,
        status: rejected ? "rejected" : "account_committed_pending_geometry",
        purchase_key: key,
        request_sha256: fingerprint,
        origin: proof.reference,
        account_id: actor.account_id,
        avatar_id: actor.avatar_id,
        balance_before: String(before),
        balance_after: String(after),
        system_sink: "4294967295",
        confirmation: {
          operation: request.operation,
          actor: request.actor,
          owner: request.owner,
          preview_hash: request.preview_hash,
          charged_cost: rejected ? "0" : request.cost,
          durable_receipt: id,
          status: rejected
            ? { Rejected: { reason: "InsufficientFunds" } }
            : "Committed",
          created_objects: objects,
        },
      };
      const retained = canonical(receipt);
      this.storage.sql.exec(
        "INSERT INTO native_purchase_receipts(purchase_key,request_sha256,receipt_id,lot_id,origin_batch_key,origin_digest,event_tick,event_ordinal,receipt_json) VALUES(?,?,?,?,?,?,?,?,?)",
        key,
        fingerprint,
        id,
        proof.reference.lot_id,
        proof.reference.batch_key,
        proof.reference.digest,
        proof.reference.tick,
        proof.reference.ordinal,
        retained,
      );
      return JSON.parse(retained);
    });
  }
}
