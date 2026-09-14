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
  return (
    <Alert
      variant="destructive"
      data-testid="fleet-connection-notice"
      className="mx-xl my-md flex items-center justify-between gap-md rounded-md px-lg py-sm"
    >
      <span>{OFFLINE_MESSAGE}</span>
      <Button type="button" size="sm" variant="outline" onClick={onRetry}>
        {RECONNECT_LABEL}
      </Button>
    </Alert>
  );
}
