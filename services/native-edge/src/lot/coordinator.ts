import { DurableObject } from "cloudflare:workers";
import {
  binding,
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
import { successor, takeoverMarker, type RecoveryGrant } from "./recovery";
import { controlFence } from "./control";
interface Env {
  CHECKPOINTS: R2Bucket;
}
interface State {
  binding: Binding;
  cursor: Cursor;
  checkpoint_sha256: string;
}
const CHUNK = 256 * 1024;
const JOURNAL_LIMIT = 64 * 1024 * 1024;
export class LotCoordinator extends DurableObject<Env> {
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    ctx.storage.sql.exec(
      `CREATE TABLE IF NOT EXISTS control_fences(authority TEXT NOT NULL, avatar_id TEXT NOT NULL, generation TEXT NOT NULL, receipt TEXT NOT NULL, PRIMARY KEY(authority,avatar_id,generation));CREATE TABLE IF NOT EXISTS lot_state(id INTEGER PRIMARY KEY CHECK(id=1),value TEXT NOT NULL);CREATE TABLE IF NOT EXISTS batches(batch_key TEXT PRIMARY KEY,digest TEXT NOT NULL,receipt TEXT NOT NULL,bytes INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS journal_chunks(batch_key TEXT NOT NULL,ordinal INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(batch_key,ordinal));CREATE TABLE IF NOT EXISTS outbox(batch_key TEXT NOT NULL,lane TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(batch_key,lane));CREATE TABLE IF NOT EXISTS checkpoints(object_key TEXT PRIMARY KEY,manifest TEXT NOT NULL);CREATE TABLE IF NOT EXISTS recoveries(old_incarnation TEXT PRIMARY KEY,status TEXT NOT NULL,grant_json TEXT NOT NULL,marker_json TEXT);`,
    );
  }
  /** Fence receipt acknowledges durable refusal only, never avatar detach or custody release. */
  recordControlFence(raw: unknown) {
    const fence = controlFence(raw);
    return this.ctx.storage.transactionSync(() => {
      const state = this.state();
      if (state.cursor.lot_id !== fence.lot_id)
        throw Error("control fence lot mismatch");
      const existing = this.ctx.storage.sql
        .exec<{
          receipt: string;
        }>("SELECT receipt FROM control_fences WHERE authority=? AND avatar_id=? AND generation=?", fence.authority, fence.avatar_id, fence.generation)
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
      return receipt;
    });
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
  beginRecovery(expectedIncarnation: unknown): RecoveryGrant {
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
      if (
        this.ctx.storage.sql
          .exec<{ count: number }>("SELECT COUNT(*) AS count FROM recoveries")
          .one().count >= 4096
      )
        throw Error("recovery history capacity");
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
      }>("SELECT batch_key,digest,receipt FROM batches ORDER BY rowid")
      .toArray();
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
        p = this.readBatch(owner, id);
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
        p = proposal({
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
      return row;
    };
    if (readRecord().status === "committed") return marker;
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
    return this.ctx.storage.transactionSync(() => {
      if (readRecord().status === "committed") return marker;
      const now = this.state();
      if (
        !equal(now.binding, grant.old_binding) ||
        !equal(now.cursor, grant.old_cursor)
      )
        throw Error("recovery predecessor changed");
      const manifest = {
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
      this.ctx.storage.sql.exec(
        "UPDATE recoveries SET status='committed',marker_json=? WHERE old_incarnation=?",
        canonical(marker),
        grant.old_binding.lot_incarnation,
      );
      return marker;
    });
  }
  initialize(rawBinding: unknown, rawCursor: unknown, checkpointHash: unknown) {
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
        if (!equal(JSON.parse(old.value), next))
          throw Error("already initialized differently");
        return next;
      }
      this.ctx.storage.sql.exec(
        "INSERT INTO lot_state(id,value) VALUES(1,?)",
        canonical(next),
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
        "SELECT COUNT(*) AS count,COALESCE(SUM(bytes),0) AS bytes FROM batches",
      )
      .one();
    return {
      ...state,
      journal: totals,
      recovery_pending: !!this.pendingRecovery(),
      pending_outbox: this.ctx.storage.sql
        .exec<{ count: number }>("SELECT COUNT(*) AS count FROM outbox")
        .one().count,
    };
  }
  async commitBatch(raw: unknown) {
    const p = proposal(raw);
    const bytes = encode(p.payload);
    if (bytes.length > 16 * 1024 * 1024) throw Error("proposal too large");
    if ((await digest(bytes)) !== p.digest)
      throw Error("proposal digest mismatch");
    const key = p.payload.binding.lot_incarnation + ":" + p.payload.batch_id;
    const serialized = new TextDecoder().decode(bytes);
    return this.ctx.storage.transactionSync(() => {
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
          "SELECT COUNT(*) AS count,COALESCE(SUM(bytes),0) AS bytes FROM batches",
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
    return row ? JSON.parse(row.receipt) : { status: "unknown" };
  }
  readBatch(incarnation: unknown, id: unknown) {
    const key = decimal(incarnation) + ":" + decimal(id);
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
  async publishCheckpoint(
    rawBinding: unknown,
    rawCursor: unknown,
    expected: unknown,
    bytes: Uint8Array,
  ) {
    const b = binding(rawBinding),
      c = cursor(rawCursor),
      sha = hash(expected);
    if (
      !(bytes instanceof Uint8Array) ||
      bytes.byteLength > 8 * 1024 * 1024 ||
      (await digest(bytes)) !== sha
    )
      throw Error("checkpoint bytes/digest mismatch");
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
      schema_version: 1,
      binding: b,
      cursor: c,
      object_key: key,
      sha256: sha,
      byte_length: bytes.byteLength,
    };
    return this.ctx.storage.transactionSync(() => {
      this.assertActive();
      const now = this.state();
      if (!equal(now.binding, b) || !equal(now.cursor, c))
        throw Error("checkpoint superseded before publication");
      this.ctx.storage.sql.exec(
        "INSERT OR IGNORE INTO checkpoints(object_key,manifest) VALUES(?,?)",
        key,
        canonical(manifest),
      );
      return manifest;
    });
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
