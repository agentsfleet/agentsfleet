"use client";

import { cn, EYEBROW_CLASS, WakePulse } from "@agentsfleet/design-system";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";
import { useWorkspaceStream } from "@/components/domain/useWorkspaceStream";
import { ACCESS_REVOKED_LABEL } from "@/components/domain/FleetConnectionIndicator";

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

// Every state but one reads quietly beside the pulse-coloured dot. Lost access
// is an end rather than a pause, so it takes the destructive tone, dot and
// all, as FleetConnectionIndicator says it.
type Tone = { text: string; dot: string };
const QUIET_TONE: Tone = { text: "text-muted-foreground", dot: "bg-pulse" };
const REVOKED_TONE: Tone = { text: "text-destructive", dot: "bg-destructive" };

type Props = {
  liveTotal: number;
};

type Reading = { text: string; live: boolean; tone: Tone };

function quiet(text: string, live = false): Reading {
  return { text, live, tone: QUIET_TONE };
}

/** What the badge says, whether its dot should pulse, and in which tone. */
function reading(
  connectionStatus: string,
  helloReceived: boolean,
  liveTotal: number,
): Reading | null {
  // Terminal: the stream ended because the caller lost access to the workspace.
  if (connectionStatus === CONNECTION_STATUS.REVOKED) {
    return { text: ACCESS_REVOKED_LABEL, live: false, tone: REVOKED_TONE };
  }
  if (connectionStatus === CONNECTION_STATUS.RECONNECTING) return quiet(RECONNECTING_COPY);
  if (connectionStatus === CONNECTION_STATUS.OFFLINE) return quiet(OFFLINE_COPY);
  // Connected at the socket but no `hello` yet means the server has not said
  // which fleets it is streaming, so nothing here is confirmed.
  if (connectionStatus !== CONNECTION_STATUS.LIVE || !helloReceived) {
    return quiet(CONNECTING_COPY);
  }
  if (liveTotal === 0) return null;
  return quiet(`${liveTotal} ${LIVE_SUFFIX}`, true);
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
    <output className={cn(EYEBROW_CLASS, "inline-flex items-center gap-2", shown.tone.text)}>
      <WakePulse
        live={shown.live}
        className={cn("inline-block w-2 h-2 rounded-full", shown.tone.dot)}
        aria-hidden="true"
      />
      {shown.text}
    </output>
  );
}
