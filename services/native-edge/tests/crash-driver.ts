// Isolated process-crash driver. Never exported from the production worker.
import driver from "./driver";
import { TestCoordinator as BaseCoordinator } from "./driver";
export class TestCoordinator extends BaseCoordinator {
  inspectOutbox() {
    return this.ctx.storage.sql
      .exec("SELECT batch_key,lane,digest FROM outbox ORDER BY batch_key,lane")
      .toArray();
  }
}
export default {
  async fetch(request: Request, env: any) {
    const input = (await request.clone().json()) as any;
    if (input.op === "inspectOutbox") {
      const stub = env.COORD.get(env.COORD.idFromName("declared-test-lot"));
      return Response.json(await stub.inspectOutbox());
    }
    if (input.crash === "before") await fetch(env.CRASH_BARRIER);
    const response = await driver.fetch(request, env);
    if (input.crash === "after") {
      if (!response.ok) throw Error("commit failed before crash boundary");
      await fetch(env.CRASH_BARRIER);
    }
    return response;
  },
};
