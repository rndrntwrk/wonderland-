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
const accountMethods = new Set([
  "initializeCatalog",
  "initializeAdmissionAuthorityV2",
  "register",
  "login",
  "logout",
  "roster",
  "createAvatar",
  "ownedAvatar",
  "ownedAssets",
  "reserveAvatarControl",
  "avatarControl",
  "exportReservedAvatarV2",
  "revalidateReservedAvatarV2",
  "pendingControlFences",
  "renewAvatarControlV2",
  "getReservedAvatarExportV2",
  "requestAvatarControlFenceV1",
  "applyCommittedAvatarLotSaveV1",
  "releaseSavedAvatarControlV1",
  "lookupSavedAvatarControlV1",
  "lookupReleasedAvatarControlV1",
]);
const lotMethods = new Set([
  "initializeControlledV2",
  "controlStateV2",
  "currentNativeEodV1",
  "readOwnedEventLanesV1",
  "retireOwnedEventLaneV1",
  "installReservedAvatarV2",
  "commitControlledBatchV2",
  "recordControlFence",
  "getBatch",
  "inspectControlledV2",
  "getAdmissionV2",
  "publishControlledCheckpointV2",
  "persistHostReceiptsV1",
  "lookupHostReceiptV1",
  "lookupPlayerActionV1",
  "beginControlledRecoveryV2",
  "controlledRecoveryBundleV2",
  "finishControlledRecoveryV2",
  "renewControlLeaseV2",
  "beginOwnedDepartureV1",
  "lookupOwnedDepartureV1",
  "activeOwnedDeparturesV1",
  "findOwnedDepartureV1",
  "pendingOwnedDeparturesV1",
  "commitOwnedDrainV1",
  "lookupCommittedAvatarSaveV1",
  "lookupCommittedAvatarDetachV1",
  "retireReleasedAvatarControlV1",
]);
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
    try {
      const body = (await request.json()) as {
        target: string;
        method: string;
        args: unknown[];
      };
      const allow =
        body.target === "account"
          ? accountMethods
          : body.target === "lot"
            ? lotMethods
            : null;
      if (
        !allow?.has(body.method) ||
        !Array.isArray(body.args) ||
        body.args.length > 8
      )
        throw Error("Unsupported private call");
      const stub =
        body.target === "account"
          ? env.ACCOUNTS!.get(env.ACCOUNTS!.idFromName(env.ACCOUNT_REALM))
          : env.COORD.get(env.COORD.idFromName(env.LOT_NAME));
      const callable = stub as unknown as Record<
        string,
        (...args: unknown[]) => Promise<unknown>
      >;
      // JSON transport has an explicit, bounded byte representation. Never let
      // arbitrary browser objects become trusted checkpoint bytes.
      const byteIndex =
        body.method === "publishControlledCheckpointV2"
          ? 3
          : body.method === "finishControlledRecoveryV2"
            ? 1
            : null;
      if (byteIndex !== null) {
        const hex = body.args[byteIndex];
        if (
          typeof hex !== "string" ||
          hex.length > 16 * 1024 * 1024 ||
          !/^(?:[0-9a-f]{2})+$/.test(hex)
        )
          throw Error("Invalid checkpoint bytes");
        body.args[byteIndex] = Uint8Array.from(hex.match(/../g)!, (b) =>
          parseInt(b, 16),
        );
      }
      const value = await callable[body.method](...body.args);
      if (body.method === "controlledRecoveryBundleV2") {
        const bundle = value as Record<string, unknown>;
        if (!(bundle.checkpoint_bytes instanceof Uint8Array))
          throw Error("Invalid recovery checkpoint");
        const checkpoint_hex = Array.from(bundle.checkpoint_bytes, (b) =>
          b.toString(16).padStart(2, "0"),
        ).join("");
        const { checkpoint_bytes, ...metadata } = bundle;
        return Response.json({ value: { ...metadata, checkpoint_hex } });
      }
      return Response.json({ value });
    } catch (error) {
      const missing =
        error instanceof Error &&
        error.message === "owned admission export unavailable";
      return Response.json(
        {
          error: String(error),
          ...(missing ? { code: "owned_export_unavailable" } : {}),
        },
        { status: 400 },
      );
    }
  },
};
