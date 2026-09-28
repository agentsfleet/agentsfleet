/**
 * The delivery states a thread row can be in.
 *
 * Shared because two files answer to them: the renderer decides which row shape
 * a message gets, and the reply body decides whether the fleet is still
 * talking. A second spelling of "received" in either place is a row that
 * silently stops streaming.
 */

import { AGENTSFLEET_EVENT_STATUS } from "@/lib/streaming/fleet-stream-row";

/** Sent by the operator, not yet acknowledged by the daemon. */
export const STATUS_OPTIMISTIC = AGENTSFLEET_EVENT_STATUS.OPTIMISTIC;
/** The fleet answered with an error rather than a reply. */
export const STATUS_AGENT_ERROR = AGENTSFLEET_EVENT_STATUS.AGENT_ERROR;
/** Accepted and in flight — the turn is still arriving. */
export const STATUS_IN_FLIGHT = AGENTSFLEET_EVENT_STATUS.RECEIVED;
