// Caller has already reserved this exact operation through the authenticated
// account API. On reply-loss retry, reuse its frozen policy binding unchanged.
export async function loadOrCreateExport({
  account,
  token,
  operation,
  avatarId,
  permissionBinding,
}) {
  try {
    return await account("getReservedAvatarExportV2", [
      token,
      operation,
      avatarId,
    ]);
  } catch (error) {
    if (error.code !== "owned_export_unavailable") throw error;
  }
  return account("exportReservedAvatarV2", [
    token,
    operation,
    avatarId,
    permissionBinding,
  ]);
}
