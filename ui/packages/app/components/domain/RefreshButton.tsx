"use client";

import { useCallback, useEffect, useRef, useState, useTransition } from "react";
import { CheckIcon, RefreshCwIcon } from "lucide-react";
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
 * from a dead one: this spins while the read is in flight and turns to a tick
 * afterwards.
 *
 * The acknowledgement never occupies layout. It used to sit beside the button
 * as the word "Refreshed", which widened the control the moment the read landed
 * — in a wrapping action bar that pushed the button onto a second row and back
 * again two seconds later. The proof of a refresh must not move the thing you
 * just clicked, so the word is announced to a screen reader and the icon
 * carries it visually, inside a button that is a fixed square either way.
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
    <>
      {/* Always mounted, so the announcement lands when the text arrives — a
          live region added at the same moment as its content is read
          unreliably, and often not at all. */}
      <output className="sr-only" data-testid="refresh-ack">
        {refreshed ? REFRESHED_LABEL : ""}
      </output>
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
        {refreshed ? (
          <CheckIcon aria-hidden="true" className="text-success" />
        ) : (
          <RefreshCwIcon aria-hidden="true" className={cn(refreshing && "animate-spin")} />
        )}
      </TooltipButton>
    </>
  );
}
