import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import {
  authoredOwnerReceipt,
  authoredGenesis,
  canonical,
} from "./authored-genesis.mjs";
const sha = (v) => createHash("sha256").update(v).digest("hex");
const manifest = {
  authority_id: "a".repeat(64),
  content_release: "b".repeat(64),
  protocol_release: "c".repeat(64),
  lot: { lot_id: "11" },
  catalog: { release: "b".repeat(64) },
  avatar_specs: [{ name: "Authored" }],
};
const created = {
  avatar_id: "7",
  name: "Normalized",
  release: manifest.content_release,
  body_key: "123",
  head_key: "456",
};
const owned = { account_id: "9", ...created };
const makeOwner = () =>
  authoredOwnerReceipt(
    manifest,
    { account_id: "9" },
    { account_id: "9", token: "never passed" },
    created,
    owned,
    "owned_demo_create_1",
  );
test("authored owner derives from real normalized owned/create result and excludes credentials", () => {
  const receipt = makeOwner();
  assert.equal(receipt.account_id, "9");
  assert.equal(receipt.avatar_id, "7");
  assert.equal(receipt.canonical_create_result_sha256, sha(canonical(created)));
  assert.equal(JSON.stringify(receipt).includes("never passed"), false);
  for (const change of [
    { account_id: "10" },
    { avatar_id: "8" },
    { name: "Different" },
  ])
    assert.throws(() =>
      authoredOwnerReceipt(
        manifest,
        { account_id: "9" },
        { account_id: "9" },
        created,
        { ...owned, ...change },
        "owned_demo_create_1",
      ),
    );
});
test("authored hook freezes only bounded nonsecret input and changes host path alone", async () => {
  const dir = await mkdtemp(join(tmpdir(), "authored-hook-"));
  try {
    const base = join(dir, "base.json"),
      host = join(dir, "host.json"),
      receiptPath = join(dir, "receipt.json");
    await writeFile(base, "{}");
    const original = { ...manifest, host_config: base };
    const config = JSON.stringify({
      binding_sha256: manifest.content_release,
      lot_id: manifest.lot.lot_id,
      appearance_file: join(dir, "world.json"),
      appearance_sha256: "a".repeat(64),
    });
    await writeFile(host, config);
    let mutation;
    const callback = async (input) => {
      assert.ok(Object.isFrozen(input.manifest.lot));
      assert.throws(() => {
        input.manifest.lot.lot_id = "12";
      });
      assert.deepEqual(Object.keys(input).sort(), [
        "account_creation_receipt",
        "account_creation_receipt_sha256",
        "base_host_config_sha256",
        "manifest",
        "manifest_sha256",
      ]);
      const receipt = {
        schema: "wonderland.authored-object-ownership-receipt.v1",
        authority_id: manifest.authority_id,
        content_release: manifest.content_release,
        protocol_release: manifest.protocol_release,
        lot_id: manifest.lot.lot_id,
        base_manifest_sha256: input.manifest_sha256,
        base_host_config_sha256: input.base_host_config_sha256,
        account_creation_receipt_sha256: input.account_creation_receipt_sha256,
        host_config: host,
        host_config_sha256: sha(config),
        owner_account_id: "9",
        owner_avatar_id: "7",
        owned_actors: 0,
        avatar_roles_changed: false,
        ...mutation,
      };
      const bytes = JSON.stringify(receipt);
      await writeFile(receiptPath, bytes);
      return {
        host_config: host,
        host_config_sha256: sha(config),
        ownership_receipt: receiptPath,
        ownership_receipt_sha256: sha(bytes),
      };
    };
    const result = await authoredGenesis(callback, original, makeOwner());
    assert.deepEqual(result.manifest, { ...original, host_config: host });
    assert.equal(original.host_config, base);
    for (mutation of [
      { authority_id: "f".repeat(64) },
      { content_release: "f".repeat(64) },
      { protocol_release: "f".repeat(64) },
      { lot_id: "12" },
      { base_manifest_sha256: "f".repeat(64) },
      { owner_account_id: "10" },
      { owner_avatar_id: "8" },
      { owned_actors: 1 },
      { avatar_roles_changed: true },
      { base_host_config_sha256: "d".repeat(64) },
      { account_creation_receipt_sha256: "e".repeat(64) },
    ])
      await assert.rejects(authoredGenesis(callback, original, makeOwner()));
    mutation = undefined;
    await assert.rejects(
      authoredGenesis(
        async (input) => ({
          ...(await callback(input)),
          authority_id: "f".repeat(64),
        }),
        original,
        makeOwner(),
      ),
    );
    await assert.rejects(
      authoredGenesis(
        async (input) => ({
          ...(await callback(input)),
          host_config_sha256: "f".repeat(64),
        }),
        original,
        makeOwner(),
      ),
    );
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
