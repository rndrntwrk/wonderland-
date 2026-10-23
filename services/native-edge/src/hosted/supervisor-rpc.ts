// Private supervisor-to-Durable-Object dispatch. This module has no public HTTP
// entrypoint. Call only after the loopback capability guard or the hosted
// container's authenticated outbound boundary; a browser must never reach it.
export type SupervisorDispatch = (
  target: "account" | "lot", method: string, args: unknown[]
) => Promise<unknown>;
const accountMethods = new Set([
  "initializeCatalog",
  "initializeAdmissionAuthorityV2",
  "register",
  "login",
  "logout",
  "roster",
  "createAvatar",
  "creationOptions",
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
  "ownedLotChatV1",
  "inspectControlledStartupV1",
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

export async function dispatchSupervisorRpc(input: unknown, dispatch: SupervisorDispatch): Promise<Response> {
  try {
    if (input === null || typeof input !== "object" || Array.isArray(input))
      throw Error("Unsupported private call");
    const body = input as Record<string, unknown>;
    const target = body.target;
    const method = body.method;
    const allow = target === "account" ? accountMethods : target === "lot" ? lotMethods : null;
    if (typeof method !== "string" || !allow?.has(method) || !Array.isArray(body.args) || body.args.length > 8)
      throw Error("Unsupported private call");
    const args = body.args.slice();
    const byteIndex = method === "publishControlledCheckpointV2" ? 3 : method === "finishControlledRecoveryV2" ? 1 : null;
    if (byteIndex !== null) {
      const hex = args[byteIndex];
      if (typeof hex !== "string" || hex.length > 16 * 1024 * 1024 || !/^(?:[0-9a-f]{2})+$/.test(hex))
        throw Error("Invalid checkpoint bytes");
      args[byteIndex] = Uint8Array.from(hex.match(/../g)!, b => parseInt(b, 16));
    }
    const value = await dispatch(target as "account" | "lot", method, args);
    if (method === "controlledRecoveryBundleV2") {
      if (value === null || typeof value !== "object") throw Error("Invalid recovery checkpoint");
      const bundle = value as Record<string, unknown>;
      if (!(bundle.checkpoint_bytes instanceof Uint8Array)) throw Error("Invalid recovery checkpoint");
      const checkpoint_hex = Array.from(bundle.checkpoint_bytes, b => b.toString(16).padStart(2, "0")).join("");
      const { checkpoint_bytes, ...metadata } = bundle;
      return Response.json({ value: { ...metadata, checkpoint_hex } });
    }
    return Response.json({ value });
  } catch (error) {
    const missing = error instanceof Error && error.message === "owned admission export unavailable";
    return Response.json({ error: String(error), ...(missing ? { code: "owned_export_unavailable" } : {}) }, { status: 400 });
  }
}
