// Same-cloud, loopback-only integration runner. Requires a pinned source lot and
// content release. No fixed actors, generic Spawn or scripted success fallback.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { readFile, writeFile, mkdtemp, mkdir, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join, dirname, extname, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { randomBytes, createHash } from "node:crypto";
import { openOwnedHost } from "./owned-demo/process.mjs";
import { drainControlFences } from "./owned-demo/fences.mjs";
import { settlePlayerAction } from "./owned-demo/actions.mjs";
import { reconcileAuthenticatedPlayerAction } from "./owned-demo/action-results.mjs";
import { validateOwnedManifest } from "./owned-demo/manifest.mjs";
import { stageRecovery } from "./owned-demo/recovery.mjs";
import { loadOrCreateExport } from "./owned-demo/admission.mjs";
import {
  askWithDurableReceipts,
  retireCheckpointReceipts,
} from "./owned-demo/receipts.mjs";
const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const require = createRequire(join(repo, "services/native-edge/package.json"));
const { Miniflare } = require("miniflare"),
  { build } = require("esbuild"),
  { WebSocketServer } = require("ws");
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const digest = (value) =>
  typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
const dec = (value) =>
  typeof value === "string" &&
  /^[1-9][0-9]{0,19}$/.test(value) &&
  BigInt(value) <= 18446744073709551615n;
const packet = (value) => {
  assert.equal(typeof value, "string");
  assert.ok(
    value.length > 0 &&
      value.length <= 160 * 1024 * 1024 &&
      /^(?:[0-9a-f]{2})+$/.test(value),
  );
  return Buffer.from(value, "hex");
};

/** manifest identifies actual source files/catalogue/appearance. It is trusted
 * operator setup, never a browser request. All actor IDs come from real accounts
 * and all actors are created by the source-native Rust admission transition. */
export async function createOwnedDemo({
  dist,
  manifest,
  hostExecutable = resolve(repo, "target/debug/wonderland-native-lot-host"),
  storageRoot,
  port = 0,
  resume = false,
}) {
  validateOwnedManifest(manifest);
  assert.ok(
    !resume || storageRoot,
    "Ingress recovery requires the original durable storage root.",
  );
  const root = storageRoot
    ? resolve(storageRoot)
    : await mkdtemp(join(tmpdir(), "wonderland-owned-"));
  await mkdir(root, { recursive: true });
  const web = resolve(dist),
    hostConfig = resolve(manifest.host_config);
  if (resume) {
    const prior = JSON.parse(await readFile(join(root, "runner.json"), "utf8"));
    assert.equal(prior.version, 1);
    assert.equal(prior.manifest_sha256, sha(JSON.stringify(manifest)));
    const previous = new URL(prior.origin);
    assert.equal(previous.hostname, "127.0.0.1");
    assert.equal(previous.protocol, "http:");
    if (port) assert.equal(port, Number(previous.port));
    else port = Number(previous.port);
  }
  const sockets = new Set(),
    tickets = new Map(),
    admitted = new Map(),
    pendingAdmissions = new Map(),
    privateCapability = randomBytes(32).toString("hex");
  let host,
    mf,
    server,
    wss,
    origin,
    timer,
    closing = false,
    halted = null,
    lastCheckpointTick = 0n,
    batch = 0n,
    serial = Promise.resolve(),
    queued = 0,
    ticking = false,
    accounts = null;
  const serially = (fn, { allowPaused = false } = {}) => {
    if (closing) return Promise.reject(Error("Demo closed."));
    if (queued >= 16)
      return Promise.reject(Error("Demo request queue is full."));
    queued++;
    const task = serial.then(() => {
      if (halted && !allowPaused) throw Error("Durable host is paused.");
      return fn();
    });
    serial = task.catch(() => {}).finally(() => queued--);
    return task;
  };
  const rpc = async (target, method, args = []) => {
    const response = await mf.dispatchFetch(
      "http://owned-private.invalid/rpc",
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "x-owned-private": privateCapability,
        },
        body: JSON.stringify({ target, method, args }),
      },
    );
    const value = await response.json();
    if (!response.ok)
      throw Object.assign(
        Error(value.error ?? "Private service unavailable."),
        { code: value.code },
      );
    return value.value;
  };
  const account = (method, args) => rpc("account", method, args),
    lot = (method, args) => rpc("lot", method, args);
  const send = (socket, bytes) => {
    if (socket.readyState !== 1) return;
    if (socket.bufferedAmount + bytes.length > 82 * 1024 * 1024) {
      socket.close(1013);
      return;
    }
    socket.send(bytes, { binary: true });
  };
  const broadcast = (hex) => {
    const bytes = packet(hex);
    for (const socket of sockets) if (socket.live) send(socket, bytes);
  };
  const stop = (error) => {
    halted = String(error.message ?? error);
    for (const socket of sockets) socket.close(1012);
  };
  async function commit(proposal) {
    if (proposal.kind !== "proposal")
      throw Error(proposal.reason ?? "Native transition was refused.");
    let receipt;
    try {
      receipt = await lot("commitControlledBatchV2", [proposal]);
    } catch (error) {
      // A missing reply is not proof of rejection. Query the exact durable ID.
      receipt = await lot("getBatch", [
        proposal.payload.binding.lot_incarnation,
        proposal.payload.batch_id,
      ]);
      if (!["committed", "rejected"].includes(receipt.status)) {
        stop(error);
        throw Error("Durable decision is unknown; publication is paused.");
      }
    }
    if (
      !["committed", "rejected"].includes(receipt.status) ||
      receipt.digest !== proposal.digest
    ) {
      stop(Error("Durable receipt mismatch."));
      throw Error("Durable receipt mismatch.");
    }
    if (receipt.status === "rejected") {
      const result = await askWithDurableReceipts(host, lot, {
        kind: "resolve",
        batch_id: proposal.payload.batch_id,
        digest: proposal.digest,
        decision: { status: "rejected" },
      });
      if (result.kind !== "rejected" || result.digest !== proposal.digest) {
        stop(Error("Host rejection mismatch."));
        throw Error("Host rejection mismatch.");
      }
      if ((await currentHost()).retained_receipts >= 32)
        await publishCheckpoint();
      return { ...result, reason: receipt.reason };
    }
    const result = await askWithDurableReceipts(host, lot, {
      kind: "resolve",
      batch_id: proposal.payload.batch_id,
      digest: proposal.digest,
      decision: { status: "committed", cursor: proposal.payload.result },
    });
    if (result.kind !== "committed" || result.digest !== proposal.digest) {
      stop(Error("Host resolution mismatch."));
      throw Error("Host resolution mismatch.");
    }
    broadcast(result.ticks_hex);
    if (BigInt(result.cursor.completed_tick) - lastCheckpointTick >= 32n)
      await publishCheckpoint();
    return result;
  }
  const recoveryControl = (durable) => ({
    control_revision: durable.control_revision,
    actor_registry_sha256: durable.actor_registry_sha256,
    actors: durable.actors,
    permission_binding: durable.permission_binding,
  });
  function matchControlled(runtime, durable) {
    assert.deepEqual(runtime.cursor, durable.cursor);
    assert.equal(runtime.actor_registry_sha256, durable.actor_registry_sha256);
    assert.deepEqual(runtime.actors, durable.actors);
    assert.deepEqual(runtime.permission_binding, durable.permission_binding);
    assert.equal(runtime.cursor.lot_id, manifest.lot.lot_id);
    assert.equal(runtime.source_lot_location, manifest.lot.lot_location);
  }
  async function publishCheckpoint() {
    const runtime = await currentHost(),
      durable = await lot("inspectControlledV2");
    matchControlled(runtime, durable);
    const published = await lot("publishControlledCheckpointV2", [
      runtime.binding,
      runtime.cursor,
      sha(packet(runtime.checkpoint_hex)),
      runtime.checkpoint_hex,
      recoveryControl(durable),
    ]);
    await retireCheckpointReceipts(host, lot, published);
    lastCheckpointTick = BigInt(runtime.cursor.completed_tick);
  }
  async function recoverHost() {
    // No snapshot is taken here. Process loss must be recoverable from the
    // checkpoint already retained in R2 plus the accepted durable journal tail.
    await sweepFences();
    const before = await lot("inspectControlledV2");
    for (const socket of sockets) socket.close(1012);
    tickets.clear();
    await host.close("SIGKILL");
    try {
      const recovery = await lot("beginControlledRecoveryV2", [
        before.binding.lot_incarnation,
      ]);
      const bundle = await lot("controlledRecoveryBundleV2", [
        before.binding.lot_incarnation,
      ]);
      assert.equal(bundle.schema_version, 2);
      assert.deepEqual(bundle.grant, recovery.grant);
      assert.deepEqual(bundle.control, recovery.control);
      assert.ok(bundle.batches.length <= 64);
      assert.equal(
        sha(packet(bundle.checkpoint_hex)),
        bundle.checkpoint.sha256,
      );
      const configPath = await stageRecovery(
        root,
        bundle,
        JSON.parse(await readFile(hostConfig, "utf8")),
      );
      const replacement = await openOwnedHost(hostExecutable, configPath);
      host = replacement;
      const candidate = host.initial;
      assert.equal(candidate.kind, "recovery_candidate");
      assert.equal(candidate.recovery_pending, true);
      assert.deepEqual(candidate.actors, bundle.control.actors);
      assert.equal(
        candidate.actor_registry_sha256,
        bundle.control.actor_registry_sha256,
      );
      assert.deepEqual(
        candidate.permission_binding,
        bundle.control.permission_binding,
      );
      assert.deepEqual(candidate.takeover_marker.grant, bundle.grant);
      assert.equal(
        candidate.takeover_marker.checkpoint_sha256,
        sha(packet(candidate.checkpoint_hex)),
      );
      const finished = await lot("finishControlledRecoveryV2", [
        {
          schema_version: 2,
          marker: candidate.takeover_marker,
          control: bundle.control,
        },
        candidate.checkpoint_hex,
      ]);
      assert.equal(finished.schema_version, 2);
      assert.deepEqual(finished.marker, candidate.takeover_marker);
      assert.deepEqual(finished.control, bundle.control);
      const activated = await host.ask({
        kind: "activate_recovery",
        marker: finished.marker,
      });
      assert.equal(activated.kind, "committed_state");
      assert.equal(activated.recovery_pending, false);
      const durable = await lot("inspectControlledV2");
      matchControlled(activated, durable);
      for (const entry of admitted.values())
        entry.binding = {
          ...entry.binding,
          lot_incarnation: activated.binding.lot_incarnation,
        };
      lastCheckpointTick = BigInt(activated.cursor.completed_tick);
      halted = null;
      return {
        cursor: durable.cursor,
        actors: durable.actors,
        actor_registry_sha256: durable.actor_registry_sha256,
      };
    } catch (error) {
      stop(error);
      throw error;
    }
  }
  async function currentHost() {
    const current = await host.ask({ kind: "inspect" });
    if (current.kind !== "committed_state" || current.recovery_pending)
      throw Error("Native host is not committed.");
    return current;
  }
  async function prepare(kind, extra = {}) {
    const current = await currentHost();
    // Large terminal lanes can fill memory well before 32 semantic ticks.
    // Transfer their exact receipts before preparing another speculative batch.
    if (
      current.retained_receipts >= 32 ||
      current.retained_receipt_bytes >= 8 * 1024 * 1024
    )
      await publishCheckpoint();
    const control = await lot("controlStateV2");
    return askWithDurableReceipts(host, lot, {
      kind,
      batch_id: String(++batch),
      binding: current.binding,
      predecessor: current.cursor,
      expected_control_revision: control.revision,
      ...extra,
    });
  }
  async function revalidate(entry) {
    const validated = await account("revalidateReservedAvatarV2", [
      entry.export,
    ]);
    const current = await currentHost(),
      durable = await lot("inspectControlledV2");
    assert.deepEqual(current.cursor, durable.cursor);
    assert.equal(current.actor_registry_sha256, durable.actor_registry_sha256);
    const control = durable.controls.find(
      (c) =>
        c.avatar_id === entry.export.avatar_id &&
        c.control_generation === entry.export.control_generation &&
        c.session_ref === entry.export.session_ref,
    );
    if (
      !control ||
      control.state !== "active" ||
      control.expires_at <= Date.now()
    )
      throw Error("Owned control is fenced or expired.");
    entry.leaseExpiresAt = Math.min(
      validated.lease.expires_at,
      control.expires_at,
    );
    const actor = current.actors?.find(
      (a) =>
        a.avatar_id === entry.export.avatar_id &&
        a.principal === entry.control.principal &&
        a.control_generation === entry.export.control_generation &&
        a.session_ref === entry.export.session_ref,
    );
    if (!actor) throw Error("Owned actor is no longer admitted.");
    return current;
  }
  async function renewActivity(entry, token = entry.token, mode = "activity") {
    if (!token) throw Error("Owned session is revoked.");
    if (!entry.renewalOperation && entry.leaseExpiresAt - Date.now() >= 30000)
      return;
    entry.renewalOperation ??= { id: randomBytes(24).toString("hex"), mode };
    // Retry identity survives an ambiguous private response. No autonomous tick
    // or checkpoint read calls this authenticated user-activity renewal.
    const receipt = await account("renewAvatarControlV2", [
      token,
      entry.export,
      entry.renewalOperation.id,
      entry.renewalOperation.mode,
    ]);
    const installed = await lot("renewControlLeaseV2", [receipt]);
    if (installed.status !== "renewed")
      throw Error("Owned activity lease was fenced.");
    entry.leaseExpiresAt = installed.expires_at;
    entry.renewalOperation = null;
  }
  const key = (token, avatar) => sha(token) + ":" + avatar;
  function ticket(entry) {
    for (const [id, value] of tickets)
      if (value.expires < Date.now()) tickets.delete(id);
    if (tickets.size >= 64) throw Error("Admission ticket capacity reached.");
    const id = randomBytes(24).toString("hex");
    tickets.set(id, { entry, expires: Date.now() + 15000 });
    return id;
  }
  async function restoreEntry(token, data) {
    assert.equal(data.lot_id, manifest.lot.lot_id);
    assert.ok(dec(data.avatar_id) && BigInt(data.avatar_id) <= 16777215n);
    assert.match(data.operation, /^[a-zA-Z0-9_-]{16,96}$/);
    const exported = await account("getReservedAvatarExportV2", [
      token,
      data.operation,
      data.avatar_id,
    ]);
    assert.equal(exported.lot_id, manifest.lot.lot_id);
    assert.equal(exported.authority_id, manifest.authority_id);
    assert.equal(exported.content_release, manifest.content_release);
    assert.equal(exported.protocol_release, manifest.protocol_release);
    const admission = await lot("getAdmissionV2", [exported]);
    if (
      admission.status !== "active" ||
      admission.receipt?.status !== "committed"
    )
      throw Error("Owned admission was not durably committed.");
    const current = await currentHost(),
      principal = admission.receipt.actor.principal;
    const actor = current.actors?.find(
      (a) =>
        a.avatar_id === data.avatar_id &&
        a.principal === principal &&
        a.control_generation === exported.control_generation &&
        a.session_ref === exported.session_ref &&
        a.export_digest === exported.export_digest,
    );
    assert.deepEqual(actor, admission.receipt.actor);
    const entry = {
      token,
      leaseExpiresAt: 0,
      renewalOperation: null,
      export: exported,
      operation: data.operation,
      control: {
        principal,
        avatar_id: data.avatar_id,
        control_generation: exported.control_generation,
        session_ref: exported.session_ref,
      },
      binding: {
        source_epoch: exported.control_generation,
        lot_incarnation: current.binding.lot_incarnation,
        lot_location: manifest.lot.lot_location,
        avatar_id: Number(data.avatar_id),
      },
      receipt: admission.receipt,
    };
    await revalidate(entry);
    admitted.set(key(token, data.avatar_id), entry);
    return entry;
  }
  async function enter(token, data) {
    await sweepFences();
    assert.deepEqual(Object.keys(data).sort(), [
      "avatar_id",
      "lot_id",
      "operation",
    ]);
    assert.equal(data.lot_id, manifest.lot.lot_id);
    assert.ok(dec(data.avatar_id) && BigInt(data.avatar_id) <= 16777215n);
    assert.match(data.operation, /^[a-zA-Z0-9_-]{16,96}$/);
    const previous = admitted.get(key(token, data.avatar_id));
    if (previous) {
      assert.equal(previous.operation, data.operation);
      await revalidate(previous);
      return {
        version: 1,
        state: "lot_ready",
        binding: previous.binding,
        ticket: ticket(previous),
      };
    }
    let candidate = pendingAdmissions.get(key(token, data.avatar_id));
    if (candidate) assert.equal(candidate.operation, data.operation);
    else {
      if (pendingAdmissions.size >= 256)
        throw Error("Owned admission capacity reached.");
      const current = await currentHost();
      candidate = {
        operation: data.operation,
        permission_binding: current.permission_binding,
        export: null,
      };
      pendingAdmissions.set(key(token, data.avatar_id), candidate);
    }
    await account("reserveAvatarControl", [
      token,
      data.operation,
      {
        avatar_id: data.avatar_id,
        lot_id: data.lot_id,
        content_release: manifest.content_release,
        protocol_release: manifest.protocol_release,
      },
    ]);
    candidate.export ??= await loadOrCreateExport({
      account,
      token,
      operation: data.operation,
      avatarId: data.avatar_id,
      permissionBinding: candidate.permission_binding,
    });
    const exported = candidate.export;
    const prior = await lot("getAdmissionV2", [exported]);
    let principal, result, leaseExpiresAt;
    if (prior.status === "active" && prior.receipt?.status === "committed") {
      principal = prior.receipt.actor.principal;
      result = prior.receipt;
      leaseExpiresAt = (await account("revalidateReservedAvatarV2", [exported]))
        .lease.expires_at;
    } else {
      const installed = await lot("installReservedAvatarV2", [exported]);
      if (installed.status !== "installed")
        throw Error("Owned control could not be installed.");
      principal = installed.principal;
      leaseExpiresAt = installed.expires_at;
      result = await commit(
        await prepare("prepare_owned_admission", {
          principal,
          export: exported,
        }),
      );
      if (result.kind !== "committed")
        throw Error("Owned admission was rejected.");
    }
    const after = await currentHost();
    const actor = after.actors?.find(
      (a) =>
        a.avatar_id === data.avatar_id &&
        a.control_generation === exported.control_generation &&
        a.principal === principal &&
        a.session_ref === exported.session_ref &&
        a.export_digest === exported.export_digest,
    );
    if (!actor)
      throw Error("Committed admission did not contain the owned actor.");
    const entry = {
      token,
      leaseExpiresAt,
      renewalOperation: null,
      export: exported,
      operation: data.operation,
      control: {
        principal: actor.principal,
        avatar_id: data.avatar_id,
        control_generation: exported.control_generation,
        session_ref: exported.session_ref,
      },
      binding: {
        source_epoch: exported.control_generation,
        lot_incarnation: after.binding.lot_incarnation,
        lot_location: manifest.lot.lot_location,
        avatar_id: Number(data.avatar_id),
      },
      receipt: result,
    };
    await revalidate(entry);
    admitted.set(key(token, data.avatar_id), entry);
    pendingAdmissions.delete(key(token, data.avatar_id));
    return {
      version: 1,
      state: "lot_ready",
      binding: entry.binding,
      ticket: ticket(entry),
    };
  }
  async function readBody(request) {
    if (
      request.headers["content-type"]?.split(";")[0] !== "application/json" ||
      request.headers["content-encoding"]
    )
      throw Error("Invalid request.");
    let size = 0;
    const chunks = [];
    for await (const chunk of request) {
      size += chunk.length;
      if (size > 8192) throw Error("Request too large.");
      chunks.push(chunk);
    }
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  }
  function authorizeRequest(request) {
    if (
      request.method !== "POST" ||
      request.headers.origin !== origin ||
      (request.headers["sec-fetch-site"] &&
        request.headers["sec-fetch-site"] !== "same-origin")
    )
      throw Error("Origin denied.");
    const header = request.headers.authorization;
    if (typeof header !== "string" || !/^Bearer [0-9a-f]{64}$/.test(header))
      throw Error("Account authentication required.");
    return header.slice(7);
  }
  async function handler(request, response) {
    try {
      const url = new URL(request.url, origin);
      if (
        url.origin !== origin ||
        url.search ||
        url.hash ||
        request.url.includes("%")
      )
        throw Error("Invalid request path.");
      if (
        [
          "/native-account/enter",
          "/native-account/reconnect",
          "/native-account/leave",
        ].includes(url.pathname)
      ) {
        const token = authorizeRequest(request),
          data = await readBody(request);
        const value = await serially(async () => {
          if (url.pathname.endsWith("/enter")) return enter(token, data);
          assert.deepEqual(Object.keys(data).sort(), [
            "avatar_id",
            "lot_id",
            "operation",
          ]);
          await sweepFences();
          const entry =
            admitted.get(key(token, data.avatar_id)) ??
            (await restoreEntry(token, data));
          assert.ok(
            entry &&
              entry.operation === data.operation &&
              data.lot_id === manifest.lot.lot_id,
          );
          await revalidate(entry);
          if (url.pathname.endsWith("/leave"))
            throw Error(
              "Source detach is not yet qualified. Custody remains held.",
            );
          await renewActivity(entry, token);
          return {
            version: 1,
            state: "lot_ready",
            binding: entry.binding,
            ticket: ticket(entry),
          };
        });
        response.writeHead(200, {
          "content-type": "application/json",
          "cache-control": "no-store",
        });
        response.end(JSON.stringify(value));
        return;
      }
      if (url.pathname.startsWith("/native-account/")) {
        // Only the four existing staged HTTP methods are forwarded, never RPC.
        assert.ok(
          [
            "/native-account/login",
            "/native-account/roster",
            "/native-account/logout",
            "/native-account/rotate",
          ].includes(url.pathname),
        );
        assert.equal(request.method, "POST");
        assert.equal(request.headers.origin, origin);
        const data = await readBody(request),
          headers = { ...request.headers, "cf-connecting-ip": "127.0.0.1" };
        delete headers.host;
        delete headers["content-length"];
        const result = await serially(
          async () => {
            const reply = await mf.dispatchFetch(url.href, {
              method: "POST",
              headers,
              body: JSON.stringify(data),
            });
            let value = await reply.json();
            if (
              reply.ok &&
              ["/native-account/login", "/native-account/roster"].includes(
                url.pathname,
              ) &&
              !halted
            ) {
              value = {
                ...value,
                admission_available: true,
                ...(url.pathname.endsWith("/roster")
                  ? { destinations: [manifest.lot] }
                  : {}),
              };
            }
            if (
              reply.ok &&
              ["/native-account/logout", "/native-account/rotate"].includes(
                url.pathname,
              )
            ) {
              const token = request.headers.authorization?.slice(7);
              // Stop streams immediately; signout acknowledgement waits for durable
              // covering fences. Account revocation is never a detach receipt.
              for (const [id, entry] of admitted)
                if (id.startsWith(sha(token ?? "") + ":")) {
                  entry.token = null;
                  for (const socket of sockets)
                    if (socket.entry === entry) socket.close(1008);
                  for (const [ticketId, t] of tickets)
                    if (t.entry === entry) tickets.delete(ticketId);
                }
              await sweepFences();
              value = { ...value, gameplay_detach_pending: true };
            }
            return { status: reply.status, value };
          },
          { allowPaused: true },
        );
        response.writeHead(result.status, {
          "content-type": "application/json",
          "cache-control": "no-store",
        });
        response.end(JSON.stringify(result.value));
        return;
      }
      if (url.pathname === "/wonderland-config.json") {
        response.writeHead(200, {
          "content-type": "application/json",
          "cache-control": "no-store",
        });
        response.end(
          JSON.stringify({
            version: 1,
            mode: "native_account",
            gateway_url: "/native-account",
            native_lots: true,
          }),
        );
        return;
      }
      assert.equal(request.method, "GET");
      const path = resolve(
        web,
        "." + (url.pathname === "/" ? "/index.html" : url.pathname),
      );
      assert.ok(path.startsWith(web + sep));
      const info = await stat(path);
      assert.ok(info.isFile() && info.size <= 320 * 1024 * 1024);
      const bytes = await readFile(path),
        type =
          {
            ".wasm": "application/wasm",
            ".js": "text/javascript",
            ".mjs": "text/javascript",
            ".css": "text/css",
            ".json": "application/json",
            ".png": "image/png",
            ".webp": "image/webp",
            ".jpg": "image/jpeg",
            ".svg": "image/svg+xml",
            ".html": "text/html",
            ".woff2": "font/woff2",
          }[extname(path)] ?? "application/octet-stream";
      response.writeHead(200, {
        "content-type": type,
        "cache-control": "no-store",
      });
      response.end(bytes);
    } catch {
      if (!response.headersSent)
        response.writeHead(503, {
          "content-type": "application/json",
          "cache-control": "no-store",
        });
      response.end(
        JSON.stringify({ error: "Owned property request is unavailable." }),
      );
    }
  }
  const sweepFences = () =>
    drainControlFences({
      account,
      lot,
      authority: manifest.authority_id,
      lotId: manifest.lot.lot_id,
    });
  async function close() {
    closing = true;
    if (timer) clearInterval(timer);
    for (const socket of sockets) socket.terminate();
    if (wss) wss.close();
    if (host) await host.close().catch(() => {});
    if (mf) await mf.dispose().catch(() => {});
    if (server?.listening)
      await new Promise((resolve) => server.close(resolve));
  }
  try {
    // Validate artifacts before opening any player-facing server.
    await stat(join(web, "index.html"));
    const pinnedConfig = JSON.parse(await readFile(hostConfig, "utf8"));
    assert.ok(
      pinnedConfig.appearance_file && digest(pinnedConfig.appearance_sha256),
      "Pinned source appearance is required.",
    );
    assert.equal(pinnedConfig.binding_sha256, manifest.content_release);
    assert.equal(pinnedConfig.lot_id, manifest.lot.lot_id);
    host = await openOwnedHost(hostExecutable, hostConfig);
    const initial = host.initial;
    if (
      initial.kind !== "committed_state" ||
      initial.recovery_pending ||
      initial.admission_ready !== true ||
      !initial.permission_binding ||
      !Array.isArray(initial.actors) ||
      initial.actors.length !== 0
    )
      throw Error(
        "Pinned source lot is not ready for ordinary owned admission; no actor was created.",
      );
    assert.equal(initial.cursor.lot_id, manifest.lot.lot_id);
    assert.equal(initial.source_lot_location, manifest.lot.lot_location);
    server = createServer(handler);
    server.requestTimeout = 15000;
    server.headersTimeout = 10000;
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(port, "127.0.0.1", resolve);
    });
    origin = `http://127.0.0.1:${server.address().port}`;
    const bundle = await build({
      entryPoints: [join(repo, "tools/native-browser/owned-demo/driver.ts")],
      bundle: true,
      format: "esm",
      platform: "neutral",
      external: ["cloudflare:workers"],
      write: false,
      plugins: [
        {
          name: "wasm",
          setup(b) {
            b.onResolve({ filter: /account_crypto_bg\.wasm$/ }, () => ({
              path: "./account_crypto_bg.wasm",
              external: true,
            }));
          },
        },
      ],
    });
    mf = new Miniflare({
      modulesRoot: root,
      modules: [
        {
          type: "ESModule",
          path: join(root, "worker.mjs"),
          contents: bundle.outputFiles[0].text,
        },
        {
          type: "CompiledWasm",
          path: join(root, "account_crypto_bg.wasm"),
          contents: await readFile(
            join(repo, "services/native-edge/generated/account_crypto_bg.wasm"),
          ),
        },
      ],
      compatibilityDate: "2026-07-01",
      durableObjects: {
        ACCOUNTS: { className: "AccountAuthority", useSQLite: true },
        COORD: { className: "LotCoordinator", useSQLite: true },
      },
      durableObjectsPersist: join(root, "sqlite"),
      r2Buckets: ["CHECKPOINTS"],
      r2Persist: join(root, "r2"),
      bindings: {
        PRIVATE_CAPABILITY: privateCapability,
        ACCOUNT_REALM: "owned-demo",
        LOT_NAME: manifest.lot.lot_id,
        NATIVE_ACCOUNT_HTTP: "staged-v1",
        NATIVE_ACCOUNT_ORIGIN: origin,
        NATIVE_ACCOUNT_REALM: "owned-demo",
      },
    });
    await mf.ready;
    await account("initializeAdmissionAuthorityV2", [manifest.authority_id]);
    await account("initializeCatalog", [manifest.catalog]);
    if (resume) await recoverHost();
    else {
      await lot("initializeControlledV2", [
        initial.binding,
        initial.cursor,
        sha(packet(initial.checkpoint_hex)),
        {
          authority_id: manifest.authority_id,
          content_release: manifest.content_release,
          protocol_release: manifest.protocol_release,
          permission_binding: initial.permission_binding,
          actors: initial.actors,
        },
      ]);
      await publishCheckpoint();
      await writeFile(
        join(root, "runner.json"),
        JSON.stringify({
          version: 1,
          origin,
          manifest_sha256: sha(JSON.stringify(manifest)),
        }),
        { flag: "wx" },
      );
    }
    wss = new WebSocketServer({
      noServer: true,
      maxPayload: 65536,
      perMessageDeflate: false,
    });
    server.on("upgrade", (request, socket, head) => {
      if (
        request.headers.origin !== origin ||
        request.url !== "/native-lot/stream" ||
        halted
      ) {
        socket.destroy();
        return;
      }
      wss.handleUpgrade(request, socket, head, (ws) =>
        wss.emit("connection", ws),
      );
    });
    wss.on("connection", (socket) => {
      sockets.add(socket);
      socket.live = false;
      socket.entry = null;
      let queued = 0;
      const deadline = setTimeout(() => socket.close(1008), 15000);
      socket.on("message", (bytes, binary) => {
        if (++queued > 4) {
          socket.close(1013);
          return;
        }
        serially(async () => {
          if (!socket.entry) {
            assert.equal(binary, false);
            const auth = JSON.parse(bytes.toString("utf8"));
            assert.equal(auth.type, "native_auth");
            assert.ok(
              Object.keys(auth).every((k) =>
                ["type", "ticket", "resume"].includes(k),
              ),
            );
            const value = tickets.get(auth.ticket);
            tickets.delete(auth.ticket);
            assert.ok(value && value.expires >= Date.now());
            await sweepFences();
            await revalidate(value.entry);
            socket.entry = value.entry;
            if (auth.resume !== true) {
              const boot = await host.ask({
                kind: "player_bootstrap",
                control: socket.entry.control,
                binding: socket.entry.binding,
              });
              assert.equal(boot.kind, "player_bootstrap");
              send(socket, packet(boot.packet_hex));
            }
            return;
          }
          if (!binary) {
            assert.ok(socket.live && bytes.length <= 33500);
            const query = JSON.parse(bytes.toString("utf8"));
            if (query.type === "native_action_result") {
              await reconcileAuthenticatedPlayerAction(query, socket.entry, {
                sweepFences,
                revalidate,
                lookup: (request) => lot("lookupPlayerActionV1", [request]),
                receipt: (hex) => send(socket, packet(hex)),
              });
              return;
            }
            assert.ok(bytes.length <= 128);
            assert.deepEqual(query, { type: "native_presence" });
            await sweepFences();
            await revalidate(socket.entry);
            await renewActivity(socket.entry, socket.entry.token, "presence");
            return;
          }
          await sweepFences();
          await revalidate(socket.entry);
          const magic = bytes.subarray(0, 4).toString();
          if (magic === "WLR1") {
            const checkpoint = await host.ask({
              kind: "player_checkpoint",
              control: socket.entry.control,
              request_hex: bytes.toString("hex"),
            });
            assert.equal(checkpoint.kind, "player_checkpoint");
            send(socket, packet(checkpoint.packet_hex));
            socket.live = true;
            clearTimeout(deadline);
          } else if (magic === "WLC1") {
            assert.ok(socket.live);
            assert.ok(bytes.length <= 16384);
            const proposal = await prepare("prepare_player_action", {
              control: socket.entry.control,
              action_hex: bytes.toString("hex"),
            });
            const result = await settlePlayerAction(proposal, {
              commit,
              receipt: (hex) => send(socket, packet(hex)),
            });
            if (result.kind === "committed") await renewActivity(socket.entry);
          } else throw Error("Unsupported native player packet.");
        })
          .catch(() => socket.close(1008))
          .finally(() => queued--);
      });
      socket.on("error", () => socket.close());
      socket.on("close", () => {
        clearTimeout(deadline);
        sockets.delete(socket);
      });
    });
    timer = setInterval(() => {
      if (ticking || closing || halted || ![...sockets].some((s) => s.live))
        return;
      ticking = true;
      serially(async () => {
        await sweepFences();
        const result = await commit(await prepare("prepare_player_tick"));
        if (result.kind !== "committed")
          throw Error("Authoritative tick was fenced.");
      })
        .catch(stop)
        .finally(() => (ticking = false));
    }, 1000 / 30);
    return {
      origin,
      storageRoot: root,
      async setupAccounts() {
        if (accounts) return accounts;
        if (resume)
          throw Error(
            "Ingress recovery never recreates accounts. Use the original test account credentials.",
          );
        accounts = [];
        for (const [index, spec] of manifest.avatar_specs.entries()) {
          const username = `owned_demo_${index + 1}`,
            password = randomBytes(24).toString("hex");
          await account("register", [username, password, sha("loopback-demo")]);
          const login = await account("login", [
            username,
            password,
            sha("loopback-demo"),
          ]);
          const avatar = await account("createAvatar", [
            login.token,
            `owned_demo_create_${index + 1}`,
            spec,
          ]);
          await account("logout", [login.token]);
          accounts.push({
            username,
            password,
            name: spec.name,
            avatar_id: avatar.avatar_id,
            lot_name: manifest.lot.name,
          });
        }
        return accounts;
      },
      async inspectDurable() {
        return serially(async () => {
          const coordinator = await lot("inspectControlledV2"),
            runtime = await currentHost();
          assert.deepEqual(runtime.cursor, coordinator.cursor);
          assert.equal(
            runtime.actor_registry_sha256,
            coordinator.actor_registry_sha256,
          );
          return {
            actors: runtime.actors,
            cursor: coordinator.cursor,
            actor_registry_sha256: runtime.actor_registry_sha256,
            journal: coordinator.journal,
            pending_outbox: coordinator.pending_outbox,
            halted,
          };
        });
      },
      async restartHost() {
        return serially(recoverHost, { allowPaused: true });
      },
      close,
    };
  } catch (error) {
    await close();
    throw error;
  }
}
