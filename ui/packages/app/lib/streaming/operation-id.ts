import { v7 } from "uuid";

// A steer's operation id: the caller's own name for one send, repeated on every
// retry so the daemon admits it once (`afd_wire::event::SteerRequest`). Minted
// here, in the browser, because only the sender can tell a retried POST from a
// person pressing Send twice — the bytes on the wire are identical.
//
// A UUID v7 — the shape the daemon mints its own row ids in (`afd_core::id::
// Uuid7`) — from the `uuid` package the command line already depends on. It
// orders ids by send time and keeps them strictly increasing within a document,
// and draws its random bits from `crypto.getRandomValues`, which every origin
// has. A platform with no generator is a refusal the caller turns into a
// returned draft, never a message sent without an identity.

/** No generator on this platform: the send cannot be named, so it is not made. */
export class MintUnavailable extends Error {
  constructor(options?: ErrorOptions) {
    super("no cryptographic generator is available to mint an operation id", options);
    this.name = "MintUnavailable";
  }
}

/** A fresh UUID v7, or a thrown `MintUnavailable`. */
export function mintOperationId(): string {
  try {
    return v7();
  } catch (cause) {
    throw new MintUnavailable({ cause });
  }
}
