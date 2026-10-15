// Bounded private Rust-process transport. No pipe is exposed over HTTP/WebSocket.
import { spawn } from "node:child_process";
export async function openOwnedHost(executable, config) {
  const child = spawn(executable, [config], {
    stdio: ["pipe", "pipe", "pipe"],
  });
  const pending = [];
  let buffered = "",
    ended = false,
    diagnostic = "";
  const rejectAll = (error) => {
    ended = true;
    for (const p of pending.splice(0)) {
      clearTimeout(p.timer);
      p.reject(error);
    }
  };
  child.stderr.on("data", (b) => {
    if (diagnostic.length < 8192)
      diagnostic += b.toString().slice(0, 8192 - diagnostic.length);
  });
  child.on("error", rejectAll);
  child.on("exit", () => rejectAll(Error("Native host exited.")));
  child.stdout.on("data", (bytes) => {
    buffered += bytes.toString();
    if (buffered.length > 96 * 1024 * 1024) {
      rejectAll(Error("Native host output exceeded its bound."));
      child.kill("SIGKILL");
      return;
    }
    for (;;) {
      const at = buffered.indexOf("\n");
      if (at < 0) break;
      const line = buffered.slice(0, at);
      buffered = buffered.slice(at + 1);
      const waiter = pending.shift();
      if (!waiter) {
        rejectAll(Error("Unsolicited native host reply."));
        child.kill("SIGKILL");
        return;
      }
      clearTimeout(waiter.timer);
      try {
        waiter.resolve(JSON.parse(line));
      } catch {
        waiter.reject(Error("Invalid native host reply."));
      }
    }
  });
  function next() {
    return new Promise((resolve, reject) => {
      if (ended) {
        reject(Error("Native host is closed."));
        return;
      }
      if (pending.length >= 2) {
        reject(Error("Native host request is already pending."));
        return;
      }
      const waiter = { resolve, reject, timer: null };
      waiter.timer = setTimeout(() => {
        rejectAll(Error("Native host reply timed out."));
        child.kill("SIGKILL");
      }, 30000);
      pending.push(waiter);
    });
  }
  const initial = await next();
  return {
    initial,
    diagnostic: () => diagnostic,
    async ask(value) {
      const encoded = JSON.stringify(value);
      if (Buffer.byteLength(encoded) > 1024 * 1024)
        throw Error("Native host request exceeded its bound.");
      const reply = next();
      child.stdin.write(encoded + "\n");
      return reply;
    },
    async close(signal = "SIGTERM") {
      if (ended) return;
      const exited = new Promise((resolve) => child.once("exit", resolve));
      child.kill(signal);
      const timeout = setTimeout(() => child.kill("SIGKILL"), 5000);
      try {
        await exited;
      } finally {
        clearTimeout(timeout);
      }
    },
  };
}
