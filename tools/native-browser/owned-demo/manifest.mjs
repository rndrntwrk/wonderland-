import assert from "node:assert/strict";
const digest = (value) =>
  typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
const decimal = (value) =>
  typeof value === "string" &&
  /^[1-9][0-9]{0,19}$/.test(value) &&
  BigInt(value) <= 18446744073709551615n;
export function validateOwnedManifest(manifest) {
  assert.ok(
    manifest &&
      digest(manifest.authority_id) &&
      digest(manifest.content_release) &&
      digest(manifest.protocol_release),
  );
  assert.ok(manifest.lot && decimal(manifest.lot.lot_id));
  assert.ok(
    Number.isInteger(manifest.lot.lot_location) &&
      manifest.lot.lot_location > 0 &&
      manifest.lot.lot_location <= 4294967295,
  );
  // Database lot ID and packed source world location are independent identities.
  assert.ok(
    typeof manifest.lot.name === "string" &&
      manifest.lot.name.length > 0 &&
      manifest.lot.name.length <= 96,
  );
  assert.equal(manifest.catalog?.release, manifest.content_release);
  assert.ok(
    Array.isArray(manifest.avatar_specs) && manifest.avatar_specs.length === 2,
  );
  for (const spec of manifest.avatar_specs)
    assert.equal(spec.release, manifest.content_release);
  assert.ok(
    typeof manifest.host_config === "string" && manifest.host_config.length > 0,
  );
  return manifest;
}
