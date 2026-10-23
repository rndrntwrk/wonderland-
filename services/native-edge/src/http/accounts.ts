import type { AccountAuthority } from "../account/authority";
import { digest, fields, decimal } from "../lot/contracts";
import { token, operation, avatar } from "../account/contracts";

export interface AccountHttpEnv {
  NATIVE_ACCOUNT_HTTP?: string;
  NATIVE_ACCOUNT_ORIGIN?: string;
  NATIVE_ACCOUNT_REALM?: string;
  ACCOUNTS?: DurableObjectNamespace<AccountAuthority>;
}
const headers = {
  "Content-Type": "application/json",
  "Cache-Control": "no-store",
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "no-referrer",
};
const reply = (status: number, value: unknown) =>
  Response.json(value, { status, headers });
class InvalidBody extends Error {}
async function body(request: Request): Promise<unknown> {
  if (
    request.headers.get("content-type")?.split(";")[0].trim() !==
      "application/json" ||
    request.headers.has("content-encoding")
  )
    throw new InvalidBody();
  const declared = request.headers.get("content-length");
  if (declared !== null && (!/^\d+$/.test(declared) || Number(declared) > 8192))
    throw new InvalidBody();
  const reader = request.body?.getReader();
  if (!reader) throw new InvalidBody();
  const chunks: Uint8Array[] = [];
  let size = 0;
  let timedOut = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const expired = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      timedOut = true;
      reject(new InvalidBody());
      void reader.cancel();
    }, 5000);
  });
  try {
    for (;;) {
      const { done, value } = await Promise.race([reader.read(), expired]);
      if (timedOut) throw new InvalidBody();
      if (done) break;
      size += value.byteLength;
      if (size > 8192) throw new InvalidBody();
      chunks.push(value);
    }
    const bytes = new Uint8Array(size);
    let at = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, at);
      at += chunk.byteLength;
    }
    return JSON.parse(
      new TextDecoder("utf-8", { fatal: true, ignoreBOM: false }).decode(bytes),
    );
  } catch {
    void reader.cancel();
    throw new InvalidBody();
  } finally {
    if (timer !== undefined) clearTimeout(timer);
    reader.releaseLock();
  }
}
/** Staged same-origin account surface. Never exposes arbitrary RPC or lot admission. */
export async function accountHttp(
  request: Request,
  env: AccountHttpEnv,
): Promise<Response> {
  if (
    env.NATIVE_ACCOUNT_HTTP !== "staged-v1" ||
    !env.ACCOUNTS ||
    !env.NATIVE_ACCOUNT_REALM ||
    !env.NATIVE_ACCOUNT_ORIGIN
  )
    return reply(404, { error: "not_found" });
  let origin: URL;
  const url = new URL(request.url);
  try {
    origin = new URL(env.NATIVE_ACCOUNT_ORIGIN);
  } catch {
    return reply(503, { error: "unavailable" });
  }
  if (
    origin.origin !== env.NATIVE_ACCOUNT_ORIGIN ||
    origin.username ||
    origin.password ||
    (origin.protocol !== "https:" &&
      !(
        origin.protocol === "http:" &&
        ["localhost", "127.0.0.1", "[::1]"].includes(origin.hostname)
      ))
  )
    return reply(503, { error: "unavailable" });
  if (
    url.origin !== origin.origin ||
    url.search ||
    request.headers.get("origin") !== origin.origin ||
    (request.headers.has("sec-fetch-site") &&
      request.headers.get("sec-fetch-site") !== "same-origin")
  )
    return reply(403, { error: "origin_denied" });
  const action = url.pathname.replace(/^\/native-account\//, "");
  if (
    !url.pathname.startsWith("/native-account/") ||
    !["login", "roster", "logout", "rotate", "create"].includes(action)
  )
    return reply(404, { error: "not_found" });
  if (request.method !== "POST")
    return reply(405, { error: "method_not_allowed" });
  let data: unknown;
  try {
    data = await body(request);
  } catch {
    return reply(400, { error: "invalid_request" });
  }
  const stub = env.ACCOUNTS.get(
    env.ACCOUNTS.idFromName(env.NATIVE_ACCOUNT_REALM),
  );
  try {
    if (action === "login") {
      const v = fields(data, ["username", "password"]);
      // Cloudflare overwrites this header at the edge. No body/forwarded header fallback.
      // A non-Cloudflare deployment must establish equivalent trusted ingress first.
      const ip = request.headers.get("cf-connecting-ip");
      if (!ip || ip.length > 64) return reply(503, { error: "unavailable" });
      const network = await digest(
        new TextEncoder().encode(env.NATIVE_ACCOUNT_REALM + "\0" + ip),
      );
      const result = await stub.login(v.username, v.password, network);
      return reply(200, {
        version: 1,
        state: "account_ready",
        ...result,
        admission_available: false,
      });
    }
    const authorization = request.headers.get("authorization");
    if (!authorization?.startsWith("Bearer "))
      return reply(401, { error: "unauthorized" });
    const secret = token(authorization.slice(7));
    if (action === "create") {
      let creation;
      try {
        const value = fields(data, ["operation", "avatar"]);
        creation = {
          operation: operation(value.operation),
          avatar: avatar(value.avatar),
        };
      } catch {
        throw new InvalidBody();
      }
      // The session selects the owner. The existing transactional authority
      // validates pinned content and retains the exact operation/result; a
      // retry cannot allocate another Sim or grant browser-supplied ownership.
      return reply(200, {
        version: 1,
        state: "avatar_created",
        avatar: await stub.createAvatar(
          secret,
          creation.operation,
          creation.avatar,
        ),
      });
    }
    if (action === "roster") {
      const v = fields(data, ["after"]);
      return reply(200, {
        version: 1,
        state: "account_ready",
        ...(await stub.roster(secret, decimal(v.after))),
        admission_available: false,
      });
    }
    if (action === "logout") {
      const v = fields(data, ["all"]);
      if (typeof v.all !== "boolean") throw new InvalidBody();
      return reply(200, await stub.logout(secret, v.all));
    }
    fields(data, []);
    return reply(200, {
      version: 1,
      state: "account_ready",
      ...(await stub.rotate(secret)),
      admission_available: false,
    });
  } catch (error) {
    if (error instanceof InvalidBody)
      return reply(400, { error: "invalid_request" });
    // Never return SQL details, credentials, verifier material or account existence.
    const unauthorized =
      error instanceof Error && /(^|: )unauthorized$/.test(error.message);
    return unauthorized
      ? reply(401, { error: "unauthorized" })
      : reply(503, { error: "unavailable" });
  }
}
