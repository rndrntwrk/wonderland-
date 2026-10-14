import { accountHttp, type AccountHttpEnv } from "./http/accounts";
export { LotCoordinator } from "./lot/coordinator";
// RPC is available only through explicitly provisioned service/DO bindings.
// No public initialization, commit, checkpoint or privileged command endpoint.
export default {
  fetch(request: Request, env: AccountHttpEnv) {
    return accountHttp(request, env);
  },
};

export { AccountAuthority } from "./account/authority";
