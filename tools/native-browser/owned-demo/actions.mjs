// Receipt routing only. The native host validates requests; the coordinator
// decides durable proposals. No action is accepted from this helper alone.
export async function settlePlayerAction(proposal, { commit, receipt }) {
  if (proposal.kind === "denied") {
    if (typeof proposal.receipt_hex !== "string")
      throw Error("Native action could not be decoded.");
    receipt(proposal.receipt_hex);
    return proposal;
  }
  if (proposal.kind !== "proposal")
    throw Error("Invalid native action response.");
  const decided = await commit(proposal);
  if (
    !["committed", "rejected"].includes(decided.kind) ||
    typeof decided.receipt_hex !== "string"
  )
    throw Error("Native action decision has no receipt.");
  receipt(decided.receipt_hex);
  return decided;
}
