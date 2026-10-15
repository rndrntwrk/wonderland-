import { AccountAuthority } from "../src/account/authority";
import { password_hash } from "../generated/account_crypto.js";
export class TestAccounts extends AccountAuthority {
  changeSource(id: string, key: string, value: unknown) {
    const row = this.ctx.storage.sql
      .exec<{
        source_json: string;
      }>("SELECT source_json FROM avatar_source_state WHERE avatar_id=?", id)
      .one();
    const source = JSON.parse(row.source_json);
    source[key] = value;
    this.ctx.storage.sql.exec(
      "UPDATE avatar_source_state SET source_json=? WHERE avatar_id=?",
      JSON.stringify(source),
      id,
    );
    return true;
  }
  testOutfitPrice(id: string, value: string) {
    this.ctx.storage.sql.exec(
      "UPDATE avatar_outfits SET sale_price=? WHERE avatar_id=?",
      value,
      id,
    );
    return true;
  }
  removeSource(id: string) {
    this.ctx.storage.sql.exec(
      "DELETE FROM avatar_source_state WHERE avatar_id=?",
      id,
    );
    return true;
  }
  sparseCatalog(release: string, mode: string) {
    return this.initializeCatalog({
      release,
      entries:
        mode === "entries"
          ? Array(2)
          : [
              { key: "1", part: "head", gender: "female", skins: Array(1) },
              { key: "2", part: "body", gender: "female", skins: ["dark"] },
            ],
    });
  }
  knownVector() {
    const bytes = new TextEncoder().encode("fixed local verification phrase");
    const hash = password_hash(
      bytes,
      Uint8Array.from({ length: 16 }, (_, i) => i),
    );
    return {
      hash: [...hash].map((v) => v.toString(16).padStart(2, "0")).join(""),
      cleared: bytes.every((v) => v === 0),
    };
  }
  controlSweepReadCost(after: number) {
    const query =
      "SELECT avatar_id,operation,digest,claim FROM avatar_control_claims WHERE CAST(avatar_id AS INTEGER)>? ORDER BY CAST(avatar_id AS INTEGER) LIMIT 51";
    const plan = this.ctx.storage.sql
      .exec("EXPLAIN QUERY PLAN " + query, after)
      .toArray();
    const cursor = this.ctx.storage.sql.exec(query, after);
    const rows = cursor.toArray();
    return { plan, returned: rows.length, rowsRead: cursor.rowsRead };
  }
  protected now() {
    return this.ctx.storage.sql
      .exec<{
        value: string;
      }>("SELECT value FROM account_meta WHERE key='test_clock'")
      .toArray()[0]?.value
      ? Number(
          this.ctx.storage.sql
            .exec<{
              value: string;
            }>("SELECT value FROM account_meta WHERE key='test_clock'")
            .one().value,
        )
      : Date.now();
  }
  advance(ms: number) {
    const now = this.now() + ms;
    this.ctx.storage.sql.exec(
      "INSERT INTO account_meta(key,value) VALUES('test_clock',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
      String(now),
    );
    return now;
  }
  seedAvatarSequence(value: string) {
    if (!/^[0-9]+$/.test(value)) throw Error("invalid test sequence");
    this.ctx.storage.sql.exec(
      "INSERT INTO account_meta(key,value) VALUES('avatar_sequence',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
      value,
    );
    return { seeded: true };
  }
  failReceipt(enabled: boolean) {
    this.ctx.storage.sql.exec("DROP TRIGGER IF EXISTS test_fail_receipt");
    if (enabled)
      this.ctx.storage.sql.exec(
        "CREATE TRIGGER test_fail_receipt BEFORE INSERT ON avatar_operations BEGIN SELECT RAISE(ABORT, 'test receipt failure'); END",
      );
    return { enabled };
  }
  testBalance(id: string, balance: string) {
    this.ctx.storage.sql.exec(
      "UPDATE avatar_balances SET balance=? WHERE avatar_id=?",
      balance,
      id,
    );
    return { updated: true };
  }
  legacyOwnership(id: string) {
    this.ctx.storage.transactionSync(() => {
      this.ctx.storage.sql.exec(
        "DELETE FROM avatar_outfits WHERE avatar_id=?",
        id,
      );
      this.ctx.storage.sql.exec(
        "DELETE FROM avatar_balances WHERE avatar_id=?",
        id,
      );
      this.ctx.storage.sql.exec(
        "DELETE FROM avatar_ownership_initializations WHERE avatar_id=?",
        id,
      );
    });
    return { cleared: true };
  }
  snapshot() {
    return {
      sequences: this.ctx.storage.sql
        .exec(
          "SELECT * FROM account_meta WHERE key IN ('avatar_sequence','outfit_sequence')",
        )
        .toArray(),
      outfits: this.ctx.storage.sql
        .exec("SELECT * FROM avatar_outfits ORDER BY length(id),id")
        .toArray(),
      balances: this.ctx.storage.sql
        .exec(
          "SELECT * FROM avatar_balances ORDER BY length(avatar_id),avatar_id",
        )
        .toArray(),
      initializations: this.ctx.storage.sql
        .exec(
          "SELECT * FROM avatar_ownership_initializations ORDER BY length(avatar_id),avatar_id",
        )
        .toArray(),
      operations: this.ctx.storage.sql
        .exec("SELECT * FROM avatar_operations")
        .toArray(),
      accounts: this.ctx.storage.sql.exec("SELECT * FROM accounts").toArray(),
      sessions: this.ctx.storage.sql.exec("SELECT * FROM sessions").toArray(),
      avatars: this.ctx.storage.sql.exec("SELECT * FROM avatars").toArray(),
    };
  }
}
const allowed = new Set([
  "initializeCatalog",
  "initializeAdmissionAuthorityV2",
  "exportReservedAvatarV2",
  "getReservedAvatarExportV2",
  "revalidateReservedAvatarV2",
  "renewAvatarControlV2",
  "revalidateControlLeaseV2",
  "removeSource",
  "changeSource",
  "testOutfitPrice",
  "register",
  "login",
  "rotate",
  "logout",
  "createAvatar",
  "roster",
  "ownedAvatar",
  "ownedAssets",
  "reserveAvatarControl",
  "avatarControl",
  "pendingControlFences",
  "controlSweepReadCost",
  "failReceipt",
  "testBalance",
  "legacyOwnership",
  "advance",
  "snapshot",
  "seedAvatarSequence",
  "knownVector",
  "sparseCatalog",
]);
export default {
  async fetch(request: Request, env: any) {
    try {
      const {
        op,
        args = [],
        realm = "local-account-fixture",
      } = (await request.json()) as any;
      if (!allowed.has(op)) throw Error("unknown test operation");
      const stub = env.ACCOUNTS.get(env.ACCOUNTS.idFromName(realm));
      return Response.json(await stub[op](...args));
    } catch (e) {
      return Response.json({ error: (e as Error).message }, { status: 400 });
    }
  },
};
