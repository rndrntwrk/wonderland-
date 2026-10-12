export { LotCoordinator } from "./lot/coordinator";
// RPC is available only through explicitly provisioned service/DO bindings.
// No public initialization, commit, checkpoint or privileged command endpoint.
export default {
  fetch() {
    return new Response("Not found", { status: 404 });
  },
};
