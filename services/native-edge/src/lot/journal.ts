import { equal, type Binding, type Cursor, type Payload } from "./contracts";

/** Select a bounded, exact predecessor chain from immutable accepted rows. */
export function recoveryChain(
  binding: Binding,
  checkpoint: Cursor,
  target: Cursor,
  candidates: Payload[],
): Payload[] {
  let cursor = checkpoint,
    frames = 0,
    bytes = 0;
  const chain: Payload[] = [];
  while (!equal(cursor, target)) {
    if (
      cursor.lot_id !== target.lot_id ||
      cursor.authority_epoch !== target.authority_epoch ||
      BigInt(cursor.completed_tick) >= BigInt(target.completed_tick)
    )
      throw Error("checkpoint cannot reach committed cursor");
    const next = candidates.filter(
      (p) => equal(p.binding, binding) && equal(p.predecessor, cursor),
    );
    if (next.length !== 1) throw Error("accepted journal gap or fork");
    const p = next[0];
    const advance =
      BigInt(p.result.completed_tick) - BigInt(p.predecessor.completed_tick);
    if (advance < 1n || advance > 16n)
      throw Error("invalid accepted tick advancement");
    frames += Number(advance);
    bytes += p.ticks_hex.length / 2;
    if (frames > 64 || bytes > 8 * 1024 * 1024)
      throw Error("recovery tail exceeds admitted profile");
    if (BigInt(p.result.completed_tick) > BigInt(target.completed_tick))
      throw Error("accepted journal overshoots committed cursor");
    chain.push(p);
    cursor = p.result;
  }
  return chain;
}
