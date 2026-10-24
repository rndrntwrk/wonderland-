// Test-only journal/funding/fault setup. Never bundled by src/index.ts.
import { TestAccounts } from "./account-driver";
import { LotCoordinator } from "../src/lot/coordinator";
import { canonical, digest, encode } from "../src/lot/contracts";
import { accountHttp } from "../src/http/accounts";
export class PurchaseAccounts extends TestAccounts {
  purchaseSnapshot() {
    return {
      balances: this.ctx.storage.sql
        .exec("SELECT * FROM avatar_balances ORDER BY avatar_id")
        .toArray(),
      objects: this.ctx.storage.sql
        .exec("SELECT * FROM native_owned_objects ORDER BY persistent_id")
        .toArray(),
      receipts: this.ctx.storage.sql
        .exec("SELECT * FROM native_purchase_receipts ORDER BY receipt_id")
        .toArray(),
      sequences: this.ctx.storage.sql
        .exec(
          "SELECT * FROM account_meta WHERE key LIKE 'purchase_%' ORDER BY key",
        )
        .toArray(),
    };
  }
  purchaseFault(enabled: boolean) {
    this.ctx.storage.sql.exec("DROP TRIGGER IF EXISTS test_purchase_fault");
    if (enabled)
      this.ctx.storage.sql.exec(
        "CREATE TRIGGER test_purchase_fault BEFORE INSERT ON native_purchase_receipts BEGIN SELECT RAISE(ABORT,'test purchase receipt failure'); END",
      );
    return true;
  }
}
export class PurchaseLot extends LotCoordinator {
  async fixtureJournal(payload: any) {
    const sha = await digest(encode(payload));
    const key = payload.binding.lot_incarnation + ":" + payload.batch_id;
    const receipt = {
      status: "committed",
      batch_id: payload.batch_id,
      digest: sha,
      cursor: payload.result,
    };
    this.ctx.storage.transactionSync(() => {
      this.ctx.storage.sql.exec(
        "INSERT INTO batches(batch_key,digest,receipt,bytes) VALUES(?,?,?,?)",
        key,
        sha,
        canonical(receipt),
        encode(payload).length,
      );
      this.ctx.storage.sql.exec(
        "INSERT INTO journal_chunks(batch_key,ordinal,payload) VALUES(?,0,?)",
        key,
        canonical(payload),
      );
    });
    return {
      lot_id: payload.result.lot_id,
      batch_key: key,
      digest: sha,
      tick: payload.result.completed_tick,
      ordinal: 0,
    };
  }
}
export default {
  async fetch(request: Request, env: any) {
    if (new URL(request.url).pathname.startsWith("/native-account/"))
      return accountHttp(request, env);
    try {
      const { target, op, args = [], crash } = (await request.json()) as any;
      const namespace = target === "account" ? env.ACCOUNTS : env.COORD;
      const stub = namespace.get(
        namespace.idFromName(
          target === "account" ? "purchase" : (args[0]?.result?.lot_id ?? "41"),
        ),
      );
      if (crash === "before") await fetch(env.CRASH_BARRIER);
      const value = await stub[op](...args);
      if (crash === "after") await fetch(env.CRASH_BARRIER);
      return Response.json(value);
    } catch (e) {
      return Response.json({ error: String(e) }, { status: 503 });
    }
  },
};
