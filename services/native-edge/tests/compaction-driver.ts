// Local storage-contract/fault driver only. Never part of the shipped worker.
import { TestCoordinator as BaseCoordinator } from "./driver";
export class TestCoordinator extends BaseCoordinator {
  archiveGate: {
    mode: string;
    written: boolean;
    reads: number;
    waiting: boolean;
    release?: () => void;
  };
  constructor(ctx: DurableObjectState, env: any) {
    const gate = {
      mode: "",
      written: false,
      reads: 0,
      waiting: false,
      release: undefined as undefined | (() => void),
    };
    const bucket = new Proxy(env.CHECKPOINTS, {
      get(target, prop) {
        if (prop === "put")
          return async (key: string, ...args: any[]) => {
            if (key.includes("/journal/") && gate.mode === "put-failure")
              throw Error("injected archive put failure");
            const result = await target.put(key, ...args);
            if (key.includes("/journal/")) {
              gate.written = true;
              if (gate.mode === "crash-written") await fetch(env.CRASH_BARRIER);
            }
            return result;
          };
        if (prop === "get")
          return async (key: string, ...args: any[]) => {
            if (key.includes("/journal/")) gate.reads++;
            if (key.includes("/journal/") && (gate.written || gate.reads > 1)) {
              if (gate.mode === "readback-failure") {
                gate.mode = "";
                return null;
              }
              if (gate.mode === "pause-readback") {
                gate.mode = "";
                gate.waiting = true;
                await new Promise<void>((resolve) => {
                  gate.release = resolve;
                });
                gate.waiting = false;
              }
            }
            return target.get(key, ...args);
          };
        const value = target[prop];
        return typeof value === "function" ? value.bind(target) : value;
      },
    });
    super(ctx, { ...env, CHECKPOINTS: bucket });
    this.archiveGate = gate;
  }
  setArchiveFault(mode: string) {
    this.archiveGate.mode = mode;
    this.archiveGate.written = false;
    this.archiveGate.reads = 0;
    return true;
  }
  archivePaused() {
    return this.archiveGate.waiting;
  }
  resumeArchive() {
    this.archiveGate.release?.();
    return true;
  }
  archiveRows() {
    return this.ctx.storage.sql
      .exec(
        "SELECT p.*,a.sha256,a.bytes,a.checkpoint_key FROM archived_batches p JOIN journal_archives a ON a.object_key=p.object_key ORDER BY p.batch_key",
      )
      .toArray();
  }
  inspectOutbox() {
    return this.ctx.storage.sql
      .exec("SELECT batch_key,lane,digest FROM outbox ORDER BY batch_key,lane")
      .toArray();
  }
  rawCounts() {
    return {
      chunks: this.ctx.storage.sql
        .exec<{ count: number }>("SELECT COUNT(*) AS count FROM journal_chunks")
        .one().count,
      live: this.ctx.storage.sql
        .exec<{ count: number }>("SELECT COUNT(*) AS count FROM journal_live")
        .one().count,
      receipts: this.ctx.storage.sql
        .exec<{ count: number }>("SELECT COUNT(*) AS count FROM batches")
        .one().count,
      archives: this.ctx.storage.sql
        .exec<{
          count: number;
        }>("SELECT COUNT(*) AS count FROM journal_archives")
        .one().count,
    };
  }
  failIndex(key: string | null) {
    this.ctx.storage.sql.exec("DROP TRIGGER IF EXISTS test_archive_abort");
    if (key) {
      if (!/^[0-9]+:[0-9]+$/.test(key)) throw Error("invalid test key");
      this.ctx.storage.sql.exec(
        `CREATE TRIGGER test_archive_abort BEFORE INSERT ON archived_batches WHEN NEW.batch_key='${key}' BEGIN SELECT RAISE(ABORT,'injected archive index failure'); END`,
      );
    }
    return true;
  }
  legacyLayout() {
    this.ctx.storage.sql.exec(
      "DELETE FROM journal_layout; DELETE FROM journal_live",
    );
    return true;
  }
  async commitMany(proposals: unknown[], controlled: boolean) {
    let result;
    for (const p of proposals)
      result = controlled
        ? await this.commitControlledBatchV2(p)
        : await this.commitBatch(p);
    return result;
  }
}
export default {
  async fetch(request: Request, env: any) {
    try {
      const {
        op,
        args = [],
        lot = "compaction",
        crash,
      } = (await request.json()) as any;
      const stub = env.COORD.get(env.COORD.idFromName(lot));
      if (op === "publishCheckpoint" || op === "publishControlledCheckpointV2")
        args[3] = Uint8Array.from(args[3]);
      if (op === "finishRecovery" || op === "finishControlledRecoveryV2")
        args[1] = Uint8Array.from(args[1]);
      const allowed = new Set([
        "initialize",
        "initializeControlledV2",
        "inspect",
        "inspectControlledV2",
        "commitBatch",
        "commitControlledBatchV2",
        "commitMany",
        "getBatch",
        "readBatch",
        "publishCheckpoint",
        "publishControlledCheckpointV2",
        "listCheckpoints",
        "beginRecovery",
        "recoveryBundle",
        "finishRecovery",
        "beginControlledRecoveryV2",
        "controlledRecoveryBundleV2",
        "finishControlledRecoveryV2",
        "setArchiveFault",
        "archivePaused",
        "resumeArchive",
        "archiveRows",
        "inspectOutbox",
        "rawCounts",
        "failIndex",
        "legacyLayout",
        "recordControlFence",
        "pauseCheckpoint",
        "checkpointPaused",
        "resumeCheckpoint",
        "persistHostReceiptsV1",
        "lookupHostReceiptV1",
      ]);
      if (!allowed.has(op)) throw Error("unknown test operation");
      let result = await stub[op](...args);
      if (crash === "compacted") await fetch(env.CRASH_BARRIER);
      if (op === "recoveryBundle" || op === "controlledRecoveryBundleV2") {
        const { checkpoint_bytes, ...rest } = result;
        result = {
          ...rest,
          checkpoint_hex: Array.from(checkpoint_bytes as Uint8Array)
            .map((b) => b.toString(16).padStart(2, "0"))
            .join(""),
        };
      }
      return Response.json(result);
    } catch (error) {
      return Response.json({ error: String(error) }, { status: 409 });
    }
  },
};
