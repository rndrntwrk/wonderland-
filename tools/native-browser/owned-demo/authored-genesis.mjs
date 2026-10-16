// Private operator-authored setup. No browser input or credentials enter the hook.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { isAbsolute } from "node:path";
export const canonical = (v) =>
  v === null || typeof v !== "object"
    ? JSON.stringify(v)
    : Array.isArray(v)
      ? "[" + v.map(canonical).join(",") + "]"
      : "{" +
        Object.keys(v)
          .sort()
          .map((k) => JSON.stringify(k) + ":" + canonical(v[k]))
          .join(",") +
        "}";
const sha = (v) => createHash("sha256").update(v).digest("hex");
const freeze = (v) => {
  if (v && typeof v === "object") {
    Object.values(v).forEach(freeze);
    Object.freeze(v);
  }
  return v;
};
export function authoredOwnerReceipt(
  manifest,
  registered,
  login,
  created,
  owned,
  operation,
) {
  assert.equal(registered.account_id, login.account_id);
  assert.equal(owned.account_id, login.account_id);
  assert.equal(owned.avatar_id, created.avatar_id);
  assert.match(owned.account_id, /^[1-9][0-9]{0,19}$/);
  assert.match(owned.avatar_id, /^[1-9][0-9]{0,7}$/);
  assert.ok(BigInt(owned.avatar_id) <= 16777215n);
  assert.match(operation, /^[A-Za-z0-9_-]{16,96}$/);
  const { account_id, avatar_id, ...spec } = owned;
  const { avatar_id: _createdAvatar, ...createdSpec } = created;
  assert.deepEqual(spec, createdSpec);
  assert.equal(spec.release, manifest.content_release);
  return {
    schema: "wonderland.authored-object-owner-account.v1",
    authority_id: manifest.authority_id,
    content_release: manifest.content_release,
    protocol_release: manifest.protocol_release,
    account_id,
    avatar_id,
    avatar_spec_sha256: sha(canonical(spec)),
    source: "AccountAuthority.createAvatar",
    creation_operation: operation,
    canonical_create_result_sha256: sha(canonical(created)),
  };
}
export async function authoredGenesis(callback, manifest, ownerReceipt) {
  assert.equal(typeof callback, "function");
  const baseBytes = await readFile(manifest.host_config);
  const input = freeze(
    JSON.parse(
      JSON.stringify({
        manifest,
        manifest_sha256: sha(canonical(manifest)),
        base_host_config_sha256: sha(baseBytes),
        account_creation_receipt: ownerReceipt,
        account_creation_receipt_sha256: sha(canonical(ownerReceipt)),
      }),
    ),
  );
  const result = await callback(input);
  assert.deepEqual(Object.keys(result).sort(), [
    "host_config",
    "host_config_sha256",
    "ownership_receipt",
    "ownership_receipt_sha256",
  ]);
  for (const name of ["host_config", "ownership_receipt"]) {
    assert.ok(typeof result[name] === "string" && isAbsolute(result[name]));
    assert.match(result[name + "_sha256"], /^[0-9a-f]{64}$/);
  }
  const configBytes = await readFile(result.host_config),
    receiptBytes = await readFile(result.ownership_receipt);
  assert.equal(sha(configBytes), result.host_config_sha256);
  assert.equal(sha(receiptBytes), result.ownership_receipt_sha256);
  const config = JSON.parse(configBytes),
    receipt = JSON.parse(receiptBytes);
  assert.ok(
    typeof config.appearance_file === "string" &&
      config.appearance_file.length > 0,
  );
  assert.match(config.appearance_sha256, /^[0-9a-f]{64}$/);
  assert.equal(config.binding_sha256, manifest.content_release);
  assert.equal(config.lot_id, manifest.lot.lot_id);
  assert.equal(
    receipt.schema,
    "wonderland.authored-object-ownership-receipt.v1",
  );
  for (const name of ["authority_id", "content_release", "protocol_release"])
    assert.equal(receipt[name], manifest[name]);
  assert.equal(receipt.lot_id, manifest.lot.lot_id);
  assert.equal(receipt.base_manifest_sha256, input.manifest_sha256);
  assert.equal(receipt.base_host_config_sha256, input.base_host_config_sha256);
  assert.equal(
    receipt.account_creation_receipt_sha256,
    input.account_creation_receipt_sha256,
  );
  assert.equal(receipt.host_config, result.host_config);
  assert.equal(receipt.host_config_sha256, result.host_config_sha256);
  assert.equal(receipt.owner_account_id, ownerReceipt.account_id);
  assert.equal(receipt.owner_avatar_id, ownerReceipt.avatar_id);
  assert.equal(receipt.owned_actors, 0);
  assert.equal(receipt.avatar_roles_changed, false);
  return {
    manifest: { ...manifest, host_config: result.host_config },
    receipt: {
      ...result,
      base_manifest_sha256: input.manifest_sha256,
      account_creation_receipt: ownerReceipt,
    },
  };
}
