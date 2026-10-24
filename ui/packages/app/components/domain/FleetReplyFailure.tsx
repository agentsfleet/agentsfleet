"use client";

import { memo, useMemo } from "react";
import { useAui, useAuiState, type MessageState } from "@assistant-ui/react";
import { useCurrentUser } from "@/lib/auth/client";
import { isSteerBy, parsePayload } from "@/lib/events/event-summary";
import { ChatFailure } from "./chat/ChatFailure";
import { messageOutcome } from "./fleetFailureCopy";
import { readFailureDetail, readFailureLabel, readReplyFigures, readRequestJson } from "./fleetMessageReaders";
import { FleetReplyFigures } from "./FleetReplyFigures";

/** Retry is a new user request through the same delivery queue as the composer. */
export const FleetReplyFailure = memo(function FleetReplyFailure({message}: {message: MessageState}) {
  const aui = useAui();
  const running = useAuiState(state => state.thread.isRunning);
  const {userId} = useCurrentUser();
  const actor = message.metadata.custom.requestActor;
  const json = readRequestJson(message);
  const request = useMemo(() => retryText(actor, json, userId), [actor, json, userId]);
  const retry = useMemo(() => request ? () => aui.thread().append(request) : undefined, [aui, request]);
  const figures = readReplyFigures(message);
  const reason = readFailureLabel(message);
  const detail = readFailureDetail(message);
  const outcome = messageOutcome(message);
  const description = outcome && outcome !== "The run failed."
    ? outcome : "Your fleet couldn’t complete this request. You can try again.";
  return <ChatFailure title="Couldn’t complete this request" description={description}
    note={figures.tokens === null || figures.costNanos === null ? "Final usage isn’t available for this request." : undefined}
    onRetry={retry} disabled={running}
    details={<>{reason ? <p>Reason: {reason}</p> : null}{detail ? <p>{detail}</p> : null}
      {!reason && !detail && Object.values(figures).every(value => value === null) ? <p>No further details were reported.</p> : null}
      <FleetReplyFigures figures={figures} /></>} />;
});

function retryText(actor: unknown, json: string | null, userId: string | null | undefined): string | undefined {
  if (typeof actor !== "string" || !isSteerBy(actor, userId ?? null)) return undefined;
  const text = parsePayload(json)?.message;
  return typeof text === "string" && text.trim() ? text : undefined;
}
