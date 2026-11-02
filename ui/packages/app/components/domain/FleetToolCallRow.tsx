import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { lazy, memo, Suspense, useState } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, ListItem, SignalSpinner } from "@agentsfleet/design-system";
import { useFleetScope } from "./FleetScope";
import { readToolResult } from "./fleetReplyMessage";
import { cellState } from "./tool-call-copy";
import { ToolCallHeader } from "./FleetToolCallHeader";
import { ToolCallDetails } from "./FleetToolCallDetails";
import { TRIGGER_CLASS } from "./tool-call-presentation";

const COMMAND = "command";
const FleetToolOutputDialog = lazy(() => import("./FleetToolOutputDialog").then(module => ({default: module.FleetToolOutputDialog})));
const FULL_OUTPUT_LOADING = <output className="inline-flex items-center gap-xs text-label text-muted-foreground"><SignalSpinner className="text-pulse" aria-hidden="true" />Loading full command output…</output>;

export type ToolCallRowProps = {
  name: string;
  args: ToolArgs;
  argsText: string;
  result: unknown;
  running: boolean;
  settled: boolean;
  eventId: string;
};

export const ToolCallRow = memo(function ToolCallRow({name, args, argsText, result, running, settled, eventId}: ToolCallRowProps) {
  const named = Object.keys(args).length > 0 ? args : undefined;
  const outcome = readToolResult(result);
  const state = cellState(outcome !== undefined, outcome?.status, running);
  const [shown, setShown] = useState(false);
  const scope = useFleetScope();
  const callId = outcome?.callId;
  const canShowAll = scope !== null && callId !== undefined && settled;
  if (shown && !canShowAll) setShown(false);
  return <ListItem data-tool={name} data-done={outcome !== undefined || undefined} data-state={state}
    className="flex min-w-0 flex-col gap-xs text-body-sm text-text-subtle">
    <Accordion type="single" collapsible>
      <AccordionItem value={COMMAND} className="border-0">
        <AccordionTrigger className={TRIGGER_CLASS}>
          <ToolCallHeader name={name} args={named} state={state} />
        </AccordionTrigger>
        <AccordionContent className="pb-sm">
          <ToolCallDetails name={name} args={named} argsText={named === undefined ? undefined : argsText} outcome={outcome} state={state}
            onShowAll={canShowAll ? () => setShown(true) : undefined} />
        </AccordionContent>
      </AccordionItem>
    </Accordion>
    {shown && canShowAll ? <Suspense fallback={FULL_OUTPUT_LOADING}><FleetToolOutputDialog scope={scope} eventId={eventId} callId={callId} name={name}
      verb={name} target="" outcome={outcome} onClose={() => setShown(false)} /></Suspense> : null}
  </ListItem>;
});
