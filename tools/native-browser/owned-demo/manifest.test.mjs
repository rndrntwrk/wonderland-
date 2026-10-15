import assert from "node:assert/strict";
import { test } from "node:test";
import { validateOwnedManifest } from "./manifest.mjs";
const manifest = () => ({
  authority_id: "a".repeat(64),
  content_release: "b".repeat(64),
  protocol_release: "c".repeat(64),
  lot: {
    lot_id: "41",
    lot_location: 16777472,
    name: "Distinct source location",
  },
  catalog: { release: "b".repeat(64) },
  avatar_specs: [{ release: "b".repeat(64) }, { release: "b".repeat(64) }],
  host_config: "/private/pinned-host.json",
});
test("database lot 41 and packed source location 16777472 remain distinct", () => {
  const value = manifest();
  assert.equal(validateOwnedManifest(value), value);
  assert.notEqual(value.lot.lot_id, String(value.lot.lot_location));
});
test("database and packed location bounds are validated independently", () => {
  for (const [key, value] of [
    ["lot_id", "0"],
    ["lot_id", "01"],
    ["lot_id", "18446744073709551616"],
    ["lot_location", 0],
    ["lot_location", 4294967296],
    ["lot_location", "16777472"],
  ]) {
    const input = manifest();
    input.lot[key] = value;
    assert.throws(() => validateOwnedManifest(input));
  }
});
