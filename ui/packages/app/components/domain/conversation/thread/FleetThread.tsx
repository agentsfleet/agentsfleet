"use client";

import type { FleetSources } from "@/lib/fleet-sources";
import type { Schedule } from "@/lib/api/draft-types";

import { useCallback, useEffect, useMemo, useRef, type ReactNode } from "react";
import { ACTIVITY_KIND, FLEET_ACTIVITY, readFleetActivity } from "@/lib/events/fleet-activity";
import {
  AssistantRuntimeProvider,
  useExternalStoreRuntime,
  type AssistantRuntime,
} from "@assistant-ui/react";
import {
  DashboardPanel,
} from "@agentsfleet/design-system";
import {
  CONNECTION_STATUS,
  useFleetEventStream,
} from "../stream/useFleetEventStream";
import { useFleetThreadEntries } from "./useFleetThreadEntries";
import { mergePreviewEntries, useFleetUploadPreviews, type FleetConversationEntry } from "../composer/useFleetUploadPreviews";
import type { EventRow } from "@/lib/api/events";
import { SenderLabelProvider } from "../../activity/messages/FleetMessageRow";
import { SenderNamesProvider } from "../../shared/identity/SenderNames";
import { senderNamesFrom, type SenderName } from "@/lib/events/sender-names";
import { FleetConnectionIndicator, useArrivalCue } from "../chrome/FleetConnectionIndicator";
import { useFleetPendingSends } from "../delivery/useFleetPendingSends";
import { useCurrentUser } from "@/lib/auth/client";
import { ACTOR } from "@/lib/events/event-summary";
import { useMessageDelivery, useOpenFleet } from "../delivery/useFleetMessageDelivery";
import { useFleetSteerQueue } from "../delivery/useFleetSteerQueue";
import { reportsOwnRun } from "../../activity/replies/fleetReplyMessage";
import { FleetConversationLayout } from "../../shared/summary/FleetWorkSidebar";
import { FleetThreadViewport } from "./FleetThreadViewport";
import { FleetScopeProvider } from "../stream/FleetScope";
import { FleetTaskProvider } from "../../tasks/FleetTaskProvider";
import { TaskPanel } from "../../tasks/TaskPanel";
import { FLEET_CHAT_CONFIG } from "../../activity/tools/FleetToolRegistry";

export type FleetThreadProps = {
  archived?: boolean;
  schedules?: readonly Schedule[];
  approvals?: {count: number; href: string};
  sources?: FleetSources;
  modelLabel?: string;
  setupRevision?: string;
  setup?: ReactNode;
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
  /** The fleet's account members a thread can name, by the actor each one's
   * messages record; the server read them beside the thread. */
  senderNames: readonly SenderName[];
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
export function FleetThread(props: FleetThreadProps) {
  const identity = useCurrentUser();
  const subject = identity.isLoaded ? identity.userId : identity.userId ?? props.viewer;
  return <OwnedFleetThread key={JSON.stringify([props.workspaceId, props.fleetId, subject])} {...props} />;
}

function OwnedFleetThread(props: FleetThreadProps) {
  const {stream, arrived, names, ledger, delivery, runtime, setupEventId, subject, previewCount, files} = useThreadRuntime(props);
  const settledLive = stream.connectionStatus === CONNECTION_STATUS.LIVE && !arrived;
  const taskSubject = stream.connectionStatus === CONNECTION_STATUS.REVOKED ? null : subject;
  const work = useMemo(() => <TaskPanel workspaceId={props.workspaceId} fleetId={props.fleetId}
    subject={taskSubject} revision={stream.tasksSeq} events={stream.events} sources={props.sources} readOnly={props.archived} />, [props.sources, props.workspaceId, props.fleetId, taskSubject, stream.tasksSeq, stream.events, props.archived]);
  return (
    <AssistantRuntimeProvider runtime={runtime} config={FLEET_CHAT_CONFIG}>
      <FleetScopeProvider workspaceId={props.workspaceId} fleetId={props.fleetId}>
        <SenderLabelProvider senderLabel={props.senderLabel}>
          <SenderNamesProvider names={names}>
            <DashboardPanel id="fleet-chat-transcript" padding="none"
              className="relative flex min-h-0 flex-1 flex-col overflow-clip rounded-none border-0 bg-background">
              {settledLive ? null : <div data-testid="fleet-chat-header"
                className="pointer-events-none absolute right-lg top-sm z-20 rounded-md border border-border bg-background px-sm py-xs shadow-sm">
                <FleetConnectionIndicator status={stream.connectionStatus} arrived={arrived} />
              </div>}
              <FleetTaskProvider files={files} workspaceId={props.workspaceId} fleetId={props.fleetId} subject={taskSubject} revision={stream.tasksSeq} events={stream.events} sources={props.sources} schedules={props.schedules} approvals={props.approvals} approvalRevision={stream.approvalsSeq} readOnly={props.archived}>
              <FleetConversationLayout work={work}><FleetThreadViewport modelLabel={props.modelLabel} setup={props.setup} setupEventId={setupEventId}
                archived={props.archived}
                eventsCount={stream.events.length + previewCount} connectionStatus={stream.connectionStatus}
                onRetry={stream.retryConnection} pending={ledger.pending} onResend={delivery.resend}
                onDismiss={ledger.writers.dismiss} onRestored={delivery.noteRestored} onDraft={delivery.noteDraft} /></FleetConversationLayout>
              </FleetTaskProvider>
            </DashboardPanel>
          </SenderNamesProvider>
        </SenderLabelProvider>
      </FleetScopeProvider>
    </AssistantRuntimeProvider>
  );
}

function useThreadRuntime({workspaceId, fleetId, senderLabel, initial, viewer, senderNames, setupRevision, archived = false}: FleetThreadProps) {
  const stream = useFleetEventStream(workspaceId, fleetId, initial);
  // Connection chrome disappears after its arrival cue settles.
  const arrived = useArrivalCue(stream.connectionStatus);
  // The signed-in identity scopes the durable pending-send ledger.
  const identity = useCurrentUser();
  const subject = identity.isLoaded ? identity.userId : identity.userId ?? viewer;
  const names = useMemo(() => senderNamesFrom(subject, senderNames), [subject, senderNames]);
  const ledger = useFleetPendingSends({ subject, workspaceId, fleetId });
  const openFleet = useOpenFleet(workspaceId, fleetId);
  // Stable registry methods keep delivery independent of streamed frames.
  const delivery = useMessageDelivery({
    workspaceId,
    fleetId,
    fleetName: senderLabel, openFleet,
    sentAs: subject === null ? undefined : `${ACTOR.STEER_PREFIX}${subject}`,
    appendOptimistic: stream.appendOptimistic,
    reconcileOptimistic: stream.reconcileOptimistic,
    discardOptimistic: stream.discardOptimistic,
    writers: ledger.writers,
  });
  // The runtime built below: its composer takes a refused draft back, and an upload reads what it sends.
  const runtimeRef = useRef<AssistantRuntime | null>(null);
  const previews = useFleetUploadPreviews({workspaceId, fleetId, subject, events: stream.events, onNew: delivery.onNew, runtime: runtimeRef});
  const { entries, convertEntry } = useFleetThreadEntries(stream.events, stream.convertEvent, previews.groupBreaks);
  const setupEventId = useMemo(() => stream.events.findLast(event => {
    if (event.status !== FLEET_ACTIVITY.status) return false;
    const activity = readFleetActivity(event.custom?.requestJson ?? "");
    return activity
      && activity.kind === ACTIVITY_KIND.setupSaved
      && activity.revision === setupRevision;
  })?.id, [stream.events, setupRevision]);
  const messages = useMemo(() => mergePreviewEntries(entries, previews.entries), [entries, previews.entries]);
  const convertMessage = useCallback((entry: FleetConversationEntry) => entry.kind === "upload-preview" ? entry.message : convertEntry(entry), [convertEntry]);
  const adapters = useMemo(() => ({attachments: previews.attachments}), [previews.attachments]);
  // Queueing permits another send while this viewer's newest reply runs.
  const queue = useFleetSteerQueue(previews.onNew, runtimeRef);
  const isRunning = useMemo(() => reportsOwnRun(stream.events, subject), [stream.events, subject]);
  const adapter = useMemo(
    () => ({ isDisabled: archived, isRunning, messages, convertMessage, onNew: previews.onNew, queue, adapters }),
    [archived, isRunning, messages, convertMessage, previews.onNew, queue, adapters],
  );
  const runtime = useExternalStoreRuntime<FleetConversationEntry>(adapter);
  useEffect(() => {
    runtimeRef.current = runtime;
    return () => { runtimeRef.current = null; };
  }, [runtime]);
  return {stream, arrived, names, ledger, delivery, runtime, setupEventId, subject, previewCount: previews.entries.length, files: previews.files};
}
