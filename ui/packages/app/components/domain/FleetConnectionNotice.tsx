"use client";

import { Alert, Button } from "@agentsfleet/design-system";

import { CONNECTION_STATUS, type ConnectionStatus } from "./useFleetEventStream";

const OFFLINE_MESSAGE = "Live updates are temporarily unavailable. We’re reconnecting automatically.";
const RECONNECT_LABEL = "Retry now";
/** What a person removed from the account reads where the stream's state shows. */
export const ACCESS_REVOKED_MESSAGE = "You no longer have access to this workspace.";
const NOTICE_TEST_ID = "fleet-connection-notice";
const NOTICE_CLASS = "mb-sm flex w-full items-center justify-between gap-md rounded-md px-lg py-sm";

export function FleetConnectionNotice({
  status,
  onRetry,
}: {
  status: ConnectionStatus;
  onRetry: () => void;
}) {
  // Lost access does not heal and a retry is refused the same way, so this
  // band offers no way back and does not share the self-healing warning's tone.
  if (status === CONNECTION_STATUS.REVOKED) {
    return (
      <Alert variant="destructive" data-testid={NOTICE_TEST_ID} className={NOTICE_CLASS}>
        {ACCESS_REVOKED_MESSAGE}
      </Alert>
    );
  }
  if (status !== CONNECTION_STATUS.OFFLINE) return null;
  // A warning, not an error: the stream heals itself and nothing was lost.
  // `Alert` announces a warning as `alert`, so the change is still spoken.
  return (
    <Alert variant="warning" data-testid={NOTICE_TEST_ID} className={NOTICE_CLASS}>
      <span>{OFFLINE_MESSAGE}</span>
      <Button type="button" size="sm" variant="outline" onClick={onRetry}>
        {RECONNECT_LABEL}
      </Button>
    </Alert>
  );
}
