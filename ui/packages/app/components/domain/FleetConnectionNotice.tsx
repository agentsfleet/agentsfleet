"use client";

import { Alert, Button } from "@agentsfleet/design-system";

import { CONNECTION_STATUS, type ConnectionStatus } from "./useFleetEventStream";

const OFFLINE_MESSAGE = "Live updates stopped. Reconnect to resume updates.";
const RECONNECT_LABEL = "Reconnect";

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
