"use client";

import { cn, EYEBROW_CLASS, WakePulse } from "@agentsfleet/design-system";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import { useWorkspaceStream } from "@/components/domain/useWorkspaceStream";

/**
 * The wall's one honest answer to "is this page still telling me the truth?"
 *
 * The count of live fleets alone said "live" while the EventSource was still
 * opening, which is the thing an operator most needs not to be lied to about.
 * This reads the stream's real connection state instead. The tile footers
 * need no help from here: every frame carries the fleet's counters, so the
 * wall never has to re-read them.
 */

const CONNECTING_COPY = "connecting…";
const RECONNECTING_COPY = "reconnecting…";
const OFFLINE_COPY = "offline";
const LIVE_SUFFIX = "live";

type Props = {
  liveTotal: number;
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

export default function WallLiveBadge({ liveTotal }: Props) {
  const { connectionStatus, helloReceived } = useWorkspaceStream();
  const shown = reading(connectionStatus, helloReceived, liveTotal);
  if (shown === null) return null;
  return (
    /*
     * An `<output>`, and no `aria-label`.
     *
     * This carried `aria-label={shown.text}` on a bare <span>. ARIA gives a
     * plain span no role for a name to attach to, so the label was dropped —
     * and it duplicated the element's own visible text anyway, which is what
     * a reader gets from the flow regardless.
     *
     * The count is the thing that changes: fleets go live and drain while the
     * page sits open, driven by the workspace stream. `<output>` carries an
     * implicit `role="status"`, so that change announces politely instead of
     * updating silently — which is what the dropped label was reaching for —
     * and it is the tag `jsx-a11y(prefer-tag-over-role)` asks for over a span
     * wearing the role by hand.
     */
    <output className={cn(EYEBROW_CLASS, "text-muted-foreground inline-flex items-center gap-2")}>
      <WakePulse
        live={shown.live}
        className="inline-block w-2 h-2 rounded-full bg-pulse"
        aria-hidden="true"
      />
      {shown.text}
    </output>
  );
}
