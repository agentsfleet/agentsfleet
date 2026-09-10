"use client";

import { useEffect } from "react";
import { cn, EYEBROW_CLASS, WakePulse } from "@agentsfleet/design-system";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import {
  useAbsorbWorkspaceCounters,
  useWorkspaceStream,
} from "@/components/domain/useWorkspaceStream";

/**
 * The wall's one honest answer to "is this page still telling me the truth?"
 *
 * The count of live fleets alone said "live" while the EventSource was still
 * opening, which is the thing an operator most needs not to be lied to about.
 * This reads the stream's real connection state instead.
 *
 * It also relays the one case the frames cannot answer. A settled row normally
 * carries its own charge, so the tile footers move with no request at all;
 * when a row arrives without one, `countersStale` bumps and the wall re-reads
 * just the fleet summaries. One badge per wall, so several tiles asking at
 * once still costs one read — and nothing is re-rendered on the server.
 */

const CONNECTING_COPY = "connecting…";
const RECONNECTING_COPY = "reconnecting…";
const OFFLINE_COPY = "offline";
const LIVE_SUFFIX = "live";

type Props = {
  liveTotal: number;
  /**
   * Re-read the fleet summaries because the frames could not price a settled
   * row, and answer with the ids that came back so the stream can drop the
   * rows the fresh base now accounts for.
   */
  onStaleCounters: () => Promise<readonly string[]>;
};

/** What the badge says, and whether its dot should pulse. */
function reading(
  connectionStatus: string,
  helloReceived: boolean,
  liveTotal: number,
): { text: string; live: boolean } | null {
  if (connectionStatus === CONNECTION_STATUS.RECONNECTING) {
    return { text: RECONNECTING_COPY, live: false };
  }
  if (connectionStatus === CONNECTION_STATUS.OFFLINE) {
    return { text: OFFLINE_COPY, live: false };
  }
  // Connected at the socket but no `hello` yet means the server has not said
  // which fleets it is streaming, so nothing here is confirmed.
  if (connectionStatus !== CONNECTION_STATUS.LIVE || !helloReceived) {
    return { text: CONNECTING_COPY, live: false };
  }
  if (liveTotal === 0) return null;
  return { text: `${liveTotal} ${LIVE_SUFFIX}`, live: true };
}

export default function WallLiveBadge({ liveTotal, onStaleCounters }: Props) {
  const { connectionStatus, helloReceived, countersStale } = useWorkspaceStream();
  const absorb = useAbsorbWorkspaceCounters();

  useEffect(() => {
    // Zero is the initial value: the page was just rendered from the server,
    // so nothing is stale yet and a read here would be pure waste.
    if (countersStale === 0) return;
    let live = true;
    void onStaleCounters().then((ids) => {
      // Absorbing after an unmount would mutate a store this wall no longer
      // renders, and absorbing a stale answer would drop rows the newer read
      // did not cover.
      if (live) absorb(ids);
    });
    return () => {
      live = false;
    };
  }, [countersStale, onStaleCounters, absorb]);

  const shown = reading(connectionStatus, helloReceived, liveTotal);
  if (shown === null) return null;
  return (
    <span
      className={cn(EYEBROW_CLASS, "text-muted-foreground inline-flex items-center gap-2")}
      aria-label={shown.text}
    >
      <WakePulse
        live={shown.live}
        className="inline-block w-2 h-2 rounded-full bg-pulse"
        aria-hidden="true"
      />
      {shown.text}
    </span>
  );
}
