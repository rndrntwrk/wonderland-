#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { createOwnedDemo } from "../owned-demo.mjs";
const args = process.argv.slice(2),
  options = {};
for (let i = 0; i < args.length; i += 2) {
  const key = args[i],
    value = args[i + 1];
  if (
    ![
      "--manifest",
      "--dist",
      "--host",
      "--storage",
      "--port",
      "--resume",
    ].includes(key) ||
    !value ||
    Object.hasOwn(options, key)
  )
    throw Error(
      "Usage: node tools/native-browser/owned-demo/run.mjs --manifest MANIFEST.json --dist apps/web-shell/dist [--host HOST] [--storage DIR] [--port 4173] [--resume true]",
    );
  options[key] = value;
}
if (!options["--manifest"] || !options["--dist"])
  throw Error(
    "A qualified source manifest and built native UI are required. No fixture actor fallback is available.",
  );
const manifest = JSON.parse(
  await readFile(resolve(options["--manifest"]), "utf8"),
);
const port = options["--port"] ? Number(options["--port"]) : 0;
if (!Number.isInteger(port) || port < 0 || port > 65535)
  throw Error("Invalid loopback port.");
if (options["--resume"] && !["true", "false"].includes(options["--resume"]))
  throw Error("Invalid resume flag.");
const resume = options["--resume"] === "true";
const demo = await createOwnedDemo({
  manifest,
  resume,
  dist: resolve(options["--dist"]),
  ...(options["--host"] ? { hostExecutable: resolve(options["--host"]) } : {}),
  ...(options["--storage"]
    ? { storageRoot: resolve(options["--storage"]) }
    : {}),
  port,
});
try {
  const accounts = resume ? undefined : await demo.setupAccounts();
  console.log(
    JSON.stringify(
      {
        origin: demo.origin,
        accounts,
        notice:
          "Loopback-only bounded test session. Presence preserves custody only within the original account idle deadline. Original-content gameplay and source detach are not yet qualified.",
      },
      null,
      2,
    ),
  );
  let closing = false;
  const close = async () => {
    if (closing) return;
    closing = true;
    await demo.close();
    process.exitCode = 0;
  };
  process.once("SIGINT", close);
  process.once("SIGTERM", close);
} catch (error) {
  await demo.close();
  throw error;
}
