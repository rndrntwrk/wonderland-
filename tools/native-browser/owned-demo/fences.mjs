// Shared by the loopback orchestration and its actual-workerd regression.
// The account sweeper wraps the immutable claim; that wrapper is not a fence.
export async function drainControlFences({
  account,
  lot,
  authority,
  lotId,
  onFence,
}) {
  if (!/^[0-9a-f]{64}$/.test(authority))
    throw Error("Invalid admission authority.");
  let after = "0";
  for (let page = 0; page < 128; page++) {
    const value = await account("pendingControlFences", [after]);
    if (!value || !Array.isArray(value.fences) || value.fences.length > 50)
      throw Error("Invalid account fence page.");
    for (const item of value.fences) {
      const claim = item.claim;
      if (
        !claim ||
        claim.state !== "fence_pending" ||
        typeof claim.lot_id !== "string"
      )
        throw Error("Invalid pending account claim.");
      if (claim.lot_id !== lotId) continue;
      await lot("recordControlFence", [
        {
          version: 1,
          authority,
          avatar_id: claim.avatar_id,
          generation: claim.generation,
          lot_id: claim.lot_id,
          request_digest: item.request_digest,
        },
      ]);
      await onFence?.(item);
    }
    if (value.next === null) return;
    if (
      typeof value.next !== "string" ||
      !/^[1-9][0-9]{0,7}$/.test(value.next) ||
      BigInt(value.next) <= BigInt(after)
    )
      throw Error("Invalid account fence cursor.");
    after = value.next;
  }
  throw Error("Fence scan exceeded its bound.");
}
