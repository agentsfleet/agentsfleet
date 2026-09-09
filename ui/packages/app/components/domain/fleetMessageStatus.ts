/**
 * The delivery states a thread row can be in.
 *
 * Shared because two files answer to them: the renderer decides which row shape
 * a message gets, and the reply body decides whether the fleet is still
 * talking. A second spelling of "received" in either place is a row that
 * silently stops streaming.
 */

/** Sent by the operator, not yet acknowledged by the daemon. */
export const STATUS_OPTIMISTIC = "optimistic";
/** The send itself failed; nothing reached the daemon. */
export const STATUS_FAILED = "failed";
/** The fleet answered with an error rather than a reply. */
export const STATUS_AGENT_ERROR = "fleet_error";
/** Accepted and in flight — the turn is still arriving. */
export const STATUS_IN_FLIGHT = "received";
