"use client";

import { useCallback, useEffect, useRef, useState, useTransition } from "react";
import { RefreshCwIcon } from "lucide-react";
import { TooltipButton, cn } from "@agentsfleet/design-system";

/** How long the acknowledgement stands before it fades out. */
const ACK_MS = 2000;

const REFRESH_LABEL = "Refresh";
const REFRESHING_LABEL = "Refreshing…";
const REFRESHED_LABEL = "Refreshed";

/**
 * A manual re-read, with proof that it happened.
 *
 * Every surface here prefers this to polling — the operator decides when a page
 * is stale, and a timer spends requests to learn nothing. The catch is that a
 * re-read is usually faster than the eye, so a bare button is indistinguishable
 * from a dead one: this spins while the read is in flight and says "Refreshed"
 * afterwards.
 *
 * `onRefresh` may be synchronous (a router refresh) or return a promise (a
 * client-side read); either way the spinner covers the whole transition.
 */
export function RefreshButton({
  onRefresh,
  label = REFRESH_LABEL,
}: {
  onRefresh: () => void | Promise<void>;
  label?: string;
}) {
  const [refreshing, startRefresh] = useTransition();
  const [refreshed, setRefreshed] = useState(false);
  const acknowledged = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (acknowledged.current !== null) clearTimeout(acknowledged.current);
    },
    [],
  );

  const refresh = useCallback(() => {
    setRefreshed(false);
    startRefresh(async () => {
      await onRefresh();
      setRefreshed(true);
      if (acknowledged.current !== null) clearTimeout(acknowledged.current);
      acknowledged.current = setTimeout(() => setRefreshed(false), ACK_MS);
    });
  }, [onRefresh]);

  return (
    <span className="flex items-center gap-md">
      {refreshed ? (
        <output className="text-label text-text-subtle">{REFRESHED_LABEL}</output>
      ) : null}
      <TooltipButton
        size="sm"
        variant="outline"
        className="aspect-square px-0"
        aria-label={refreshing ? REFRESHING_LABEL : label}
        tooltip={label}
        disabled={refreshing}
        aria-busy={refreshing}
        onClick={refresh}
      >
        <RefreshCwIcon aria-hidden="true" className={cn(refreshing && "animate-spin")} />
      </TooltipButton>
    </span>
  );
}
