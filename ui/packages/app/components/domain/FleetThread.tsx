"use client";

import { useMemo, useRef } from "react";
import {
  AssistantRuntimeProvider,
  useExternalStoreRuntime,
  type AssistantRuntime,
} from "@assistant-ui/react";
import {
  DashboardPanel,
  DashboardPanelHeader,
} from "@agentsfleet/design-system";
import {
  CONNECTION_STATUS,
  useFleetEventStream,
} from "./useFleetEventStream";
import { useFleetThreadEntries, type FleetThreadEntry } from "./useFleetThreadEntries";
import type { EventRow } from "@/lib/api/events";
import { SenderLabelProvider } from "./FleetMessageRow";
import { FleetConnectionIndicator, useArrivalCue } from "./FleetConnectionIndicator";
import { useFleetPendingSends } from "./useFleetPendingSends";
import { useCurrentUser } from "@/lib/auth/client";
import { ACTOR } from "@/lib/events/event-summary";
import { useMessageDelivery } from "./useFleetMessageDelivery";
import { useFleetSteerQueue } from "./useFleetSteerQueue";
import { reportsOwnRun } from "./fleetReplyMessage";
import { FleetThreadViewport } from "./FleetThreadViewport";

export type FleetThreadProps = {
  workspaceId: string;
  fleetId: string;
  /** The console's own fleet — the name a fleet reply is labelled with. */
  senderLabel: string;
  /**
   * Server-rendered initial event rows. The browser holds no credential —
   * this data is fetched in the parent Server Component and passed as a
   * prop; live updates arrive over the cookie-authed SSE route handler.
   */
  initial: EventRow[];
  /**
   * The signed-in user as the server rendered the page. The pending-send
   * ledger is keyed by user, and the client learns the user only once its
   * auth script loads: a send made before then went to a ledger that vanished
   * when it did, taking its Resend with it.
   */
  viewer: string | null;
};

/**
 * Operator-facing chat surface backed by the durable event log. Wraps
 * `@assistant-ui/react` over `useFleetEventStream` + `postSteer` (the same-origin
 * `/live` steer route); `fleetMessageRenderers` paints each durable event as the
 * approved conversation row.
 *
 * The runtime is told the thread runs while the newest turn is one this tab
 * sent and its reply is still running, and every send goes through a queue (`useFleetSteerQueue`), so a run
 * never closes the composer: assistant-ui routes a send made mid-run to the
 * queue's steer lane instead of refusing it. That flag is what the viewport's
 * top anchor keys on to hold the viewer's place while their reply grows and
 * folds; any other sender's reply, another tab's included, leaves it off
 * (`reportsOwnRun`).
 */
export function FleetThread({
  workspaceId,
  fleetId,
  senderLabel,
  initial,
  viewer,
}: FleetThreadProps) {
  const stream = useFleetEventStream(workspaceId, fleetId, initial);
  // The header row is chrome that earns its space only while the stream is not
  // yet fine. `arrived` keeps it for the length of the arrival cue so the
  // operator who WAS waiting gets the confirmation, and then the row goes —
  // its disappearance being the steady-state signal that nothing is wrong.
  const arrived = useArrivalCue(stream.connectionStatus);
  const settledLive = stream.connectionStatus === CONNECTION_STATUS.LIVE && !arrived;
  // Every send this fleet has not heard back on, from submit to the 202 — and
  // what Resend and the notice work from. Module state with a storage mirror,
  // so it outlives this component and this document.
  // Keyed by the signed-in user as well, so the next person on a shared
  // browser never sees, or resends as themselves, what this one typed.
  const { userId } = useCurrentUser();
  const subject = userId ?? viewer;
  const ledger = useFleetPendingSends({ subject, workspaceId, fleetId });
  // Pass the registry methods (each `useCallback([fleetId])`-stable), not
  // the whole `stream` object — `stream` is a fresh reference on every SSE
  // frame, so listing it would rebuild `onNew` per frame for no benefit.
  const delivery = useMessageDelivery({
    workspaceId,
    fleetId,
    sentAs: subject === null ? undefined : `${ACTOR.STEER_PREFIX}${subject}`,
    appendOptimistic: stream.appendOptimistic,
    reconcileOptimistic: stream.reconcileOptimistic,
    discardOptimistic: stream.discardOptimistic,
    writers: ledger.writers,
  });
  // Runs of identical activity render as one expandable row. Grouping is a
  // pure view over the array the stream already ordered — it never reorders,
  // drops, or renames an event, so a group can always hand back what it hid.
  const { entries, convertEntry } = useFleetThreadEntries(stream.events, stream.convertEvent);
  // The runtime compares its adapter by identity: a fresh literal on a render
  // that changed no message (a connection cue, a ledger entry) re-ran every
  // assistant-ui selector in the thread for nothing.
  // The steer rides assistant-ui's queue, so a run can be reported as one
  // without closing the composer; the queue reaches the runtime through a ref
  // because the runtime is built from the adapter that holds the queue.
  const runtimeRef = useRef<AssistantRuntime | null>(null);
  const queue = useFleetSteerQueue(delivery.onNew, runtimeRef);
  // The newest turn only: assistant-ui anchors whatever turn is last while the
  // thread runs, so a teammate's turn landing under the viewer's own running
  // reply would otherwise be pinned to the top and pull the viewer off theirs.
  const isRunning = useMemo(() => reportsOwnRun(stream.events, subject), [stream.events, subject]);
  const adapter = useMemo(
    () => ({ isRunning, messages: entries, convertMessage: convertEntry, onNew: delivery.onNew, queue }),
    [isRunning, entries, convertEntry, delivery.onNew, queue],
  );
  const runtime = useExternalStoreRuntime<FleetThreadEntry>(adapter);
  runtimeRef.current = runtime;
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <SenderLabelProvider senderLabel={senderLabel}>
        <DashboardPanel
          id="fleet-chat-transcript"
          aria-label="Fleet chat"
          padding="none"
          className="flex min-h-0 flex-1 flex-col overflow-clip rounded-none border-0 bg-background"
        >
          {/*
            * The header speaks only when the stream is not fine.
            *
            * It carried the word "Chat" directly under a tab already reading
            * "Chat", and a steady "Live" that said nothing on the overwhelming
            * majority of loads. A transcript is the page's content; labelling
            * it costs a row and tells the operator what they can see.
            *
            * What is worth saying is the exception, so connecting, reconnecting
            * and offline still render here — and OFFLINE additionally gets the
            * notice above the composer, with its retry. `PANEL_TITLE` stays as
            * the scroll region's accessible name, where it is the only name
            * that region has.
            */}
          {settledLive ? null : (
            <DashboardPanelHeader
              data-testid="fleet-chat-header"
              className="shrink-0 border-b border-border px-lg py-md sm:px-xl"
            >
              <FleetConnectionIndicator status={stream.connectionStatus} arrived={arrived} />
            </DashboardPanelHeader>
          )}
          <FleetThreadViewport
            eventsCount={stream.events.length}
            connectionStatus={stream.connectionStatus}
            onRetry={stream.retryConnection}
            pending={ledger.pending}
            onResend={delivery.resend}
            onDismiss={ledger.writers.dismiss}
            onRestored={delivery.noteRestored}
            onDraft={delivery.noteDraft}
          />
        </DashboardPanel>
      </SenderLabelProvider>
    </AssistantRuntimeProvider>
  );
}
