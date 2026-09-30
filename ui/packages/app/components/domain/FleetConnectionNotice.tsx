"use client";

import { Alert, Button } from "@agentsfleet/design-system";

import { CONNECTION_STATUS, type ConnectionStatus } from "./useFleetEventStream";

const OFFLINE_MESSAGE = "Live updates are temporarily unavailable. We’re reconnecting automatically.";
const RECONNECT_LABEL = "Retry now";

export function FleetConnectionNotice({
  status,
  onRetry,
}: {
  status: ConnectionStatus;
  onRetry: () => void;
}) {
  if (status !== CONNECTION_STATUS.OFFLINE) return null;
  // A warning, not an error: the stream heals itself and nothing was lost.
  // `Alert` announces a warning as `alert`, so the change is still spoken.
  return (
    <Alert
      variant="warning"
      data-testid="fleet-connection-notice"
      className="mb-sm flex w-full items-center justify-between gap-md rounded-md px-lg py-sm"
    >
      <span>{OFFLINE_MESSAGE}</span>
      <Button type="button" size="sm" variant="outline" onClick={onRetry}>
        {RECONNECT_LABEL}
      </Button>
    </Alert>
  );
}
