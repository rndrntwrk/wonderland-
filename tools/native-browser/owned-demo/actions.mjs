// Receipt routing only. The native host validates requests; the coordinator
// decides durable proposals. No action is accepted from this helper alone.
export async function settlePlayerAction(proposal, { commit, receipt }) {
  if (proposal.kind === "denied") {
    if (typeof proposal.receipt_hex !== "string")
      throw Error("Native action could not be decoded.");
    receipt(proposal.receipt_hex);
    return proposal;
  }
  if (!["proposal", "committed", "rejected"].includes(proposal.kind))
    throw Error("Invalid native action response.");
  // A same-input retry can be an exact terminal reply retrieved from durable
  // receipt custody. Re-deliver its correlation without committing it again.
  const decided =
    proposal.kind === "proposal" ? await commit(proposal) : proposal;
  if (
    !["committed", "rejected"].includes(decided.kind) ||
    typeof decided.receipt_hex !== "string"
  )
    throw Error("Native action decision has no receipt.");
  receipt(decided.receipt_hex);
  return decided;
}
