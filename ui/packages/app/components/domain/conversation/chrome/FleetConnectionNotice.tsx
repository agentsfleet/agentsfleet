"use client";

import { useEffect, useId, useRef, useState, type RefObject } from "react";
import { Alert, Button } from "@agentsfleet/design-system";
import { WarningCircleIcon } from "@phosphor-icons/react/ssr";

import { CONNECTION_STATUS, type ConnectionStatus } from "../stream/useFleetTurnStream";

const OFFLINE_MESSAGE = "Live updates paused";
/** What a person removed from the account reads where the stream's state shows. */
export const ACCESS_REVOKED_MESSAGE = "You no longer have access to this workspace.";
const NOTICE_TEST_ID = "fleet-connection-notice";

export function FleetConnectionNotice({
  status,
  onRetry,
  focusTarget,
}: {
  status: ConnectionStatus;
  onRetry: () => void;
  focusTarget?: RefObject<HTMLElement | null>;
}) {
  const description = useId();
  const retryFocused = useRef(false);
  const interrupted = useRef(false);
  const [announcement, setAnnouncement] = useState("");
  const canRetry = status === CONNECTION_STATUS.OFFLINE;
  const reconnecting = status === CONNECTION_STATUS.RECONNECTING;
  useEffect(() => {
    if (canRetry || reconnecting) {
      interrupted.current = true;
      setAnnouncement(reconnecting ? "Reconnecting live updates." : OFFLINE_MESSAGE);
      return;
    }
    const restored = interrupted.current && status === CONNECTION_STATUS.LIVE;
    setAnnouncement(restored ? "Live updates restored." : "");
    const target = focusTarget?.current;
    if (restored && retryFocused.current && target && target.ownerDocument.activeElement === target.ownerDocument.body) {
      target.focus({preventScroll: true});
    }
    interrupted.current = false;
    retryFocused.current = false;
  }, [canRetry, reconnecting, status, focusTarget]);

  if (status === CONNECTION_STATUS.REVOKED) {
    return (
      <Alert variant="destructive" data-testid={NOTICE_TEST_ID} className="mb-sm flex w-full items-center gap-md rounded-md px-lg py-sm">
        {ACCESS_REVOKED_MESSAGE}
      </Alert>
    );
  }
  return <>
    <output className="sr-only">{announcement}</output>
    {canRetry || reconnecting ? <div className="flex justify-center pb-md">
      <div data-testid={NOTICE_TEST_ID} className="flex w-fit max-w-full items-center gap-sm rounded-md border border-border bg-muted pr-xs pl-lg text-eyebrow leading-eyebrow text-muted-foreground">
        <span className="flex min-w-0 items-center gap-sm">
          <WarningCircleIcon size={16} className="shrink-0 text-warning" aria-hidden="true" />
          <span id={description}>{OFFLINE_MESSAGE}</span>
        </span>
        <Button size="compact" variant="ghost" className="shrink-0 text-pulse hover:bg-accent aria-disabled:cursor-default aria-disabled:text-muted-foreground"
          aria-describedby={description} aria-disabled={reconnecting || undefined} onClick={() => {if (canRetry) onRetry();}}
          onFocus={() => {retryFocused.current = true;}} onBlur={() => {retryFocused.current = false;}}>
          {reconnecting ? "Reconnecting…" : "Reconnect"}
        </Button>
      </div>
    </div> : null}
  </>;
}
