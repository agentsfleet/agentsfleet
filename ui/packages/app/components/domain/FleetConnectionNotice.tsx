"use client";

import { Alert, Button } from "@agentsfleet/design-system";

import { CONNECTION_STATUS, type ConnectionStatus } from "./useFleetEventStream";

// A band above the conversation is the loudest thing on this surface. It is
// spent only on a state that asks the operator for a decision.
//
// Connecting and reconnecting do not: they resolve on their own, usually
// faster than the sentence describing them can be read, and the header
// indicator already shows them with motion. They used to render a full-width
// alert that sat motionless and explained that history remained available —
// narrating our transport, and reassuring about something visible on screen.
// Reaching live used to render a fourth alert that lingered for four seconds;
// the indicator's arrival cue says it in one.
//
// Losing the connection is different. There is nothing to wait for and a
// choice to make, so it keeps its band and its Retry.

const OFFLINE_MESSAGE = "Live updates stopped. Retry any message that failed to send.";
const RETRY_LABEL = "Retry";

export function FleetConnectionNotice({
  status,
  onRetry,
}: {
  status: ConnectionStatus;
  onRetry: () => void;
}) {
  if (status !== CONNECTION_STATUS.OFFLINE) return null;
  return (
    <Alert
      variant="destructive"
      data-testid="fleet-connection-notice"
      className="mx-xl my-md flex items-center justify-between gap-md rounded-md px-lg py-sm"
    >
      <span>{OFFLINE_MESSAGE}</span>
      <Button type="button" size="sm" variant="outline" onClick={onRetry}>
        {RETRY_LABEL}
      </Button>
    </Alert>
  );
}
