"use client";

import { useEffect, useRef, useState } from "react";
import { Alert, Button } from "@agentsfleet/design-system";
import {
  CONNECTION_STATUS,
  type ConnectionStatus,
} from "./useFleetEventStream";

const RESTORED_NOTICE_MS = 4_000;
const NOTICE_CLASS_NAME = "mx-xl mb-md";

export function FleetConnectionNotice({
  status,
  onRetry,
}: {
  status: ConnectionStatus;
  onRetry: () => void;
}) {
  const recoveryPending = useRef(
    status === CONNECTION_STATUS.RECONNECTING || status === CONNECTION_STATUS.OFFLINE,
  );
  const [restored, setRestored] = useState(false);

  useEffect(() => {
    if (status === CONNECTION_STATUS.RECONNECTING || status === CONNECTION_STATUS.OFFLINE) {
      recoveryPending.current = true;
      setRestored(false);
      return;
    }
    if (status === CONNECTION_STATUS.CONNECTING) {
      setRestored(false);
      return;
    }
    if (!recoveryPending.current) {
      return;
    }
    recoveryPending.current = false;
    setRestored(true);
    const timer = setTimeout(() => setRestored(false), RESTORED_NOTICE_MS);
    return () => clearTimeout(timer);
  }, [status]);

  if (status === CONNECTION_STATUS.LIVE) {
    return restored ? (
      <Alert variant="success" className={NOTICE_CLASS_NAME}>
        Live connection restored.
      </Alert>
    ) : null;
  }
  if (status === CONNECTION_STATUS.CONNECTING) {
    return (
      <Alert variant="info" className={NOTICE_CLASS_NAME}>
        Connecting to live updates. Recent history remains available.
      </Alert>
    );
  }
  if (status === CONNECTION_STATUS.RECONNECTING) {
    return (
      <Alert variant="warning" className={NOTICE_CLASS_NAME}>
        Connection lost. Reconnecting… New messages will queue; history remains available.
      </Alert>
    );
  }
  return (
    <Alert
      variant="destructive"
      className={`${NOTICE_CLASS_NAME} items-center justify-between`}
    >
      <span>Live connection unavailable. History remains visible and new messages will queue.</span>
      <Button type="button" size="sm" variant="outline" onClick={onRetry}>
        Retry
      </Button>
    </Alert>
  );
}
