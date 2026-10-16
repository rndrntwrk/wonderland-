import { controlTarget, type ControlClaim } from "./control";
import { AccountLotSaveStore } from "./lot-save";
import {
  configuredDepartureReceiptReader,
  type DepartureReceiptEnv,
} from "./departure-proof";
import {
  initialAvatarSource,
  permissionBinding,
  PORTABLE_LIMIT,
  type AvatarSource,
  type ControlLeaseV2,
  type ReservedAvatarExportV2,
} from "./portable";
import { DurableObject } from "cloudflare:workers";
import {
  canonical,
  decimal,
  digest,
  encode,
  equal,
  fields,
  hash,
} from "../lot/contracts";
import {
  avatar,
  avatar as avatarSpec,
  catalog,
  operation,
  token,
  username,
} from "./contracts";
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
export class AccountAuthority extends DurableObject<DepartureReceiptEnv> {
  private readonly lotSaves: AccountLotSaveStore;
  constructor(ctx: DurableObjectState, env: DepartureReceiptEnv) {
    super(ctx, env);
    ctx.storage.sql.exec(`
      CREATE TABLE IF NOT EXISTS account_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS account_source_state(account_id TEXT PRIMARY KEY,registered_at_ms INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS avatar_source_state(avatar_id TEXT PRIMARY KEY,source_json TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS avatar_control_leases(avatar_id TEXT PRIMARY KEY,revision TEXT NOT NULL,expires_at INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS avatar_lease_operations(account_id TEXT NOT NULL,operation TEXT NOT NULL,request_json TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(account_id,operation));
      CREATE TABLE IF NOT EXISTS avatar_control_exports(avatar_id TEXT PRIMARY KEY,operation TEXT NOT NULL,source_json TEXT NOT NULL,export_json TEXT NOT NULL);
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
      CREATE TABLE IF NOT EXISTS avatar_control_claims(avatar_id TEXT PRIMARY KEY,account_id TEXT NOT NULL,session_hash TEXT NOT NULL,session_revision TEXT NOT NULL,operation TEXT NOT NULL,digest TEXT NOT NULL,claim TEXT NOT NULL);
      CREATE UNIQUE INDEX IF NOT EXISTS controls_operation ON avatar_control_claims(account_id,operation);
      CREATE INDEX IF NOT EXISTS controls_account ON avatar_control_claims(account_id);
      CREATE INDEX IF NOT EXISTS controls_avatar_numeric ON avatar_control_claims(CAST(avatar_id AS INTEGER));
      CREATE TABLE IF NOT EXISTS avatar_operations(account_id TEXT NOT NULL,operation TEXT NOT NULL,digest TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(account_id,operation));
    `);
    this.lotSaves = new AccountLotSaveStore(
      ctx.storage,
      configuredDepartureReceiptReader(env),
    );
  }
  /** Trusted binding only. Source bytes are fetched from committed lot evidence;
   * no browser HTTP route accepts these methods or writable avatar state.
   */
  applyCommittedAvatarLotSaveV1(reference: unknown) {
    return this.lotSaves.applyCommittedAvatarLotSaveV1(reference);
  }
  releaseSavedAvatarControlV1(reference: unknown) {
    return this.lotSaves.releaseSavedAvatarControlV1(reference);
  }
  lookupSavedAvatarControlV1(raw: unknown) {
    const v = fields(raw, ["identity", "departure_operation"]);
    return this.lotSaves.lookupSavedAvatarControlV1(
      v.identity,
      v.departure_operation,
    );
  }
  lookupReleasedAvatarControlV1(raw: unknown) {
    const v = fields(raw, ["identity", "departure_operation"]);
    return this.lotSaves.lookupReleasedAvatarControlV1(
      v.identity,
      v.departure_operation,
    );
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
      this.ctx.storage.sql.exec(
        "INSERT INTO account_source_state(account_id,registered_at_ms) VALUES(?,?)",
        id,
        Math.floor(this.now() / 1000) * 1000,
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
  private authorize(tokenHash: string, touchIdle = true): Account {
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
    if (touchIdle)
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
      this.fenceControls(account.id, all ? undefined : key);
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
      this.fenceControls(account.id, old);
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
  /** Reservation only: Rust admission and lot-side fencing must precede gameplay. */
  async reserveAvatarControl(
    rawToken: unknown,
    rawOperation: unknown,
    rawTarget: unknown,
  ) {
    const key = await this.tokenHash(rawToken),
      op = operation(rawOperation),
      target = controlTarget(rawTarget);
    const sha = await digest(encode({ session_hash: key, target }));
    return this.ctx.storage.transactionSync(() => {
      const account = this.authorize(key);
      const owned = this.ctx.storage.sql
        .exec<{
          spec: string;
        }>(
          "SELECT spec FROM avatars WHERE id=? AND account_id=?",
          target.avatar_id,
          account.id,
        )
        .toArray()[0];
      if (!owned) throw Error("avatar unavailable");
      if (JSON.parse(owned.spec).release !== target.content_release)
        throw Error("content release mismatch");
      const initialized = this.ctx.storage.sql
        .exec<{
          version: number;
        }>(
          "SELECT version FROM avatar_ownership_initializations WHERE avatar_id=?",
          target.avatar_id,
        )
        .toArray()[0];
      if (initialized?.version !== 1)
        throw Error("avatar ownership migration required");
      const old = this.ctx.storage.sql
        .exec<{
          operation: string;
          digest: string;
          claim: string;
        }>(
          "SELECT operation,digest,claim FROM avatar_control_claims WHERE avatar_id=?",
          target.avatar_id,
        )
        .toArray()[0];
      if (old) {
        if (old.operation !== op || old.digest !== sha)
          throw Error("avatar control already reserved");
        return this.observeControl(target.avatar_id, old.claim);
      }
      // Existing exact reservation retries still observe their fenced claim.
      // Tombstones prevent a retired operation from allocating a new claim.
      this.lotSaves.assertReservationOperationUnused(account.id, op);
      const session = this.ctx.storage.sql
        .exec<Session>("SELECT * FROM sessions WHERE token_hash=?", key)
        .one();
      const claim: ControlClaim = {
        version: 1,
        account_id: account.id,
        ...target,
        generation: this.next("control_generation"),
        expires_at: Math.min(
          Math.floor(this.now() / 1000) * 1000 + 60000,
          session.expires,
          session.idle,
        ),
        state: "reserved",
      };
      this.ctx.storage.sql.exec(
        "INSERT INTO avatar_control_claims(avatar_id,account_id,session_hash,session_revision,operation,digest,claim) VALUES(?,?,?,?,?,?,?)",
        target.avatar_id,
        account.id,
        key,
        account.revision,
        op,
        sha,
        canonical(claim),
      );
      this.ctx.storage.sql.exec(
        "INSERT INTO avatar_control_leases(avatar_id,revision,expires_at) VALUES(?,'1',?)",
        target.avatar_id,
        claim.expires_at,
      );
      return claim;
    });
  }
  private observeControl(avatarId: string, encoded: string): ControlClaim {
    const claim = JSON.parse(encoded) as ControlClaim;
    if (claim.expires_at <= this.now() && claim.state === "reserved") {
      claim.state = "fence_pending";
      this.ctx.storage.sql.exec(
        "UPDATE avatar_control_claims SET claim=? WHERE avatar_id=?",
        canonical(claim),
        avatarId,
      );
    }
    return claim;
  }
  /** Private realm configuration, never supplied by a browser export caller. */
  initializeAdmissionAuthorityV2(rawAuthority: unknown) {
    const authority_id = hash(rawAuthority);
    return this.ctx.storage.transactionSync(() => {
      const prior = this.ctx.storage.sql
        .exec<{
          value: string;
        }>("SELECT value FROM account_meta WHERE key='admission_authority_v2'")
        .toArray()[0];
      if (prior && prior.value !== authority_id)
        throw Error("admission authority already pinned");
      this.ctx.storage.sql.exec(
        "INSERT OR IGNORE INTO account_meta(key,value) VALUES('admission_authority_v2',?)",
        authority_id,
      );
      return { authority_id };
    });
  }
  private reservedSourceV2(
    sessionRef: string,
    op: string,
    avatarId: string,
    permission: ReturnType<typeof permissionBinding>,
    observedAt: number,
  ) {
    const account = this.authorize(sessionRef, false);
    const authority = this.ctx.storage.sql
      .exec<{
        value: string;
      }>("SELECT value FROM account_meta WHERE key='admission_authority_v2'")
      .toArray()[0];
    if (!authority) throw Error("admission authority unavailable");
    const row = this.ctx.storage.sql
      .exec<{
        account_id: string;
        session_hash: string;
        session_revision: string;
        operation: string;
        digest: string;
        claim: string;
      }>("SELECT * FROM avatar_control_claims WHERE avatar_id=?", avatarId)
      .toArray()[0];
    if (
      !row ||
      row.account_id !== account.id ||
      row.session_hash !== sessionRef ||
      row.session_revision !== account.revision ||
      row.operation !== op
    )
      throw Error("reserved avatar session mismatch");
    const claim = this.observeControl(avatarId, row.claim);
    // Return denial so expiry's fence obligation commits instead of rolling back.
    if (claim.state !== "reserved") return null;
    const owned = this.ctx.storage.sql
      .exec<{
        spec: string;
      }>(
        "SELECT spec FROM avatars WHERE id=? AND account_id=?",
        avatarId,
        account.id,
      )
      .toArray()[0];
    const accountSource = this.ctx.storage.sql
      .exec<{
        registered_at_ms: number;
      }>(
        "SELECT registered_at_ms FROM account_source_state WHERE account_id=?",
        account.id,
      )
      .toArray()[0];
    const source = this.ctx.storage.sql
      .exec<{
        source_json: string;
      }>(
        "SELECT source_json FROM avatar_source_state WHERE avatar_id=?",
        avatarId,
      )
      .toArray()[0];
    if (!owned || !accountSource || !source)
      throw Error("avatar source migration required");
    const initialized = this.ctx.storage.sql
      .exec<{
        version: number;
      }>(
        "SELECT version FROM avatar_ownership_initializations WHERE avatar_id=?",
        avatarId,
      )
      .toArray()[0];
    if (initialized?.version !== 1)
      throw Error("avatar ownership migration required");
    const balance = this.ctx.storage.sql
      .exec<{
        balance: string;
      }>("SELECT balance FROM avatar_balances WHERE avatar_id=?", avatarId)
      .toArray()[0];
    if (!balance || BigInt(decimal(balance.balance)) > 4294967295n)
      throw Error("avatar budget unavailable");
    const outfits = this.ctx.storage.sql
      .exec<{
        id: string;
        asset_key: string;
        suit_type: number;
        source: string;
        purchase_price: string;
        sale_price: string;
      }>(
        "SELECT id,asset_key,suit_type,source,purchase_price,sale_price FROM avatar_outfits WHERE avatar_id=? ORDER BY length(id),id LIMIT 257",
        avatarId,
      )
      .toArray();
    if (outfits.length > 256) throw Error("portable owned assets capacity");
    return {
      authority_id: authority.value,
      account_id: account.id,
      account_revision: account.revision,
      session_ref: sessionRef,
      admission_operation: op,
      reservation_digest: row.digest,
      claim,
      spec: JSON.parse(owned.spec) as ReturnType<typeof avatarSpec>,
      source: JSON.parse(source.source_json) as AvatarSource,
      registered_at_ms: accountSource.registered_at_ms,
      balance: balance.balance,
      outfits,
      permission_binding: permission,
      observed_at_ms: observedAt,
    };
  }
  private sourceIdentityV2(
    source: NonNullable<ReturnType<AccountAuthority["reservedSourceV2"]>>,
  ) {
    return { ...source, claim: { ...source.claim, expires_at: 0 } };
  }
  /** Private source export; this never grants an admitted actor. */
  async exportReservedAvatarV2(
    rawToken: unknown,
    rawOperation: unknown,
    rawAvatar: unknown,
    rawPermission: unknown,
  ) {
    const sessionRef = await this.tokenHash(rawToken),
      op = operation(rawOperation),
      avatarId = decimal(rawAvatar),
      permission = permissionBinding(rawPermission);
    const cached = this.ctx.storage.sql
      .exec<{
        operation: string;
        source_json: string;
        export_json: string;
      }>(
        "SELECT operation,source_json,export_json FROM avatar_control_exports WHERE avatar_id=?",
        avatarId,
      )
      .toArray()[0];
    if (cached) {
      if (cached.operation !== op) throw Error("avatar export identity reused");
      const exported = JSON.parse(cached.export_json) as ReservedAvatarExportV2;
      if (!equal(exported.permission_binding, permission))
        throw Error("avatar export permission binding changed");
      const current = this.ctx.storage.transactionSync(() =>
        this.reservedSourceV2(
          sessionRef,
          op,
          avatarId,
          permission,
          exported.permission_source.observed_at_ms,
        ),
      );
      if (!current) throw Error("avatar control fenced");
      if (canonical(this.sourceIdentityV2(current)) !== cached.source_json)
        throw Error("avatar source revision changed");
      return exported;
    }
    const observed = Math.floor(this.now() / 1000) * 1000;
    const source = this.ctx.storage.transactionSync(() =>
      this.reservedSourceV2(sessionRef, op, avatarId, permission, observed),
    );
    if (!source) throw Error("avatar control fenced");
    const { spec, claim, outfits } = source,
      persisted = source.source;
    if (
      persisted.schema_version !== 1 ||
      !Number.isInteger(persisted.moderation_level) ||
      persisted.moderation_level < 0 ||
      persisted.moderation_level > 255
    )
      throw Error("unsupported source avatar profile");
    for (const [asset, suit] of [
      [persisted.default_suits.daywear, 0],
      [persisted.default_suits.swimwear, 2],
      [persisted.default_suits.sleepwear, 5],
    ] as const)
      if (
        !outfits.some(
          (outfit) => outfit.asset_key === asset && outfit.suit_type === suit,
        )
      )
        throw Error("portable outfit is not owned");
    const age = Math.floor(
      (Math.floor(observed / 1000) -
        Math.floor(persisted.created_at_ms / 1000)) /
        86400,
    );
    if (age < 0 || observed < Math.floor(source.registered_at_ms / 1000) * 1000)
      throw Error("source clock moved backwards");
    const person_data = [...persisted.person_data];
    person_data[10] = ((20 + Math.floor(age / 7)) << 16) >> 16; // StateFromDB unchecked i16 narrowing.
    const portable = {
      name: spec.name,
      persistent_id: avatarId,
      default_suits: persisted.default_suits,
      // Runtime must derive final role/flags from bound source facts and lot metadata.
      budget_mirror: source.balance,
      body:
        persisted.body_current === "0" ? spec.body_key : persisted.body_current,
      head: spec.head_key,
      skin_tone: ["light", "medium", "dark"].indexOf(spec.skin),
      is_worker: false,
      custom_guid: 0,
      motive_data: persisted.motive_data,
      person_data,
      relationships: persisted.relationships,
      job_info: persisted.job_info,
      ignored_avatars: persisted.ignored_avatars,
    };
    const portable_json = canonical(portable);
    if (new TextEncoder().encode(portable_json).byteLength > PORTABLE_LIMIT)
      throw Error("portable avatar byte capacity");
    const permission_source = {
      registered_at_ms: source.registered_at_ms,
      created_at_ms: persisted.created_at_ms,
      observed_at_ms: observed,
      roommate_lots: persisted.roommate_lots,
      moderation_level: persisted.moderation_level,
      mayor_nhood: persisted.mayor_nhood,
    };
    const source_revisions = {
      owned_assets: 1 as const,
      wardrobe: await digest(encode(outfits)),
      ledger: await digest(encode({ balance: source.balance })),
      progression: await digest(encode(persisted)),
      permissions: await digest(
        encode({ permission_source, permission_binding: permission }),
      ),
    };
    const payload = {
      schema_version: 2 as const,
      authority_id: source.authority_id,
      account_id: source.account_id,
      avatar_id: avatarId,
      lot_id: claim.lot_id,
      control_generation: claim.generation,
      admission_operation: op,
      reservation_digest: source.reservation_digest,
      session_ref: sessionRef,
      account_revision: source.account_revision,
      content_release: claim.content_release,
      protocol_release: claim.protocol_release,
      expires_at: claim.expires_at,
      permission_binding: permission,
      permission_source,
      source_revisions,
      portable_json,
      portable_sha256: await digest(new TextEncoder().encode(portable_json)),
    };
    const exported: ReservedAvatarExportV2 = {
      ...payload,
      export_digest: await digest(encode(payload)),
    };
    const installed = this.ctx.storage.transactionSync(() => {
      const current = this.reservedSourceV2(
        sessionRef,
        op,
        avatarId,
        permission,
        observed,
      );
      if (!current) return false;
      if (!equal(current, source))
        throw Error("avatar source revision changed during export");
      const prior = this.ctx.storage.sql
        .exec<{
          export_json: string;
        }>(
          "SELECT export_json FROM avatar_control_exports WHERE avatar_id=?",
          avatarId,
        )
        .toArray()[0];
      if (prior && !equal(JSON.parse(prior.export_json), exported))
        throw Error("avatar export raced");
      this.ctx.storage.sql.exec(
        "INSERT OR IGNORE INTO avatar_control_exports(avatar_id,operation,source_json,export_json) VALUES(?,?,?,?)",
        avatarId,
        op,
        canonical(this.sourceIdentityV2(source)),
        canonical(exported),
      );
      return true;
    });
    if (!installed) throw Error("avatar control fenced");
    return exported;
  }
  /** Authenticated recovery read: never replaces the frozen permission binding. */
  async getReservedAvatarExportV2(
    rawToken: unknown,
    rawOperation: unknown,
    rawAvatar: unknown,
  ) {
    const sessionRef = await this.tokenHash(rawToken),
      op = operation(rawOperation),
      avatarId = decimal(rawAvatar);
    const saved = this.ctx.storage.sql
      .exec<{
        export_json: string;
      }>(
        "SELECT export_json FROM avatar_control_exports WHERE avatar_id=? AND operation=?",
        avatarId,
        op,
      )
      .toArray()[0];
    if (!saved) throw Error("owned admission export unavailable");
    const exported = JSON.parse(saved.export_json) as ReservedAvatarExportV2;
    if (exported.session_ref !== sessionRef)
      throw Error("reserved avatar session mismatch");
    this.revalidateReservedAvatarV2(exported);
    return exported;
  }
  /** Cross-object authorization handshake; coordinator must still serialize fences. */
  revalidateReservedAvatarV2(raw: unknown) {
    if (!raw || typeof raw !== "object") throw Error("invalid avatar export");
    const exported = raw as ReservedAvatarExportV2;
    const avatarId = decimal(exported.avatar_id);
    const result = this.ctx.storage.transactionSync(() => {
      const saved = this.ctx.storage.sql
        .exec<{
          source_json: string;
          export_json: string;
        }>(
          "SELECT source_json,export_json FROM avatar_control_exports WHERE avatar_id=?",
          avatarId,
        )
        .toArray()[0];
      if (!saved || !equal(JSON.parse(saved.export_json), raw))
        throw Error("unknown or changed avatar export");
      const current = this.reservedSourceV2(
        exported.session_ref,
        exported.admission_operation,
        avatarId,
        exported.permission_binding,
        exported.permission_source.observed_at_ms,
      );
      if (!current) return null;
      if (canonical(this.sourceIdentityV2(current)) !== saved.source_json)
        throw Error("avatar source revision changed");
      const lease = this.ctx.storage.sql
        .exec<{
          revision: string;
          expires_at: number;
        }>(
          "SELECT revision,expires_at FROM avatar_control_leases WHERE avatar_id=?",
          avatarId,
        )
        .toArray()[0];
      if (!lease) throw Error("control lease migration required");
      return {
        status: "validated" as const,
        export: exported,
        validated_at: this.now(),
        lease,
      };
    });
    if (!result) throw Error("avatar control fenced");
    return result;
  }
  /** Authenticated browser activity/presence only, never an autonomous server tick. */
  async renewAvatarControlV2(
    rawToken: unknown,
    rawExport: unknown,
    rawOperation: unknown,
    mode: "activity" | "presence" = "activity",
  ): Promise<ControlLeaseV2> {
    if (mode !== "activity" && mode !== "presence")
      throw Error("invalid lease renewal mode");
    const sessionRef = await this.tokenHash(rawToken),
      op = operation(rawOperation),
      e = rawExport as ReservedAvatarExportV2;
    const avatarId = decimal(e?.avatar_id);
    const result = this.ctx.storage.transactionSync(() => {
      const account = this.authorize(sessionRef, mode === "activity");
      const saved = this.ctx.storage.sql
        .exec<{
          source_json: string;
          export_json: string;
        }>(
          "SELECT source_json,export_json FROM avatar_control_exports WHERE avatar_id=?",
          avatarId,
        )
        .toArray()[0];
      if (
        !saved ||
        !equal(JSON.parse(saved.export_json), rawExport) ||
        e.session_ref !== sessionRef ||
        e.account_id !== account.id
      )
        throw Error("unknown avatar lease owner");
      const current = this.reservedSourceV2(
        sessionRef,
        e.admission_operation,
        avatarId,
        e.permission_binding,
        e.permission_source.observed_at_ms,
      );
      if (!current) return null;
      if (canonical(this.sourceIdentityV2(current)) !== saved.source_json)
        throw Error("avatar source revision changed");
      const request = canonical({
        export_digest: e.export_digest,
        session_ref: sessionRef,
        mode,
      });
      const previous = this.ctx.storage.sql
        .exec<{
          request_json: string;
          result: string;
        }>(
          "SELECT request_json,result FROM avatar_lease_operations WHERE account_id=? AND operation=?",
          account.id,
          op,
        )
        .toArray()[0];
      if (previous) {
        if (previous.request_json !== request)
          throw Error("lease operation identity reused");
        return JSON.parse(previous.result);
      }
      if (
        this.ctx.storage.sql
          .exec<{
            n: number;
          }>(
            "SELECT COUNT(*) AS n FROM avatar_lease_operations WHERE account_id=?",
            account.id,
          )
          .one().n >= 4096
      )
        throw Error("lease operation retention capacity");
      const lease = this.ctx.storage.sql
        .exec<{
          revision: string;
          expires_at: number;
        }>(
          "SELECT revision,expires_at FROM avatar_control_leases WHERE avatar_id=?",
          avatarId,
        )
        .toArray()[0];
      if (!lease) throw Error("control lease migration required");
      const next = BigInt(lease.revision) + 1n;
      if (next > 18446744073709551615n) throw Error("lease revision capacity");
      const session = this.ctx.storage.sql
        .exec<Session>("SELECT * FROM sessions WHERE token_hash=?", sessionRef)
        .one();
      const expires_at = Math.min(
        this.now() + 60000,
        session.expires,
        session.idle,
      );
      const receipt: ControlLeaseV2 = {
        schema_version: 2,
        status: "renewed",
        authority_id: e.authority_id,
        account_id: account.id,
        avatar_id: avatarId,
        control_generation: e.control_generation,
        admission_operation: e.admission_operation,
        export_digest: e.export_digest,
        session_ref: sessionRef,
        lease_revision: next.toString(),
        expires_at,
        renewal_operation: op,
      };
      this.ctx.storage.sql.exec(
        "UPDATE avatar_control_leases SET revision=?,expires_at=? WHERE avatar_id=?",
        next.toString(),
        expires_at,
        avatarId,
      );
      this.ctx.storage.sql.exec(
        "UPDATE avatar_control_claims SET claim=? WHERE avatar_id=?",
        canonical({ ...current.claim, expires_at }),
        avatarId,
      );
      this.ctx.storage.sql.exec(
        "INSERT INTO avatar_lease_operations(account_id,operation,request_json,result) VALUES(?,?,?,?)",
        account.id,
        op,
        request,
        canonical(receipt),
      );
      return receipt;
    });
    if (!result) throw Error("avatar control fenced");
    return result;
  }
  /** Binding-only validation does not refresh account idle activity. */
  revalidateControlLeaseV2(raw: unknown): {
    status: "validated";
    lease: ControlLeaseV2;
    validated_at: number;
  } {
    if (!raw || typeof raw !== "object") throw Error("invalid control lease");
    const value = raw as {
      account_id: string;
      avatar_id: string;
      renewal_operation: string;
    };
    const saved = this.ctx.storage.sql
      .exec<{
        result: string;
      }>(
        "SELECT result FROM avatar_lease_operations WHERE account_id=? AND operation=?",
        decimal(value.account_id),
        operation(value.renewal_operation),
      )
      .toArray()[0];
    if (!saved || !equal(JSON.parse(saved.result), raw))
      throw Error("unknown or changed control lease");
    const e = this.ctx.storage.sql
      .exec<{
        export_json: string;
      }>(
        "SELECT export_json FROM avatar_control_exports WHERE avatar_id=?",
        decimal(value.avatar_id),
      )
      .toArray()[0];
    if (!e) throw Error("unknown control lease avatar");
    const validated = this.revalidateReservedAvatarV2(
        JSON.parse(e.export_json),
      ),
      receipt = JSON.parse(saved.result) as ControlLeaseV2;
    if (
      validated.lease.revision !== receipt.lease_revision ||
      validated.lease.expires_at !== receipt.expires_at
    )
      throw Error("stale control lease revision");
    return { status: "validated", lease: receipt, validated_at: this.now() };
  }
  /** No timeout, read, logout or rotation releases custody for a second actor. */
  async avatarControl(rawToken: unknown, rawAvatar: unknown) {
    const key = await this.tokenHash(rawToken),
      avatarId = decimal(rawAvatar);
    return this.ctx.storage.transactionSync(() => {
      const account = this.authorize(key);
      const row = this.ctx.storage.sql
        .exec<{
          claim: string;
        }>(
          "SELECT claim FROM avatar_control_claims WHERE avatar_id=? AND account_id=?",
          avatarId,
          account.id,
        )
        .toArray()[0];
      return row ? this.observeControl(avatarId, row.claim) : null;
    });
  }
  /** An authenticated owned leave fences input immediately, but never releases
   * custody. The private runner forwards this existing fence obligation to the
   * exact installed lot control; source save/detach proof is still required.
   */
  async requestAvatarControlFenceV1(rawToken: unknown, rawExpected: unknown) {
    const expected = fields(rawExpected, [
      "avatar_id",
      "generation",
      "admission_operation",
      "reservation_digest",
    ]);
    const key = await this.tokenHash(rawToken),
      avatarId = decimal(expected.avatar_id),
      generation = decimal(expected.generation),
      admissionOperation = operation(expected.admission_operation),
      reservationDigest = hash(expected.reservation_digest);
    return this.ctx.storage.transactionSync(() => {
      const account = this.authorize(key, false);
      const row = this.ctx.storage.sql
        .exec<{
          session_hash: string;
          operation: string;
          digest: string;
          claim: string;
        }>(
          "SELECT session_hash,operation,digest,claim FROM avatar_control_claims WHERE avatar_id=? AND account_id=?",
          avatarId,
          account.id,
        )
        .toArray()[0];
      if (!row || row.session_hash !== key)
        throw Error("owned departure session mismatch");
      const claim = JSON.parse(row.claim) as ControlClaim;
      if (
        claim.generation !== generation ||
        row.operation !== admissionOperation ||
        row.digest !== reservationDigest
      )
        throw Error("owned departure admission mismatch");
      claim.state = "fence_pending";
      this.ctx.storage.sql.exec(
        "UPDATE avatar_control_claims SET claim=? WHERE avatar_id=? AND account_id=? AND session_hash=?",
        canonical(claim),
        avatarId,
        account.id,
        key,
      );
      return { claim, operation: row.operation, request_digest: row.digest };
    });
  }
  /** Trusted binding sweeper: expiry fencing must not depend on a live user token. */
  pendingControlFences(after: unknown = "0") {
    const cursor = decimal(after);
    if (BigInt(cursor) > 16777215n) return { fences: [], next: null };
    return this.ctx.storage.transactionSync(() => {
      const rows = this.ctx.storage.sql
        .exec<{
          avatar_id: string;
          operation: string;
          digest: string;
          claim: string;
        }>(
          "SELECT avatar_id,operation,digest,claim FROM avatar_control_claims WHERE CAST(avatar_id AS INTEGER)>? ORDER BY CAST(avatar_id AS INTEGER) LIMIT 51",
          Number(cursor),
        )
        .toArray();
      const fences = rows.slice(0, 50).flatMap((row) => {
        const claim = this.observeControl(row.avatar_id, row.claim);
        return claim.state === "fence_pending"
          ? [{ claim, operation: row.operation, request_digest: row.digest }]
          : [];
      });
      return { fences, next: rows.length > 50 ? rows[49].avatar_id : null };
    });
  }
  private fenceControls(accountId: string, sessionHash?: string) {
    const rows = sessionHash
      ? this.ctx.storage.sql
          .exec<{
            avatar_id: string;
            claim: string;
          }>(
            "SELECT avatar_id,claim FROM avatar_control_claims WHERE account_id=? AND session_hash=?",
            accountId,
            sessionHash,
          )
          .toArray()
      : this.ctx.storage.sql
          .exec<{
            avatar_id: string;
            claim: string;
          }>(
            "SELECT avatar_id,claim FROM avatar_control_claims WHERE account_id=?",
            accountId,
          )
          .toArray();
    for (const row of rows) {
      const claim = JSON.parse(row.claim) as ControlClaim;
      claim.state = "fence_pending";
      this.ctx.storage.sql.exec(
        "UPDATE avatar_control_claims SET claim=? WHERE avatar_id=?",
        canonical(claim),
        row.avatar_id,
      );
    }
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
      this.ctx.storage.sql.exec(
        "INSERT INTO avatar_source_state(avatar_id,source_json) VALUES(?,?)",
        id,
        canonical(
          initialAvatarSource(Math.floor(this.now() / 1000) * 1000, spec),
        ),
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
  async ownedAvatar(raw: unknown, id: unknown, touchIdle = true) {
    if (typeof touchIdle !== "boolean") throw Error("invalid activity mode");
    const key = await this.tokenHash(raw),
      avatarId = decimal(id);
    return this.ctx.storage.transactionSync(() => {
      const account = this.authorize(key, touchIdle);
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
        }>(
          "SELECT version FROM avatar_ownership_initializations WHERE avatar_id=?",
          avatarId,
        )
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
        }>(
          "SELECT id,asset_key,suit_type,source,purchase_price,sale_price FROM avatar_outfits WHERE avatar_id=? ORDER BY length(id),id",
          avatarId,
        )
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
