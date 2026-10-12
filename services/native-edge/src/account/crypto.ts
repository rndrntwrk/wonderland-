// The binary is a static Worker module, never dynamically downloaded or compiled.
import wasm from "../../generated/account_crypto_bg.wasm";
import {
  initSync,
  password_hash,
  password_matches,
} from "../../generated/account_crypto.js";
initSync({ module: wasm });
export const PROFILE = "argon2id-v19-m19456-t2-p1-l32";
export const hex = (bytes: Uint8Array) =>
  [...bytes].map((v) => v.toString(16).padStart(2, "0")).join("");
export const unhex = (value: string) =>
  Uint8Array.from(value.match(/../g) || [], (v) => parseInt(v, 16));
export function password(value: unknown): Uint8Array {
  if (typeof value !== "string" || value.length > 1024)
    throw Error("invalid password");
  const bytes = new TextEncoder().encode(value);
  if ([...value].length < 15 || bytes.length > 1024)
    throw Error(
      "password must contain at least 15 characters and at most 1024 UTF-8 bytes",
    );
  return bytes;
}
export function createVerifier(bytes: Uint8Array) {
  const salt = crypto.getRandomValues(new Uint8Array(16));
  try {
    return {
      profile: PROFILE,
      salt: hex(salt),
      hash: hex(password_hash(bytes, salt)),
    };
  } finally {
    bytes.fill(0);
  }
}
export function matches(
  bytes: Uint8Array,
  record: { profile: string; salt: string; hash: string },
) {
  try {
    if (
      record.profile !== PROFILE ||
      !/^[0-9a-f]{32}$/.test(record.salt) ||
      !/^[0-9a-f]{64}$/.test(record.hash)
    )
      throw Error("unsupported verifier");
    return password_matches(bytes, unhex(record.salt), unhex(record.hash));
  } finally {
    bytes.fill(0);
  }
}
export const randomToken = () =>
  hex(crypto.getRandomValues(new Uint8Array(32)));
