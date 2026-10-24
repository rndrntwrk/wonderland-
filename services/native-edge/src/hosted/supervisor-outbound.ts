// Register only as the trusted Container class's outbound handler for
// owned-private.invalid. This is NOT a public Worker fetch handler.
interface SupervisorOutboundOptions {
  publicOrigin: string;
  dispatchRpc: (body: unknown) => Promise<Response>;
  accountHttp: (request: Request) => Promise<Response>;
  maxPrivateRequestBytes?: number;
}
const ACCOUNT_PATHS = new Set([
  "/native-account/login", "/native-account/roster",
  "/native-account/logout", "/native-account/rotate",
  "/native-account/create",
  "/native-account/creation-options",
  "/native-account/assets",
]);
const MAX_PRIVATE_BYTES = 40 * 1024 * 1024;
async function boundedJson(request: Request, maximum: number): Promise<unknown> {
  const declared = request.headers.get("content-length");
  if (declared !== null && (!/^(0|[1-9][0-9]*)$/.test(declared) || Number(declared) > maximum))
    throw Error("Private request exceeds its byte budget");
  if (!request.body) throw Error("Missing private request body");
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    for (;;) {
      const {value, done} = await reader.read();
      if (done) break;
      if (value.byteLength > maximum - length) {
        await reader.cancel();
        throw Error("Private request exceeds its byte budget");
      }
      length += value.byteLength;
      chunks.push(value);
    }
  } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  return JSON.parse(new TextDecoder("utf-8", {fatal:true,ignoreBOM:false}).decode(bytes));
}
export function createSupervisorOutboundHandler(options: SupervisorOutboundOptions) {
  const origin = new URL(options.publicOrigin);
  if (origin.protocol !== "https:" || options.publicOrigin !== origin.origin)
    throw Error("Hosted public origin must be an exact HTTPS origin");
  const maximum = options.maxPrivateRequestBytes ?? MAX_PRIVATE_BYTES;
  if (!Number.isSafeInteger(maximum) || maximum < 1 || maximum > MAX_PRIVATE_BYTES)
    throw Error("Invalid private request byte budget");
  return async (request: Request): Promise<Response> => {
    const url = new URL(request.url);
    if (url.origin !== "http://owned-private.invalid" || url.search || url.hash)
      return new Response(null, {status:404});
    if (request.method !== "POST" || request.headers.get("content-type") !== "application/json")
      return new Response(null, {status:403});
    if (url.pathname === "/rpc") {
      if (request.headers.has("origin")) return new Response(null, {status:403});
      try { return await options.dispatchRpc(await boundedJson(request, maximum)); }
      catch { return Response.json({error:"Invalid private request"},{status:400}); }
    }
    if (url.pathname.startsWith("/account-http/")) {
      const path = url.pathname.slice("/account-http".length);
      if (!ACCOUNT_PATHS.has(path)) return new Response(null, {status:404});
      if (request.headers.get("origin") !== origin.origin)
        return new Response(null, {status:403});
      // AccountHttp keeps its original Origin, ingress-IP, body-size, timeout,
      // session and throttle checks. No localhost IP or fabricated identity.
      const forwarded = new Request(origin.origin + path, request);
      forwarded.headers.delete("x-owned-private");
      forwarded.headers.delete("host");
      return options.accountHttp(forwarded);
    }
    return new Response(null, {status:404});
  };
}
