// Private loopback-runner driver. Never imported by a production entrypoint.
// Only the Node supervisor calls this Worker through Miniflare.dispatchFetch;
// the public loopback server does not proxy this RPC endpoint.
export { AccountAuthority } from "../../../services/native-edge/src/account/authority";
export { LotCoordinator } from "../../../services/native-edge/src/lot/coordinator";
import {
  accountHttp,
  type AccountHttpEnv,
} from "../../../services/native-edge/src/http/accounts";
interface Env extends AccountHttpEnv {
  ACCOUNT_REALM: string;
  PRIVATE_CAPABILITY: string;
  LOT_NAME: string;
  COORD: DurableObjectNamespace;
}
import { dispatchSupervisorRpc } from "../../../services/native-edge/src/hosted/supervisor-rpc";
export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    if (new URL(request.url).pathname.startsWith("/native-account/"))
      return accountHttp(request, env);
    if (
      request.url !== "http://owned-private.invalid/rpc" ||
      request.method !== "POST"
    )
      return new Response(null, { status: 404 });
    if (
      request.headers.has("origin") ||
      request.headers.get("content-type") !== "application/json" ||
      !env.PRIVATE_CAPABILITY ||
      request.headers.get("x-owned-private") !== env.PRIVATE_CAPABILITY
    )
      return new Response(null, { status: 403 });
    let body: unknown;
    try { body = await request.json(); }
    catch { return Response.json({error:"Invalid private JSON"},{status:400}); }
    return dispatchSupervisorRpc(body, async (target, method, args) => {
      const stub = target === "account"
        ? env.ACCOUNTS!.get(env.ACCOUNTS!.idFromName(env.ACCOUNT_REALM))
        : env.COORD.get(env.COORD.idFromName(env.LOT_NAME));
      const callable = stub as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>;
      return callable[method](...args);
    });
  },
};
