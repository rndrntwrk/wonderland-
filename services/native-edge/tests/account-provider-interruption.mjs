import assert from "node:assert/strict";
import { test } from "node:test";
import { createNativeAccountProvider } from "../../../apps/web-shell/public/native-account.mjs";

const session = {
  version: 1,
  state: "account_ready",
  admission_available: false,
  token: "a".repeat(64),
};
const roster = {
  version: 1,
  state: "account_ready",
  admission_available: false,
  account_id: "1",
  avatars: [{ avatar_id: "1", name: "First Traveller" }],
  next: null,
};
const deferred = () => {
  let resolve;
  const promise = new Promise((r) => {
    resolve = r;
  });
  return { promise, resolve };
};

test("refresh keeps the authenticated roster visible and sign-out invalidates a late response", async () => {
  let latest,
    delay = false,
    requests = [];
  const pending = deferred();
  const provider = createNativeAccountProvider(
    "/native-account",
    (value) => (latest = value),
    {
      origin: "https://test",
      fetch: async (url, options) => {
        requests.push({ url, options });
        if (url.endsWith("/login")) return Response.json(session);
        if (url.endsWith("/logout")) return Response.json({ signed_out: true });
        return delay ? pending.promise : Response.json(roster);
      },
    },
  );
  try {
    await provider.login("owner", "secret");
    delay = true;
    const refresh = provider.refresh();
    assert.equal(latest.state, "account_refreshing");
    assert.deepEqual(latest.avatars, roster.avatars);
    await provider.refresh();
    assert.equal(requests.filter((r) => r.url.endsWith("/roster")).length, 2);
    await provider.logout();
    assert.equal(latest.state, "signed_out");
    assert.deepEqual(latest.avatars, []);
    pending.resolve(Response.json(roster));
    await refresh;
    assert.equal(latest.state, "signed_out");
  } finally {
    pending.resolve(Response.json(roster));
    provider.dispose();
  }
});

test("repeated sign-out does not abort the only server revocation or falsely confirm it", async () => {
  let latest,
    logoutSignal,
    logoutCount = 0;
  const pending = deferred();
  const provider = createNativeAccountProvider(
    "/native-account",
    (value) => (latest = value),
    {
      origin: "https://test",
      fetch: async (url, options) => {
        if (url.endsWith("/login")) return Response.json(session);
        if (url.endsWith("/roster")) return Response.json(roster);
        logoutCount++;
        logoutSignal = options.signal;
        return pending.promise;
      },
    },
  );
  try {
    await provider.login("owner", "secret");
    const logout = provider.logout();
    assert.equal(latest.state, "signing_out");
    assert.deepEqual(latest.avatars, []);
    await provider.logout();
    assert.equal(logoutCount, 1);
    assert.equal(logoutSignal.aborted, false);
    assert.equal(latest.state, "signing_out");
    await provider.login("other", "secret");
    pending.resolve(new Response(null, { status: 503 }));
    await logout;
    assert.equal(latest.state, "signed_out");
    assert.match(latest.message, /not confirmed/);
  } finally {
    pending.resolve(new Response(null, { status: 503 }));
    provider.dispose();
  }
});

test("expired account roster is not reused while a different account refreshes", async () => {
  let latest,
    phase = "first";
  const pending = deferred();
  const provider = createNativeAccountProvider(
    "/native-account",
    (value) => (latest = value),
    {
      origin: "https://test",
      fetch: async (url) => {
        if (url.endsWith("/login")) return Response.json(session);
        if (phase === "first") return Response.json(roster);
        if (phase === "expired") return new Response(null, { status: 401 });
        if (phase === "other-error") return new Response(null, { status: 503 });
        return pending.promise;
      },
    },
  );
  try {
    await provider.login("first", "secret");
    phase = "expired";
    await provider.refresh();
    assert.equal(latest.state, "signed_out");
    phase = "other-error";
    await provider.login("other", "secret");
    assert.equal(latest.state, "account_error");
    phase = "other-refresh";
    const refresh = provider.refresh();
    assert.equal(latest.state, "account_refreshing");
    assert.deepEqual(latest.avatars, []);
    pending.resolve(Response.json({ ...roster, account_id: "2", avatars: [] }));
    await refresh;
  } finally {
    pending.resolve(Response.json({ ...roster, avatars: [] }));
    provider.dispose();
  }
});

test("aborted response body does not create an unhandled cancellation rejection", async () => {
  let latest;
  const provider = createNativeAccountProvider(
    "/native-account",
    (value) => (latest = value),
    {
      origin: "https://test",
      fetch: async () =>
        new Response(
          new ReadableStream({
            start(controller) {
              controller.error(new DOMException("Aborted", "AbortError"));
            },
          }),
          { headers: { "content-type": "application/json" } },
        ),
    },
  );
  await provider.login("owner", "secret");
  assert.equal(latest.state, "signed_out");
  provider.dispose();
  await new Promise((resolve) => setImmediate(resolve));
});
