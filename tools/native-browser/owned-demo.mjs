// Same-cloud native supervisor. Defaults to loopback; hosted mode is explicit.
// Requires a pinned source lot and
// content release. No fixed actors, generic Spawn or scripted success fallback.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import {
  readFile,
  writeFile,
  mkdtemp,
  mkdir,
  stat,
  readdir,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join, dirname, extname, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { randomBytes, createHash } from "node:crypto";
import {
  authoredOwnerReceipt,
  authoredGenesis,
} from "./owned-demo/authored-genesis.mjs";
import { ownedPlacement, ownedReadiness } from "./owned-demo/placement.mjs";
import { openOwnedHost } from "./owned-demo/process.mjs";
import { currentPreparationStatus } from "./owned-demo/preparation-status.mjs";
import { drainControlFences } from "./owned-demo/fences.mjs";
import {
  departureRequest,
  departureForOwnedEntry,
  activeDepartures,
  deliverDepartureReceipts,
  drainOneDeparture,
} from "./owned-demo/departures.mjs";
import { authenticatedEod } from "./owned-demo/eod.mjs";
import { settlePlayerAction } from "./owned-demo/actions.mjs";
import { reconcileAuthenticatedPlayerAction } from "./owned-demo/action-results.mjs";
import { validateOwnedManifest } from "./owned-demo/manifest.mjs";
import { stageRecovery } from "./owned-demo/recovery.mjs";
import { supervisorProfile } from "./owned-demo/supervisor-profile.mjs";
import { loadOrCreateExport } from "./owned-demo/admission.mjs";
import {
  askWithDurableReceipts,
  retireCheckpointReceipts,
} from "./owned-demo/receipts.mjs";
const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const require = createRequire(join(repo, "services/native-edge/package.json"));
const { WebSocketServer } = require("ws");
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
  hostExecutable = resolve(repo, "target/release/wonderland-native-lot-host"),
  storageRoot,
  port = 0,
  resume = false,
  authorGenesis,
  hosted,
  contentProvider,
}) {
  const profile = supervisorProfile(hosted);
  validateOwnedManifest(manifest);
  manifest = JSON.parse(JSON.stringify(manifest));
  if (contentProvider) {
    assert.equal(contentProvider.release, manifest.content_release);
    assert.equal(typeof contentProvider.manifest, "function");
    assert.equal(typeof contentProvider.read, "function");
  }
  assert.ok(authorGenesis === undefined || typeof authorGenesis === "function");
  assert.ok(
    !resume || authorGenesis === undefined,
    "Authored setup is fresh-only.",
  );
  assert.ok(
    !resume || profile.hosted || storageRoot,
    "Ingress recovery requires the original durable storage root.",
  );
  const root = storageRoot
    ? resolve(storageRoot)
    : await mkdtemp(join(tmpdir(), "wonderland-owned-"));
  await mkdir(root, { recursive: true });
  if (authorGenesis)
    assert.deepEqual(
      await readdir(root),
      [],
      "Authored setup requires a fresh empty store.",
    );
  const web = resolve(dist);
  let hostConfig = resolve(manifest.host_config),
    firstOwnerReceipt = null,
    authoredSetupReceipt = null,
    ready = false;
  if (resume && !profile.hosted) {
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
    departureWork = false,
    departureDeliveryError = null,
    eventDeliveryError = null,
    eventLaneCursor = "",
    eventDeliveryPending = [],
    requestFailure = null,
    lastIdleFenceSweep = 0,
    accounts = null;
  let resetTickCadence = () => {};
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
  const sendPrivate = (socket, value) => {
    if (socket.readyState !== 1 || !socket.live) return;
    const text = JSON.stringify(value);
    assert.ok(Buffer.byteLength(text) <= 512 * 1024);
    if (socket.bufferedAmount + Buffer.byteLength(text) > 82 * 1024 * 1024) { socket.close(1013); return; }
    socket.send(text, {binary:false});
  };
  async function flushPrivate(socket) {
    if (!socket.live || !socket.entry || socket.readyState !== 1) return;
    const output = await host.ask({kind:"read_private_eod",control:socket.entry.control});
    // A concurrently revoked owned session has no private delivery authority.
    if (output.kind === "denied") return;
    assert.equal(output.kind,"private_eod_outputs");
    if (output.messages.length) sendPrivate(socket,{type:"native_eod_outputs",messages:output.messages});
  }
  const broadcast = (hex) => {
    const bytes = packet(hex);
    for (const socket of sockets) if (socket.live) send(socket, bytes);
  };
  const stop = (error) => {
    resetTickCadence();
    halted = String(error.message ?? error);
    for (const socket of sockets) socket.close(1012);
  };
  async function commit(proposal, method = "commitControlledBatchV2") {
    assert.ok(
      ["commitControlledBatchV2", "commitOwnedDrainV1"].includes(method),
    );
    if (proposal.kind !== "proposal")
      throw Error(proposal.reason ?? "Native transition was refused.");
    let receipt;
    try {
      receipt = await lot(method, [proposal]);
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
    for (const socket of sockets) await flushPrivate(socket);
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
    resetTickCadence();
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
      const restored = await host.ask({kind:"restore_native_eod",checkpoint:await lot("currentNativeEodV1")});
      assert.equal(restored.kind,"native_eod_restored",restored.reason);
      const finished = await lot("finishControlledRecoveryV2", [
        {
          schema_version: 2,
          marker: candidate.takeover_marker,
          control: bundle.control,
          native_eod_v1: restored.native_eod_v1,
        },
        candidate.checkpoint_hex,
      ]);
      assert.equal(finished.schema_version, 2);
      assert.deepEqual(finished.marker, candidate.takeover_marker);
      assert.deepEqual(finished.control, bundle.control);
      assert.equal(finished.native_eod_checkpoint_sha256, restored.native_eod_v1.checkpoint_sha256);
      const activated = await host.ask({
        kind: "activate_recovery",
        marker: finished.marker,
        native_eod_checkpoint_sha256: finished.native_eod_checkpoint_sha256,
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
    const current = await currentPreparationStatus(host);
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
      return ownedReadiness(await revalidate(previous), previous, ticket);
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
    return ownedReadiness(await currentHost(), entry, ticket);
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
  function forgetReleased(identity) {
    for (const [k, entry] of admitted) {
      if (
        entry.export.avatar_id === identity.avatar_id &&
        entry.export.control_generation === identity.control_generation &&
        entry.operation === identity.admission_operation
      ) {
        for (const socket of sockets)
          if (socket.entry === entry) socket.close(1000);
        for (const [id, grant] of tickets)
          if (grant.entry === entry) tickets.delete(id);
        admitted.delete(k);
      }
    }
  }
  async function deliverOwnedEvents() {
    try {
      const page=await lot("readOwnedEventLanesV1",[eventLaneCursor]);
      eventDeliveryPending = page.unsupported_effect_lanes ? [{lane:"effects",reason:"no_consumer",count:page.unsupported_effect_lanes}] : [];
      for(const lane of page.lanes){
        if(!lane.supported){eventDeliveryPending.push({batch_key:lane.batch_key,reason:lane.reason});continue;}
        const proof=await host.ask({kind:"inspect_owned_event_lane",events_hex:lane.events_hex});
        assert.equal(proof.kind,"owned_event_lane_proof",proof.reason);
        if(proof.pending.length){eventDeliveryPending.push({batch_key:lane.batch_key,pending:proof.pending});continue;}
        await lot("retireOwnedEventLaneV1",[{batch_key:lane.batch_key,digest:lane.digest,proof}]);
      }
      eventLaneCursor=page.next??"";
      eventDeliveryError=null;
    } catch(error) {
      // No successful consumer receipt: retain and retry exactly. This lane
      // does not grant input or bypass the separate custody-release workflow.
      eventDeliveryError=String(error.message??error);
    }
  }
  async function deliverDepartures() {
    try {
      await deliverDepartureReceipts({
        lot,
        account,
        onReleased: forgetReleased,
      });
      departureDeliveryError = null;
      await deliverOwnedEvents();
      return true;
    } catch (error) {
      // An account/storage transport error cannot prove release. Retain durable
      // work and retry; never fabricate an acknowledgment or ordinary tick past
      // a still-fenced control.
      departureDeliveryError = String(error.message ?? error);
      departureWork = true;
      return false;
    }
  }
  async function leaveOwned(token, data) {
    const owner = await account("ownedAvatar", [token, data.avatar_id, false]);
    let existing = await lot("findOwnedDepartureV1", [
      {
        account_id: owner.account_id,
        avatar_id: data.avatar_id,
        session_ref: sha(token),
        admission_operation: data.operation,
      },
    ]);
    if (!existing) {
      const entry =
        admitted.get(key(token, data.avatar_id)) ??
        (await restoreEntry(token, data));
      assert.equal(entry.operation, data.operation);
      const current = await revalidate(entry);
      const request = departureForOwnedEntry(
        current,
        entry,
        manifest.authority_id,
        manifest.lot.lot_id,
      );
      const fenced = await account("requestAvatarControlFenceV1", [
        token,
        {
          avatar_id: entry.export.avatar_id,
          generation: entry.export.control_generation,
          admission_operation: entry.operation,
          reservation_digest: entry.export.reservation_digest,
        },
      ]);
      assert.equal(fenced.claim.state, "fence_pending");
      existing = await lot("beginOwnedDepartureV1", [request]);
      for (const socket of sockets)
        if (socket.entry === entry) socket.close(1000);
      for (const [id, grant] of tickets)
        if (grant.entry === entry) tickets.delete(id);
    }
    if (existing.status !== "released") departureWork = true;
    return {
      version: 1,
      state: existing.status === "released" ? "detached" : "detach_pending",
      avatar_id: data.avatar_id,
      status: existing.status,
      blocker: existing.observation?.blocker ?? null,
    };
  }
  async function handler(request, response) {
    try {
      if (!ready) throw Error("Owned property initialization is incomplete.");
      const url = new URL(request.url, origin);
      if (
        url.origin !== origin ||
        url.search ||
        url.hash ||
        request.url.includes("%")
      )
        throw Error("Invalid request path.");
      if (url.pathname.startsWith("/native-content/")) {
        if (!contentProvider) {
          response.writeHead(404); response.end(); return;
        }
        const token = authorizeRequest(request), data = await readBody(request);
        const resource = url.pathname === "/native-content/manifest" ? null
          : /^\/native-content\/file\/([0-9a-f]{64})$/.exec(url.pathname)?.[1];
        assert.ok(resource !== undefined, "Unsupported content resource.");
        // Recheck the live account reservation and committed source placement on
        // every request. A manifest or hash is never a substitute for admission.
        await serially(async () => {
          assert.deepEqual(Object.keys(data).sort(), ["avatar_id", "lot_id", "operation"]);
          assert.equal(data.lot_id, manifest.lot.lot_id);
          const entry = admitted.get(key(token, data.avatar_id)) ?? await restoreEntry(token, data);
          assert.equal(entry.operation, data.operation);
          await sweepFences();
          const current = await revalidate(entry);
          assert.ok(ownedPlacement(current, entry).in_world);
          await renewActivity(entry, token);
        });
        const body = resource === null ? JSON.stringify(contentProvider.manifest())
          : await contentProvider.read(resource);
        response.writeHead(200, {
          "content-type": resource === null ? "application/json" : "application/octet-stream",
          "cache-control": "private, no-store",
          "cross-origin-resource-policy": "same-origin",
          "x-content-type-options": "nosniff",
        });
        response.end(body); return;
      }
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
          assert.equal(data.lot_id, manifest.lot.lot_id);
          assert.ok(dec(data.avatar_id) && BigInt(data.avatar_id) <= 16777215n);
          assert.match(data.operation, /^[a-zA-Z0-9_-]{16,96}$/);
          await sweepFences();
          if (url.pathname.endsWith("/leave")) return leaveOwned(token, data);
          const entry =
            admitted.get(key(token, data.avatar_id)) ??
            (await restoreEntry(token, data));
          assert.ok(
            entry &&
              entry.operation === data.operation &&
              data.lot_id === manifest.lot.lot_id,
          );
          const current = await revalidate(entry);
          const readiness = ownedReadiness(current, entry, ticket);
          if (readiness.state === "lot_ready")
            await renewActivity(entry, token);
          return readiness;
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
          headers = profile.accountHeaders(request.headers);
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
    } catch (error) {
      // Supervisor-only bounded diagnostics. Never retain request data, tokens,
      // assertion values or arbitrary exception text in this projection.
      const message = String(error?.message ?? "");
      requestFailure = {
        name: error?.name === "AssertionError" ? "AssertionError" : "Error",
        message: /^[A-Za-z ._-]{1,160}$/.test(message) ? message : "redacted",
        frames: String(error?.stack ?? "")
          .split("\n")
          .slice(1)
          .filter((line) => /^\s+at [A-Za-z0-9_. /():-]+$/.test(line))
          .slice(0, 6),
      };
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
      onFence: async ({ claim }) => {
        const current = await currentHost();
        const actor = current.actors.find(
          (a) =>
            a.avatar_id === claim.avatar_id &&
            a.control_generation === claim.generation,
        );
        // Never-admitted reservations require a separate absence proof; do not
        // fabricate a native departure for them.
        if (!actor) return;
        await lot("beginOwnedDepartureV1", [
          departureRequest({
            authority_id: manifest.authority_id,
            lot_id: manifest.lot.lot_id,
            ...actor,
          }),
        ]);
        departureWork = true;
      },
    });
  async function setupAccounts() {
    if (accounts) return accounts;
    if (resume)
      throw Error(
        "Ingress recovery never recreates accounts. Use the original test account credentials.",
      );
    accounts = [];
    for (const [index, spec] of manifest.avatar_specs.entries()) {
      const username = `owned_demo_${index + 1}`,
        password = randomBytes(24).toString("hex");
      const registered = await account("register", [
        username,
        password,
        sha("loopback-demo"),
      ]);
      const login = await account("login", [
        username,
        password,
        sha("loopback-demo"),
      ]);
      const operation = `owned_demo_create_${index + 1}`;
      const avatar = await account("createAvatar", [
        login.token,
        operation,
        spec,
      ]);
      if (index === 0 && authorGenesis) {
        const owned = await account("ownedAvatar", [
          login.token,
          avatar.avatar_id,
          false,
        ]);
        firstOwnerReceipt = authoredOwnerReceipt(
          manifest,
          registered,
          login,
          avatar,
          owned,
          operation,
        );
      }
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
  }
  async function close() {
    closing = true;
    resetTickCadence();
    if (timer) clearTimeout(timer);
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
    let initial = host.initial;
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
      server.listen(port, profile.listenHost, resolve);
    });
    origin = profile.hosted ? profile.publicOrigin
      : `http://127.0.0.1:${server.address().port}`;
    if (profile.hosted) {
      mf = profile.transport;
    } else {
      const { Miniflare } = require("miniflare"), { build } = require("esbuild");
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
        // This private loopback host needs no remote Request.cf metadata refresh.
        // Miniflare otherwise fetches workers.cloudflare.com/cf.json by default.
        cf: false,
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
    }
    await account("initializeAdmissionAuthorityV2", [manifest.authority_id]);
    await account("initializeCatalog", [manifest.catalog]);
    if (authorGenesis) {
      await setupAccounts();
      assert.ok(firstOwnerReceipt);
      const authored = await authoredGenesis(
        authorGenesis,
        manifest,
        firstOwnerReceipt,
      );
      // This operator hook cannot create a ready browser actor or change the
      // account catalog, release, authority, lot identity or selected avatars.
      const basePermission = initial.permission_binding;
      const baseRegistry = initial.actor_registry_sha256;
      await host.close();
      manifest = authored.manifest;
      authoredSetupReceipt = authored.receipt;
      hostConfig = resolve(manifest.host_config);
      host = await openOwnedHost(hostExecutable, hostConfig);
      initial = host.initial;
      assert.equal(initial.kind, "committed_state");
      assert.equal(initial.recovery_pending, false);
      assert.equal(initial.admission_ready, true);
      assert.deepEqual(initial.permission_binding, basePermission);
      assert.equal(initial.actor_registry_sha256, baseRegistry);
      assert.deepEqual(initial.actors, []);
      assert.deepEqual(initial.actor_placements, []);
      assert.equal(initial.cursor.lot_id, manifest.lot.lot_id);
      assert.equal(initial.source_lot_location, manifest.lot.lot_location);
      await writeFile(
        join(root, "authored-manifest.json"),
        JSON.stringify(manifest),
        { flag: "wx" },
      );
      await writeFile(
        join(root, "authored-setup-receipt.json"),
        JSON.stringify(authoredSetupReceipt),
        { flag: "wx" },
      );
    }
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
      if (!profile.hosted) await writeFile(
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
            const current = await revalidate(value.entry);
            if (!ownedPlacement(current, value.entry).in_world) {
              socket.close(4009, "source_placement_unavailable");
              return;
            }
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
            if (["native_eod_read","native_eod_ui","native_eod_ack"].includes(query.type)) {
              await authenticatedEod(query,socket.entry,{
                revalidate:async(entry)=>{await sweepFences(); const current=await revalidate(entry); assert.ok(ownedPlacement(current,entry).in_world);},
                prepare,commit,
                read:(control)=>host.ask({kind:"read_private_eod",control}),
                send:(value)=>sendPrivate(socket,value),
              });
              return;
            }
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
            const current = await revalidate(socket.entry);
            if (!ownedPlacement(current, socket.entry).in_world) {
              socket.close(4009, "source_placement_unavailable");
              return;
            }
            await renewActivity(socket.entry, socket.entry.token, "presence");
            return;
          }
          await sweepFences();
          const current = await revalidate(socket.entry);
          if (!ownedPlacement(current, socket.entry).in_world) {
            socket.close(4009, "source_placement_unavailable");
            return;
          }
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
            await flushPrivate(socket);
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
    await sweepFences();
    departureWork =
      (await activeDepartures(lot)).length > 0 ||
      (await lot("pendingOwnedDeparturesV1")).length > 0;
    const tickPeriodMs = 1000 / 30;
    const maxTickDebtMs = 100;
    const maxRecoveryTurns = 6;
    let nextTickDueAt, armedTimerDueAt, recoveryTurns = 0;
    resetTickCadence = () => {
      nextTickDueAt = undefined;
      recoveryTurns = 0;
    };
    async function pumpTick() {
      let bodyStartedAt;
      let ordinaryCommitted = false;
      try {
        if (closing || halted) {
          resetTickCadence();
          return;
        }
        if (performance.now() - armedTimerDueAt >= maxTickDebtMs)
          resetTickCadence();
        const live = [...sockets].some((s) => s.live);
        // Disconnection may precede lease expiry. Continue non-renewing security
        // sweeps while idle so later expiry becomes durable departure work.
        if (!departureWork && !live) {
          resetTickCadence();
          if (Date.now() - lastIdleFenceSweep < 5000) return;
          lastIdleFenceSweep = Date.now();
        }
        ticking = true;
        const tickQueuedAt = performance.now();
        await serially(async () => {
          if (closing) return;
          bodyStartedAt = performance.now();
          const queueWaitMs = bodyStartedAt - tickQueuedAt;
          // Only an active phase may recover short FIFO waits within the debt/burst caps.
          // Initial, inactive and excessive waits start at actual execution instead.
          if (queueWaitMs >= maxTickDebtMs) resetTickCadence();
          if (nextTickDueAt === undefined) nextTickDueAt = bodyStartedAt;
          await sweepFences();
          if (!(await deliverDepartures())) return;
          if (await drainOneDeparture({ lot, prepare, commit })) {
            departureWork = true;
            await deliverDepartures();
            return;
          }
          departureWork = (await lot("pendingOwnedDeparturesV1")).length > 0;
          if (departureWork || ![...sockets].some((s) => s.live)) return;
          const result = await commit(await prepare("prepare_player_tick"));
          if (result.kind !== "committed")
            throw Error("Authoritative tick was fenced.");
          ordinaryCommitted = true;
          nextTickDueAt += tickPeriodMs;
        });
      } catch (error) {
        stop(error);
      } finally {
        ticking = false;
        let delay = tickPeriodMs;
        if (!ordinaryCommitted || halted || closing || nextTickDueAt === undefined) {
          resetTickCadence();
        } else {
          const now = performance.now();
          const debtMs = now - nextTickDueAt;
          if (debtMs > maxTickDebtMs) {
            resetTickCadence();
          } else if (debtMs >= 0 && recoveryTurns >= maxRecoveryTurns) {
            resetTickCadence();
            // Resume ordinary start spacing without adding another full idle period.
            const remaining = bodyStartedAt + tickPeriodMs - now;
            if (remaining > 0) delay = remaining;
          } else {
            delay = Math.max(0, -debtMs);
            recoveryTurns = delay === 0 ? recoveryTurns + 1 : 0;
          }
        }
        // One yielded FIFO body per turn; bounded recovery never batches source ticks.
        // Keep harmless polling while halted so successful recovery can resume.
        if (!closing) {
          armedTimerDueAt = performance.now() + delay;
          timer = setTimeout(pumpTick, delay);
        }
      }
    }
    armedTimerDueAt = performance.now() + tickPeriodMs;
    timer = setTimeout(pumpTick, tickPeriodMs);
    ready = true;
    return {
      origin,
      // Supervisor control only; never included in browser configuration.
      localAddress: server.address(),
      storageRoot: root,
      manifest: JSON.parse(JSON.stringify(manifest)),
      authored_setup_receipt: authoredSetupReceipt,
      setupAccounts,
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
            actor_placements: runtime.actor_placements,
            cursor: coordinator.cursor,
            actor_registry_sha256: runtime.actor_registry_sha256,
            journal: coordinator.journal,
            pending_outbox: coordinator.pending_outbox,
            departure_delivery_error: departureDeliveryError,
            event_delivery_error: eventDeliveryError,
            event_delivery_pending: eventDeliveryPending,
            request_failure: requestFailure,
            halted,
          };
        }, { allowPaused: true });
      },
      // Supervisor-only evidence inspection. This object is never exposed by
      // the HTTP or WebSocket router; browser callers cannot choose proof RPCs.
      async inspectDeparture(selector) {
        return serially(async () => {
          const departure = await lot("findOwnedDepartureV1", [selector]);
          if (!departure) return null;
          const request = {
            identity: departure.identity,
            departure_operation: departure.departure_operation,
          };
          const saved = await account("lookupSavedAvatarControlV1", [request]);
          const released = await account("lookupReleasedAvatarControlV1", [
            request,
          ]);
          return { departure, saved, released };
        });
      },
      async inspectNativeCheckpoint() {
        // Private supervisor evidence only; never routed to browser clients.
        return serially(async () => {
          const current = await currentHost();
          assert.ok(typeof current.checkpoint_hex === "string");
          return current.checkpoint_hex;
        }, { allowPaused: true });
      },
      async restartHost({ consecutive = 1 } = {}) {
        assert.ok(Number.isSafeInteger(consecutive) && consecutive >= 1 && consecutive <= 3);
        return serially(async () => {
          const results = [];
          for (let i=0; i<consecutive; i++) results.push(await recoverHost());
          return consecutive === 1 ? results[0] : results;
        }, { allowPaused: true });
      },
      close,
    };
  } catch (error) {
    await close();
    throw error;
  }
}
