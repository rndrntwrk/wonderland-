// Private supervisor status only; full inspection remains the checkpoint API.
const record = value => value !== null && typeof value === "object" && !Array.isArray(value);
const digest = value => typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
const decimal = (value, zero = false) => typeof value === "string"
  && /^(0|[1-9][0-9]{0,19})$/.test(value)
  && (zero || value !== "0") && BigInt(value) <= 18446744073709551615n;
const count = value => Number.isSafeInteger(value) && value >= 0;

export async function currentPreparationStatus(host) {
  const current = await host.ask({kind: "inspect_status"});
  const binding = current?.binding, cursor = current?.cursor;
  if (current?.kind !== "committed_status" || current.recovery_pending !== false
    || !record(binding) || binding.protocol !== 1
    || typeof binding.source_pin !== "string" || !binding.source_pin.trim()
    || !digest(binding.binding_sha256) || !decimal(binding.lot_incarnation)
    || !record(cursor) || !decimal(cursor.lot_id) || !decimal(cursor.authority_epoch)
    || !decimal(cursor.completed_tick, true) || !digest(cursor.state_hash)
    || !count(current.retained_receipts) || !count(current.retained_receipt_bytes))
    throw Error("Native host preparation status is unavailable or not committed.");
  return current;
}
