// Test-only workerd driver. Never bundled by src/index.ts.
import { TestAccounts } from "./account-driver";
import { LotCoordinator } from "../src/lot/coordinator";
export { TestAccounts };
export class TestOwnedLot extends LotCoordinator {
  constructor(ctx: DurableObjectState, env: any) {
    super(ctx, env);
    ctx.storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS test_clock(id INTEGER PRIMARY KEY,value INTEGER NOT NULL)",
    );
  }
  protected now() {
    return (
      this.ctx.storage.sql
        .exec<{ value: number }>("SELECT value FROM test_clock WHERE id=1")
        .toArray()[0]?.value ?? Date.now()
    );
  }
  advance(ms: number) {
    const now = this.now() + ms;
    // A separate test clock avoids modifying admitted production state.
    this.ctx.storage.sql.exec(
      "CREATE TABLE IF NOT EXISTS test_clock(id INTEGER PRIMARY KEY,value INTEGER NOT NULL)",
    );
    this.ctx.storage.sql.exec(
      "INSERT INTO test_clock(id,value) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET value=excluded.value",
      now,
    );
    return now;
  }
  clock() {
    return this.ctx.storage.sql
      .exec<{ value: number }>("SELECT value FROM test_clock WHERE id=1")
      .toArray()[0]?.value;
  }
}
const accountOps = new Set([
  "initializeCatalog",
  "seedAvatarSequence",
  "getReservedAvatarExportV2",
  "initializeAdmissionAuthorityV2",
  "register",
  "login",
  "createAvatar",
  "reserveAvatarControl",
  "exportReservedAvatarV2",
  "revalidateReservedAvatarV2",
  "renewAvatarControlV2",
  "revalidateControlLeaseV2",
  "logout",
  "advance",
  "pendingControlFences",
]);
const lotOps = new Set([
  "initializeControlledV2",
  "installReservedAvatarV2",
  "renewControlLeaseV2",
  "inspectControlledV2",
  "commitControlledBatchV2",
  "lookupPlayerActionV1",
  "commitBatch",
  "recordControlFence",
  "getAdmissionV2",
  "advance",
  "inspect",
  "getBatch",
  "readBatch",
  "publishControlledCheckpointV2",
  "beginControlledRecoveryV2",
  "controlledRecoveryBundleV2",
  "finishControlledRecoveryV2",
]);
export default {
  async fetch(request: Request, env: any) {
    try {
      const { target, op, args = [], name } = (await request.json()) as any;
      if (!(target === "account" ? accountOps : lotOps).has(op))
        throw Error("unknown private test operation");
      const namespace = target === "account" ? env.ACCOUNTS : env.COORD;
      const stub = namespace.get(
        namespace.idFromName(
          name ?? (target === "account" ? env.ACCOUNT_REALM : "owned-lot"),
        ),
      );
      if (op === "publishControlledCheckpointV2")
        args[3] = Uint8Array.from(args[3]);
      if (op === "finishControlledRecoveryV2")
        args[1] = Uint8Array.from(args[1]);
      const result = await stub[op](...args);
      if (op === "controlledRecoveryBundleV2") {
        const { checkpoint_bytes, ...rest } = result;
        return Response.json({
          ...rest,
          checkpoint_hex: Array.from(checkpoint_bytes as Uint8Array)
            .map((b) => b.toString(16).padStart(2, "0"))
            .join(""),
        });
      }
      return Response.json(result);
    } catch (error) {
      return Response.json({ error: String(error) }, { status: 409 });
    }
  },
};
