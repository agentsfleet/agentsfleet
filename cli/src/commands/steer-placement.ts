// Where the daemon placed a steered message, read off the 202 that accepted it.

import { Schema } from "effect";

export const DISPOSITION_STARTED = "started" as const;
export const DISPOSITION_FOLDED = "folded" as const;
export const DISPOSITION_QUEUED = "queued" as const;

// A daemon that predates `disposition` answers without it, and every message
// it took started a run of its own, so an absent value is read as `started`
// rather than failing a send the daemon already holds. It stays absent in the
// decoded value: `--json` output for such a daemon is a pinned shape.
const MessagesResponseSchema = Schema.Struct({
  event_id: Schema.optionalKey(Schema.String),
  disposition: Schema.optionalKey(Schema.Literals([DISPOSITION_STARTED, DISPOSITION_FOLDED, DISPOSITION_QUEUED])),
  folded_into_event_id: Schema.optionalKey(Schema.String),
});

export type MessagesResponse = typeof MessagesResponseSchema.Type;

export const decodeMessagesResponse = (value: unknown) =>
  Schema.decodeUnknownEffect(MessagesResponseSchema)(value);
