import { AccountAuthority } from "../src/account/authority";
import { password_hash } from "../generated/account_crypto.js";
export class TestAccounts extends AccountAuthority {
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
  snapshot() {
    return {
      accounts: this.ctx.storage.sql.exec("SELECT * FROM accounts").toArray(),
      sessions: this.ctx.storage.sql.exec("SELECT * FROM sessions").toArray(),
      avatars: this.ctx.storage.sql.exec("SELECT * FROM avatars").toArray(),
    };
  }
}
const allowed = new Set([
  "initializeCatalog",
  "register",
  "login",
  "rotate",
  "logout",
  "createAvatar",
  "roster",
  "ownedAvatar",
  "advance",
  "snapshot",
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
