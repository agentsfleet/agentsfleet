import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { lazy, memo, Suspense, useMemo, useState } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, ListItem, SignalSpinner, cn } from "@agentsfleet/design-system";
import { useFleetScope } from "../../conversation/stream/FleetScope";
import { readToolResult } from "../replies/fleetReplyMessage";
import { cellState, CELL_STATE, toolCopy } from "./copy/tool-call-copy";
import { ToolCallHeader } from "./FleetToolCallHeader";
import { ToolCallDetails } from "./FleetToolCallDetails";
import { TRIGGER_CLASS } from "./presentation/tool-call-presentation";
import { TOOL_BODY, TOOL_NAME } from "./presentation/tool-call-shape";
import { DiffRows } from "./FleetToolCallBody";
import { FleetCommandResult } from "./FleetCommandResult";

const COMMAND = "command";
const CARD_TOOLS: ReadonlySet<string> = new Set([
  TOOL_NAME.EXEC_COMMAND, TOOL_NAME.SHELL, TOOL_NAME.FILE_EDIT, TOOL_NAME.FILE_EDIT_HASHED, TOOL_NAME.APPLY_PATCH,
]);
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
  // These bounded cards mount with the work group. Other tools keep their body behind the action disclosure.
  const card = useMemo(() => CARD_TOOLS.has(name) ? toolCopy(name, named) : undefined, [name, named]);
  const [shown, setShown] = useState(false);
  const scope = useFleetScope();
  const callId = outcome?.callId;
  const canShowAll = scope !== null && callId !== undefined && settled;
  if (shown && !canShowAll) setShown(false);
  const showAll = canShowAll ? () => setShown(true) : undefined;
  const diff = card?.body.kind === TOOL_BODY.DIFF ? card.body.diff : undefined;
  const command = name === TOOL_NAME.EXEC_COMMAND || name === TOOL_NAME.SHELL;
  const commandBody = card?.body.kind === TOOL_BODY.COMMAND ? card.body : undefined;
  const confirmed = state === CELL_STATE.SUCCEEDED;
  const showCounts = diff !== undefined && confirmed;
  const showSuccess = command && confirmed && (outcome?.exitCode === undefined || outcome.exitCode === 0);
  return <ListItem data-tool={name} data-done={outcome !== undefined || undefined} data-state={state}
    className="flex min-w-0 flex-col gap-xs text-body-sm text-muted-foreground">
    <Accordion type="single" collapsible>
      <AccordionItem value={COMMAND} className="border-0">
        <AccordionTrigger className={cn(TRIGGER_CLASS, (showCounts || showSuccess) && "[&>svg:last-child]:ml-0")}>
          <ToolCallHeader name={name} args={named} state={state} />
          {showCounts ? <span className="ml-auto shrink-0 font-mono text-label tabular-nums" aria-label={`${diff.added} ${diff.added === 1 ? "addition" : "additions"}, ${diff.removed} ${diff.removed === 1 ? "deletion" : "deletions"}`}>
            <span className="text-success">+{diff.added}</span>{" "}<span className="text-destructive">−{diff.removed}</span>
          </span> : null}
          {showSuccess ? <span className="ml-auto text-label text-success" aria-hidden="true">Succeeded</span> : null}
        </AccordionTrigger>
        {diff ? <div className="mb-md px-md">
          <DiffRows diff={diff} filename={card?.target} confirmed={confirmed} />
        </div> : null}
        {command ? <div className="mb-md px-md">
          <FleetCommandResult command={commandBody ? [card?.target, ...commandBody.rail].join("\n") : ""} commandLinesHidden={commandBody?.hiddenLines ?? 0}
            outcome={outcome} state={state} onShowAll={showAll} />
        </div> : null}
        <AccordionContent className="ml-md border-l border-border pb-lg pl-2xl pt-sm">
          <ToolCallDetails name={name} args={named} argsText={named === undefined ? undefined : argsText} outcome={outcome} state={state}
            card={card} command={command} onShowAll={showAll} />
        </AccordionContent>
      </AccordionItem>
    </Accordion>
    {shown && canShowAll ? <Suspense fallback={FULL_OUTPUT_LOADING}><FleetToolOutputDialog scope={scope} eventId={eventId} callId={callId} name={name}
      verb={name} target="" outcome={outcome} onClose={() => setShown(false)} /></Suspense> : null}
  </ListItem>;
});
