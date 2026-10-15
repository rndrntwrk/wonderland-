import { decimal, fields } from "./contracts";
/** Staged private guard state. Parsing it never authorizes an actor or a write. */
export function controlStateV2(raw: unknown) {
  const value = fields(raw, ["schema_version", "mode", "revision"]);
  if (value.schema_version !== 2 || value.mode !== "controlled_v2")
    throw Error("unsupported controlled lot state");
  return {
    schema_version: 2 as const,
    mode: "controlled_v2" as const,
    revision: decimal(value.revision),
  };
}
