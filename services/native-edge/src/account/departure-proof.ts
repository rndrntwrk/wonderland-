/** Private account-to-lot receipt lookup. No caller-selected URL or namespace. */
import { digest, encode, equal } from "../lot/contracts";
import {
  committedAvatarDetachV1,
  committedAvatarSaveV1,
  detachProofReferenceV1,
  saveProofReferenceV1,
  saveReference,
  type DetachProofReferenceV1,
  type SaveProofReferenceV1,
  type TrustedDepartureReceiptReader,
} from "./lot-save";

export interface DepartureReceiptStub {
  lookupCommittedAvatarSaveV1(
    reference: SaveProofReferenceV1,
  ): Promise<unknown>;
  lookupCommittedAvatarDetachV1(
    reference: DetachProofReferenceV1,
  ): Promise<unknown>;
}
export interface DepartureReceiptEnv {
  COORD?: {
    idFromName(name: string): DurableObjectId;
    get(id: DurableObjectId): DepartureReceiptStub;
  };
}

/** Existing private runner maps coordinator objects by canonical decimal lot ID.
 * An absent binding leaves ordinary account operations available, but every
 * departure persistence/release attempt fails closed. Configuration never comes
 * from the proof request or a browser session.
 */
export function configuredDepartureReceiptReader(
  env: DepartureReceiptEnv,
): TrustedDepartureReceiptReader {
  function lot(id: string): DepartureReceiptStub {
    if (!env.COORD) throw Error("trusted departure coordinator unavailable");
    return env.COORD.get(env.COORD.idFromName(id));
  }
  return {
    async verifyCommittedSave(raw) {
      const reference = saveProofReferenceV1(raw);
      const receipt = committedAvatarSaveV1(
        await lot(reference.identity.lot_id).lookupCommittedAvatarSaveV1(
          reference,
        ),
      );
      if (
        !equal(saveReference(receipt), reference) ||
        (await digest(encode(receipt.save_record))) !== reference.save_sha256
      )
        throw Error("authoritative source save proof mismatch");
      return receipt;
    },
    async verifyCommittedDetach(raw) {
      const reference = detachProofReferenceV1(raw);
      const receipt = committedAvatarDetachV1(
        await lot(reference.identity.lot_id).lookupCommittedAvatarDetachV1(
          reference,
        ),
      );
      if (
        !equal(receipt.identity, reference.identity) ||
        receipt.departure_operation !== reference.departure_operation ||
        !equal(receipt.origin, reference.origin) ||
        (await digest(encode(receipt))) !== reference.detach_sha256
      )
        throw Error("authoritative actor detach proof mismatch");
      return receipt;
    },
  };
}
