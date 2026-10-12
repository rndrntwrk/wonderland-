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
      `CREATE TABLE IF NOT EXISTS lot_state(id INTEGER PRIMARY KEY CHECK(id=1),value TEXT NOT NULL);CREATE TABLE IF NOT EXISTS batches(batch_key TEXT PRIMARY KEY,digest TEXT NOT NULL,receipt TEXT NOT NULL,bytes INTEGER NOT NULL);CREATE TABLE IF NOT EXISTS journal_chunks(batch_key TEXT NOT NULL,ordinal INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(batch_key,ordinal));CREATE TABLE IF NOT EXISTS outbox(batch_key TEXT NOT NULL,lane TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(batch_key,lane));CREATE TABLE IF NOT EXISTS checkpoints(object_key TEXT PRIMARY KEY,manifest TEXT NOT NULL);`,
    );
  }
  private state(): State {
    const row = this.ctx.storage.sql
      .exec<{ value: string }>("SELECT value FROM lot_state WHERE id=1")
      .toArray()[0];
    if (!row) throw Error("lot not initialized");
    return JSON.parse(row.value);
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
