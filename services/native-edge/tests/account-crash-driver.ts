// Only this test entry point exposes fault barriers; production exports no fetch routes.
import driver from "./account-driver";
export { TestAccounts } from "./account-driver";
export default {
  async fetch(request: Request, env: any) {
    const body = (await request.clone().json()) as {
      op?: string;
      crash?: string;
    };
    if (body.op === "createAvatar" && body.crash === "before")
      await fetch(env.CRASH_BARRIER);
    const response = await driver.fetch(request, env);
    if (response.ok && body.op === "createAvatar" && body.crash === "after")
      await fetch(env.CRASH_BARRIER);
    return response;
  },
};
