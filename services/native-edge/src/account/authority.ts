import { DurableObject } from "cloudflare:workers";
import {
  canonical,
  decimal,
  digest,
  encode,
  equal,
  hash,
} from "../lot/contracts";
import { avatar, catalog, operation, token, username } from "./contracts";
import {
  createVerifier,
  matches,
  password,
  PROFILE,
  randomToken,
} from "./crypto";

const ABSOLUTE = 12 * 60 * 60 * 1000,
  IDLE = 30 * 60 * 1000;
type Account = {
  id: string;
  username: string;
  verifier: string;
  revision: string;
};
type Session = {
  account_id: string;
  revision: string;
  expires: number;
  idle: number;
};
/** Private realm account boundary. Binding access is trusted; there is no public auth route. */
export class AccountAuthority extends DurableObject<Record<string, never>> {
  constructor(ctx: DurableObjectState, env: Record<string, never>) {
    super(ctx, env);
    ctx.storage.sql.exec(`
      CREATE TABLE IF NOT EXISTS account_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS accounts(id TEXT PRIMARY KEY,username TEXT UNIQUE NOT NULL,verifier TEXT NOT NULL,revision TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS sessions(token_hash TEXT PRIMARY KEY,account_id TEXT NOT NULL,revision TEXT NOT NULL,expires INTEGER NOT NULL,idle INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS attempts(key TEXT PRIMARY KEY,start INTEGER NOT NULL,count INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS avatars(id TEXT PRIMARY KEY,account_id TEXT NOT NULL,name_key TEXT UNIQUE NOT NULL,spec TEXT NOT NULL);
      CREATE INDEX IF NOT EXISTS sessions_account ON sessions(account_id);
      CREATE INDEX IF NOT EXISTS sessions_expiry ON sessions(expires);
      CREATE INDEX IF NOT EXISTS sessions_idle ON sessions(idle);
      CREATE INDEX IF NOT EXISTS attempts_expiry ON attempts(start);
      CREATE INDEX IF NOT EXISTS avatar_roster ON avatars(account_id,length(id),id);
      CREATE TABLE IF NOT EXISTS avatar_ownership_initializations(avatar_id TEXT PRIMARY KEY,version INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS avatar_balances(avatar_id TEXT PRIMARY KEY,balance TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS avatar_outfits(id TEXT PRIMARY KEY,avatar_id TEXT NOT NULL,asset_key TEXT NOT NULL,suit_type INTEGER NOT NULL,source TEXT NOT NULL,purchase_price TEXT NOT NULL,sale_price TEXT NOT NULL);
      CREATE INDEX IF NOT EXISTS outfits_owner ON avatar_outfits(avatar_id,length(id),id);
      CREATE TABLE IF NOT EXISTS avatar_operations(account_id TEXT NOT NULL,operation TEXT NOT NULL,digest TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(account_id,operation));
    `);
  }
  protected now() {
    return Date.now();
  }
  private next(key: string, maximum = 18446744073709551615n): string {
    const row = this.ctx.storage.sql
      .exec<{
        value: string;
      }>("SELECT value FROM account_meta WHERE key=?", key)
      .toArray()[0];
    const next = BigInt(row?.value ?? "0") + 1n;
    if (next > maximum) throw Error("identity capacity");
    this.ctx.storage.sql.exec(
      "INSERT INTO account_meta(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
      key,
      next.toString(),
    );
    return next.toString();
  }
  // Caller is the private edge admission service, not a browser-provided IP/header.
  private rate(name: string, network: unknown) {
    const origin = hash(network),
      now = this.now();
    this.ctx.storage.transactionSync(() => {
      this.ctx.storage.sql.exec(
        "DELETE FROM attempts WHERE start<=?",
        now - 60000,
      );
      const keys = [
        ["name:" + name, 5],
        ["origin:" + origin, 20],
      ] as const;
      if (
        this.ctx.storage.sql
          .exec<{ n: number }>("SELECT COUNT(*) AS n FROM attempts")
          .one().n >= 16384
      )
        throw Error("authentication temporarily busy");
      for (const [key, max] of keys) {
        const row = this.ctx.storage.sql
          .exec<{
            count: number;
          }>("SELECT count FROM attempts WHERE key=?", key)
          .toArray()[0];
        if (row && row.count >= max)
          throw Error("authentication temporarily busy");
      }
      for (const [key] of keys)
        this.ctx.storage.sql.exec(
          "INSERT INTO attempts(key,start,count) VALUES(?,?,1) ON CONFLICT(key) DO UPDATE SET count=count+1",
          key,
          now,
        );
    });
  }
  initializeCatalog(raw: unknown) {
    const value = catalog(raw);
    if (encode(value).byteLength > 1024 * 1024)
      throw Error("catalog byte profile exceeded");
    return this.ctx.storage.transactionSync(() => {
      const prior = this.ctx.storage.sql
        .exec<{
          value: string;
        }>("SELECT value FROM account_meta WHERE key='catalog'")
        .toArray()[0];
      if (prior && !equal(JSON.parse(prior.value), value))
        throw Error("catalog already pinned");
      this.ctx.storage.sql.exec(
        "INSERT OR IGNORE INTO account_meta(key,value) VALUES('catalog',?)",
        canonical(value),
      );
      return { release: value.release };
    });
  }
  register(rawName: unknown, rawPassword: unknown, network: unknown) {
    const name = username(rawName),
      bytes = password(rawPassword);
    try {
      this.rate(name, network);
    } catch (e) {
      bytes.fill(0);
      throw e;
    }
    const verifier = createVerifier(bytes);
    return this.ctx.storage.transactionSync(() => {
      if (
        this.ctx.storage.sql
          .exec("SELECT id FROM accounts WHERE username=?", name)
          .toArray().length
      )
        throw Error("registration unavailable");
      const id = this.next("account_sequence");
      this.ctx.storage.sql.exec(
        "INSERT INTO accounts(id,username,verifier,revision) VALUES(?,?,?,'1')",
        id,
        name,
        canonical(verifier),
      );
      return { account_id: id, username: name };
    });
  }
  async login(rawName: unknown, rawPassword: unknown, network: unknown) {
    const name = username(rawName),
      bytes = password(rawPassword);
    try {
      this.rate(name, network);
    } catch (e) {
      bytes.fill(0);
      throw e;
    }
    const account = this.ctx.storage.sql
      .exec<Account>("SELECT * FROM accounts WHERE username=?", name)
      .toArray()[0];
    // Unknown users perform the same expensive derivation. No account-state details escape.
    const valid = matches(
      bytes,
      account
        ? JSON.parse(account.verifier)
        : { profile: PROFILE, salt: "0".repeat(32), hash: "0".repeat(64) },
    );
    if (!valid || !account) throw Error("unauthorized");
    const secret = randomToken(),
      tokenHash = await digest(new TextEncoder().encode(secret));
    return this.ctx.storage.transactionSync(() => {
      const current = this.ctx.storage.sql
        .exec<Account>("SELECT * FROM accounts WHERE id=?", account.id)
        .one();
      if (
        current.revision !== account.revision ||
        current.verifier !== account.verifier
      )
        throw Error("authentication changed; sign in again");
      const now = this.now();
      this.ctx.storage.sql.exec(
        "DELETE FROM sessions WHERE expires<=? OR idle<=?",
        now,
        now,
      );
      if (
        this.ctx.storage.sql
          .exec<{
            n: number;
          }>(
            "SELECT COUNT(*) AS n FROM sessions WHERE account_id=?",
            account.id,
          )
          .one().n >= 16
      )
        throw Error("session capacity; sign out another session");
      this.ctx.storage.sql.exec(
        "INSERT INTO sessions(token_hash,account_id,revision,expires,idle) VALUES(?,?,?,?,?)",
        tokenHash,
        account.id,
        account.revision,
        now + ABSOLUTE,
        now + IDLE,
      );
      return {
        token: secret,
        account_id: account.id,
        username: name,
        expires_at: now + ABSOLUTE,
      };
    });
  }
  private async tokenHash(raw: unknown) {
    return digest(new TextEncoder().encode(token(raw)));
  }
  private authorize(tokenHash: string): Account {
    const session = this.ctx.storage.sql
        .exec<Session>("SELECT * FROM sessions WHERE token_hash=?", tokenHash)
        .toArray()[0],
      now = this.now();
    if (!session || session.expires <= now || session.idle <= now)
      throw Error("unauthorized");
    const account = this.ctx.storage.sql
      .exec<Account>("SELECT * FROM accounts WHERE id=?", session.account_id)
      .one();
    if (account.revision !== session.revision) throw Error("unauthorized");
    this.ctx.storage.sql.exec(
      "UPDATE sessions SET idle=? WHERE token_hash=?",
      Math.min(now + IDLE, session.expires),
      tokenHash,
    );
    return account;
  }
  async logout(raw: unknown, all = false) {
    if (typeof all !== "boolean") throw Error("invalid logout scope");
    const key = await this.tokenHash(raw);
    return this.ctx.storage.transactionSync(() => {
      const account = this.authorize(key);
      if (all) {
        const revision = BigInt(account.revision) + 1n;
        if (revision > 18446744073709551615n) throw Error("revision capacity");
        this.ctx.storage.sql.exec(
          "UPDATE accounts SET revision=? WHERE id=?",
          revision.toString(),
          account.id,
        );
        this.ctx.storage.sql.exec(
          "DELETE FROM sessions WHERE account_id=?",
          account.id,
        );
      } else
        this.ctx.storage.sql.exec(
          "DELETE FROM sessions WHERE token_hash=?",
          key,
        );
      return { signed_out: true };
    });
  }
  async rotate(raw: unknown) {
    const old = await this.tokenHash(raw),
      secret = randomToken(),
      next = await this.tokenHash(secret);
    return this.ctx.storage.transactionSync(() => {
      const account = this.authorize(old);
      const current = this.ctx.storage.sql
        .exec<Session>("SELECT * FROM sessions WHERE token_hash=?", old)
        .one();
      this.ctx.storage.sql.exec("DELETE FROM sessions WHERE token_hash=?", old);
      this.ctx.storage.sql.exec(
        "INSERT INTO sessions(token_hash,account_id,revision,expires,idle) VALUES(?,?,?,?,?)",
        next,
        account.id,
        account.revision,
        current.expires,
        current.idle,
      );
      return {
        token: secret,
        account_id: account.id,
        expires_at: current.expires,
      };
    });
  }
  async createAvatar(rawToken: unknown, rawOperation: unknown, raw: unknown) {
    const key = await this.tokenHash(rawToken),
      op = operation(rawOperation),
      spec = avatar(raw),
      sha = await digest(encode(spec));
    return this.ctx.storage.transactionSync(() => {
      const account = this.authorize(key);
      const previous = this.ctx.storage.sql
        .exec<{
          digest: string;
          result: string;
        }>(
          "SELECT digest,result FROM avatar_operations WHERE account_id=? AND operation=?",
          account.id,
          op,
        )
        .toArray()[0];
      if (previous) {
        if (previous.digest !== sha) throw Error("operation identity reused");
        return JSON.parse(previous.result);
      }
      const record = this.ctx.storage.sql
        .exec<{
          value: string;
        }>("SELECT value FROM account_meta WHERE key='catalog'")
        .toArray()[0];
      if (!record) throw Error("avatar content unavailable");
      const release = catalog(JSON.parse(record.value));
      if (spec.release !== release.release)
        throw Error("content release mismatch");
      for (const part of ["head", "body"] as const) {
        const asset = release.entries.find(
          (e) => e.key === spec[(part + "_key") as "head_key" | "body_key"],
        );
        if (
          !asset ||
          asset.part !== part ||
          asset.gender !== spec.gender ||
          !asset.skins.includes(spec.skin)
        )
          throw Error("appearance unavailable in pinned content");
      }
      if (
        this.ctx.storage.sql
          .exec<{
            n: number;
          }>(
            "SELECT COUNT(*) AS n FROM avatar_operations WHERE account_id=?",
            account.id,
          )
          .one().n >= 4096
      )
        throw Error("operation retention capacity");
      if (
        this.ctx.storage.sql
          .exec<{
            n: number;
          }>("SELECT COUNT(*) AS n FROM avatars WHERE account_id=?", account.id)
          .one().n >= 3
      )
        throw Error("source account avatar limit");
      const nameKey = spec.name.normalize("NFKC").toLowerCase();
      if (
        this.ctx.storage.sql
          .exec("SELECT id FROM avatars WHERE name_key=?", nameKey)
          .toArray().length
      )
        throw Error("avatar name unavailable");
      // Source classifies persistent IDs >= 2^24 as objects, not avatars.
      const id = this.next("avatar_sequence", 16777215n),
        result = { avatar_id: id, ...spec };
      this.ctx.storage.sql.exec(
        "INSERT INTO avatars(id,account_id,name_key,spec) VALUES(?,?,?,?)",
        id,
        account.id,
        nameKey,
        canonical(spec),
      );
      // Original ordinary CAS creation grants selected daywear and source defaults.
      // Catalog eligibility is not ownership; each grant has its own instance ID.
      const defaults =
        spec.gender === "male"
          ? ["5789615915021", "5802500816909"]
          : ["5587752452109", "420906795021"];
      for (const [asset, suit] of [
        [spec.body_key, 0],
        [defaults[0], 5],
        [defaults[1], 2],
      ] as const) {
        this.ctx.storage.sql.exec(
          "INSERT INTO avatar_outfits(id,avatar_id,asset_key,suit_type,source,purchase_price,sale_price) VALUES(?,?,?,?,?,?,?)",
          this.next("outfit_sequence"),
          id,
          asset,
          suit,
          "cas",
          "0",
          "0",
        );
      }
      this.ctx.storage.sql.exec(
        "INSERT INTO avatar_balances(avatar_id,balance) VALUES(?,?)",
        id,
        "0",
      );
      this.ctx.storage.sql.exec(
        "INSERT INTO avatar_ownership_initializations(avatar_id,version) VALUES(?,1)",
        id,
      );
      this.ctx.storage.sql.exec(
        "INSERT INTO avatar_operations(account_id,operation,digest,result) VALUES(?,?,?,?)",
        account.id,
        op,
        sha,
        canonical(result),
      );
      return result;
    });
  }
  async roster(raw: unknown, after: unknown = "0") {
    const key = await this.tokenHash(raw),
      cursor = decimal(after);
    return this.ctx.storage.transactionSync(() => {
      const account = this.authorize(key);
      const rows = this.ctx.storage.sql
        .exec<{
          id: string;
          spec: string;
        }>(
          "SELECT id,spec FROM avatars WHERE account_id=? AND (length(id)>length(?) OR (length(id)=length(?) AND id>?)) ORDER BY length(id),id LIMIT 51",
          account.id,
          cursor,
          cursor,
          cursor,
        )
        .toArray();
      return {
        account_id: account.id,
        avatars: rows
          .slice(0, 50)
          .map((r) => ({ avatar_id: r.id, ...JSON.parse(r.spec) })),
        next: rows.length > 50 ? rows[49].id : null,
      };
    });
  }
  async ownedAvatar(raw: unknown, id: unknown) {
    const key = await this.tokenHash(raw),
      avatarId = decimal(id);
    return this.ctx.storage.transactionSync(() => {
      const account = this.authorize(key);
      const row = this.ctx.storage.sql
        .exec<{
          spec: string;
        }>(
          "SELECT spec FROM avatars WHERE id=? AND account_id=?",
          avatarId,
          account.id,
        )
        .toArray()[0];
      if (!row) throw Error("avatar unavailable");
      return {
        account_id: account.id,
        avatar_id: avatarId,
        ...JSON.parse(row.spec),
      };
    });
  }
  async ownedAssets(raw: unknown, id: unknown) {
    const key = await this.tokenHash(raw),
      avatarId = decimal(id);
    return this.ctx.storage.transactionSync(() => {
      const account = this.authorize(key);
      if (
        !this.ctx.storage.sql
          .exec(
            "SELECT id FROM avatars WHERE id=? AND account_id=?",
            avatarId,
            account.id,
          )
          .toArray().length
      )
        throw Error("avatar unavailable");
      const initialized = this.ctx.storage.sql
        .exec<{
          version: number;
        }>("SELECT version FROM avatar_ownership_initializations WHERE avatar_id=?", avatarId)
        .toArray()[0];
      // Existing avatars are never silently granted assets by a read or retry.
      // They need an explicitly reviewed migration before native admission.
      if (initialized?.version !== 1)
        throw Error("avatar ownership migration required");
      const balance = this.ctx.storage.sql
        .exec<{
          balance: string;
        }>("SELECT balance FROM avatar_balances WHERE avatar_id=?", avatarId)
        .toArray()[0];
      if (!balance) throw Error("avatar balance unavailable");
      const outfits = this.ctx.storage.sql
        .exec<{
          id: string;
          asset_key: string;
          suit_type: number;
          source: string;
          purchase_price: string;
          sale_price: string;
        }>("SELECT id,asset_key,suit_type,source,purchase_price,sale_price FROM avatar_outfits WHERE avatar_id=? ORDER BY length(id),id", avatarId)
        .toArray();
      return {
        version: 1,
        account_id: account.id,
        avatar_id: avatarId,
        balance: balance.balance,
        outfits,
      };
    });
  }
}
