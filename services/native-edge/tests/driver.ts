// Local E2E driver only; never included in src/index.ts or deployment output.
import { LotCoordinator } from "../src/lot/coordinator";
// Fault injection exists only in this local driver, never in the shipped worker.
export class TestCoordinator extends LotCoordinator {
  gate: { armed: boolean; waiting: boolean; release?: () => void };
  constructor(ctx: DurableObjectState, env: any) {
    const gate = {
      armed: false,
      waiting: false,
      release: undefined as undefined | (() => void),
    };
    const bucket = new Proxy(env.CHECKPOINTS, {
      get(target, prop) {
        if (prop === "get")
          return async (...args: any[]) => {
            if (gate.armed) {
              gate.armed = false;
              gate.waiting = true;
              await new Promise<void>((resolve) => {
                gate.release = resolve;
              });
              gate.waiting = false;
            }
            return target.get(...args);
          };
        const value = target[prop];
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
    super(ctx, { CHECKPOINTS: bucket });
    this.gate = gate;
  }
  pauseCheckpoint() {
    this.gate.armed = true;
    return true;
  }
  checkpointPaused() {
    return this.gate.waiting;
  }
  resumeCheckpoint() {
    this.gate.release?.();
    return true;
  }

  faultCapacity(mode: string) {
    if (mode === "count")
      this.ctx.storage.sql.exec(
        `WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<4096) INSERT INTO batches(batch_key,digest,receipt,bytes) SELECT 'fault:'||x,'fault','{}',0 FROM n`,
      );
    else if (mode === "bytes")
      this.ctx.storage.sql.exec(
        "INSERT INTO batches(batch_key,digest,receipt,bytes) VALUES('fault:capacity','fault','{}',67108864)",
      );
    else throw Error("unknown fault mode");
    return this.inspect();
  }
}
export default {
  async fetch(request: Request, env: any) {
    try {
      const {
        op,
        args = [],
        lot = "declared-test-lot",
      } = (await request.json()) as any;
      const stub = env.COORD.get(env.COORD.idFromName(lot));
      let result;
      if (op === "beginRecovery") result = await stub.beginRecovery(...args);
      else if (op === "finishRecovery")
        result = await stub.finishRecovery(args[0], Uint8Array.from(args[1]));
      else if (op === "pauseCheckpoint") result = await stub.pauseCheckpoint();
      else if (op === "checkpointPaused")
        result = await stub.checkpointPaused();
      else if (op === "resumeCheckpoint")
        result = await stub.resumeCheckpoint();
      else if (op === "faultCapacity")
        result = await stub.faultCapacity(...args);
      else if (op === "initialize") result = await stub.initialize(...args);
      else if (op === "inspect") result = await stub.inspect();
      else if (op === "commitBatch") result = await stub.commitBatch(...args);
      else if (op === "getBatch") result = await stub.getBatch(...args);
      else if (op === "readBatch") result = await stub.readBatch(...args);
      else if (op === "publishCheckpoint")
        result = await stub.publishCheckpoint(
          args[0],
          args[1],
          args[2],
          Uint8Array.from(args[3]),
        );
      else if (op === "listCheckpoints") result = await stub.listCheckpoints();
      else throw Error("unknown test operation");
      return Response.json(result);
    } catch (error) {
      return Response.json({ error: String(error) }, { status: 409 });
    }
  },
};
