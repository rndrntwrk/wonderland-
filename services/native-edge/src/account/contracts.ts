import { decimal, fields, hash } from "../lot/contracts";
export function username(value: unknown): string {
  if (
    typeof value !== "string" ||
    !/^[A-Za-z0-9][A-Za-z0-9_]{2,23}$/.test(value)
  )
    throw Error("invalid username");
  return value.toLowerCase();
}
export function operation(value: unknown): string {
  if (typeof value !== "string" || !/^[a-zA-Z0-9_-]{16,96}$/.test(value))
    throw Error("invalid operation identity");
  return value;
}
export function token(value: unknown): string {
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value))
    throw Error("unauthorized");
  return value;
}
export function avatar(value: unknown) {
  const v = fields(value, [
    "name",
    "description",
    "gender",
    "skin",
    "head_key",
    "body_key",
    "release",
  ]);
  const name = v.name;
  if (
    typeof name !== "string" ||
    !/^[a-zA-Z][a-zA-Z ]{2,23}$/.test(name) ||
    typeof v.description !== "string" ||
    !/^[\x20-\x7e\t\n\v\f\r\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]{0,499}$/.test(
      v.description,
    ) ||
    !["male", "female"].includes(v.gender as string) ||
    !["light", "medium", "dark"].includes(v.skin as string)
  )
    throw Error("invalid avatar specification");
  return {
    name,
    description: v.description as string,
    gender: v.gender as string,
    skin: v.skin as string,
    head_key: decimal(v.head_key),
    body_key: decimal(v.body_key),
    release: hash(v.release),
  };
}
export interface CatalogEntry {
  key: string;
  part: "head" | "body";
  gender: string;
  skins: string[];
}
export function catalog(value: unknown) {
  const v = fields(value, ["release", "entries"]);
  if (
    !Array.isArray(v.entries) ||
    v.entries.length < 2 ||
    v.entries.length > 16384
  )
    throw Error("catalog bounds");
  const seen = new Set<string>();
  const entries: CatalogEntry[] = Array.from(v.entries).map((raw) => {
    const e = fields(raw, ["key", "part", "gender", "skins"]),
      key = decimal(e.key);
    if (
      seen.has(key) ||
      !["head", "body"].includes(e.part as string) ||
      !["male", "female"].includes(e.gender as string) ||
      !Array.isArray(e.skins) ||
      !e.skins.length ||
      e.skins.length > 3 ||
      new Set(e.skins).size !== e.skins.length ||
      Array.from(e.skins).some((s) => !["light", "medium", "dark"].includes(s))
    )
      throw Error("invalid catalog entry");
    seen.add(key);
    return {
      key,
      part: e.part as "head" | "body",
      gender: e.gender as string,
      skins: e.skins as string[],
    };
  });
  return { release: hash(v.release), entries };
}
