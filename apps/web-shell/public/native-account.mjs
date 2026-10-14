// Native account transport only. It never manufactures a lot-ready session.
export function createNativeAccountProvider(base, publish, options = {}) {
  const origin = options.origin ?? globalThis.location.origin;
  const endpoint = new URL(base, origin);
  if (
    (endpoint.protocol !== "https:" &&
      !(
        endpoint.protocol === "http:" &&
        ["localhost", "127.0.0.1", "[::1]"].includes(endpoint.hostname)
      )) ||
    endpoint.origin !== origin ||
    endpoint.pathname !== "/native-account" ||
    endpoint.search ||
    endpoint.hash ||
    endpoint.username ||
    endpoint.password
  )
    throw Error("Invalid native account endpoint");
  const fetcher = options.fetch ?? globalThis.fetch;
  let token = null,
    generation = 0,
    active = null,
    disposed = false;
  const emit = (value) => {
    if (!disposed) publish(value);
  };
  async function request(action, data, secret, signal) {
    const r = await fetcher(endpoint.href + "/" + action, {
      method: "POST",
      credentials: "omit",
      cache: "no-store",
      redirect: "error",
      headers: {
        "Content-Type": "application/json",
        ...(secret ? { Authorization: "Bearer " + secret } : {}),
      },
      body: JSON.stringify(data),
      signal,
    });
    if (!r.ok) {
      const error = Error(
        r.status === 401
          ? "Sign in again to access your account."
          : "The native account service is unavailable.",
      );
      error.unauthorized = r.status === 401;
      throw error;
    }
    if (!r.headers.get("content-type")?.startsWith("application/json"))
      throw Error("Invalid account response.");
    const reader = r.body?.getReader();
    if (!reader) throw Error("Invalid account response.");
    let size = 0;
    const chunks = [];
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        size += value.length;
        if (size > 65536) throw Error("Account response exceeds its limit.");
        chunks.push(value);
      }
      const bytes = new Uint8Array(size);
      let at = 0;
      for (const c of chunks) {
        bytes.set(c, at);
        at += c.length;
      }
      try {
        return JSON.parse(
          new TextDecoder("utf-8", { fatal: true }).decode(bytes),
        );
      } catch {
        throw Error("Invalid account response.");
      }
    } finally {
      void reader.cancel();
      reader.releaseLock();
    }
  }
  function begin() {
    if (disposed || active) return null;
    const n = ++generation;
    const controller = new AbortController();
    active = controller;
    emit({
      state: "loading",
      avatars: [],
      message: "Connecting to your native account…",
    });
    return {
      n,
      controller,
      timer: setTimeout(() => controller.abort(), 15000),
    };
  }
  const current = (job) => !disposed && generation === job.n;
  const finish = (job) => {
    clearTimeout(job.timer);
    if (current(job)) active = null;
  };
  function validateRoster(value) {
    if (
      value.version !== 1 ||
      value.state !== "account_ready" ||
      value.admission_available !== false ||
      !/^\d+$/.test(value.account_id) ||
      !Array.isArray(value.avatars) ||
      value.avatars.length > 3 ||
      value.next !== null
    )
      throw Error("Unsupported account roster.");
    return value.avatars.map((v) => {
      if (
        typeof v.avatar_id !== "string" ||
        !/^[1-9]\d{0,7}$/.test(v.avatar_id) ||
        Number(v.avatar_id) > 16777215 ||
        typeof v.name !== "string" ||
        v.name.length > 24
      )
        throw Error("Invalid avatar record.");
      return { avatar_id: v.avatar_id, name: v.name };
    });
  }
  async function roster(job) {
    const v = await request(
      "roster",
      { after: "0" },
      token,
      job.controller.signal,
    );
    const avatars = validateRoster(v);
    if (current(job))
      emit({
        state: "account_ready",
        avatars,
        message:
          "Account connected. Native world admission is not yet available.",
      });
  }
  async function login(username, password) {
    if (token) return;
    const job = begin();
    if (!job) return;
    token = null;
    try {
      const v = await request(
        "login",
        { username, password },
        null,
        job.controller.signal,
      );
      password = "";
      if (!current(job)) return;
      if (
        v.version !== 1 ||
        v.state !== "account_ready" ||
        v.admission_available !== false ||
        typeof v.token !== "string" ||
        !/^[0-9a-f]{64}$/.test(v.token)
      )
        throw Error("Invalid account session.");
      token = v.token;
      await roster(job);
    } catch (error) {
      if (current(job)) {
        if (error.unauthorized) token = null;
        emit({
          state: token ? "account_error" : "signed_out",
          avatars: [],
          message:
            error.name === "AbortError"
              ? token
                ? "Connection timed out. Refresh your roster or sign out."
                : "Connection timed out. Please sign in again."
              : error.message,
        });
      }
    } finally {
      password = "";
      finish(job);
    }
  }
  async function refresh() {
    const job = begin();
    if (!job) return;
    if (!token) {
      finish(job);
      emit({
        state: "signed_out",
        avatars: [],
        message: "Sign in to access your account.",
      });
      return;
    }
    try {
      await roster(job);
    } catch (error) {
      if (current(job)) {
        if (error.unauthorized) token = null;
        emit({
          state: token ? "account_error" : "signed_out",
          avatars: [],
          message: error.message,
        });
      }
    } finally {
      finish(job);
    }
  }
  async function logout() {
    if (disposed) return;
    active?.abort();
    generation++;
    active = null;
    const secret = token;
    token = null;
    const job = begin();
    if (!job) return;
    try {
      if (secret)
        await request("logout", { all: false }, secret, job.controller.signal);
      if (current(job))
        emit({ state: "signed_out", avatars: [], message: "Signed out." });
    } catch {
      if (current(job))
        emit({
          state: "signed_out",
          avatars: [],
          message:
            "This tab is signed out. Server sign-out was not confirmed; the session expires automatically.",
        });
    } finally {
      finish(job);
    }
  }
  function dispose() {
    disposed = true;
    generation++;
    active?.abort();
    active = null;
    token = null;
  }
  emit({
    state: "signed_out",
    avatars: [],
    message: "Sign in to your native Wonderland account.",
  });
  return { login, refresh, logout, dispose };
}
