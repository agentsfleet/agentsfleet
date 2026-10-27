"use client";

import { memo, useMemo, useState, type ReactNode } from "react";
import { useToolCallElapsed } from "@assistant-ui/react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, CopyButton, List, ListItem, SignalSpinner, cn } from "@agentsfleet/design-system";
import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { formatSeconds } from "@/lib/utils";
import { ToolCallBody, ToolCallInputPreview } from "./FleetToolCallBody";
import { useFleetScope } from "./FleetScope";
import { FleetToolOutputDialog } from "./FleetToolOutputDialog";
import { readToolResult, type ToolResult } from "./fleetReplyMessage";
import { CELL_STATE, cellState, toolCopy, type CellState } from "./tool-call-copy";
import { ARGS_NOT_RECORDED, type ToolCopy } from "./tool-call-shape";

export const TOOL_CALLS_LABEL = "Tool calls";
export const TOOL_BULLET = "•";
export const TOOL_SUCCEEDED_GLYPH = "✓";
const FIGURE_SEPARATOR = " · ";
const INPUT = "input";
const OUTPUT = "output";
const TRIGGER_CLASS = "justify-start gap-xs py-xs text-label text-muted-foreground hover:no-underline";
const CELL_COPY: Record<CellState, {label: string; tone: string}> = {
  [CELL_STATE.RUNNING]: {label: "Running", tone: "text-text-dim"},
  [CELL_STATE.SUCCEEDED]: {label: "Done", tone: "text-success"},
  [CELL_STATE.FAILED]: {label: "Failed", tone: "text-destructive"},
  [CELL_STATE.INTERRUPTED]: {label: "Interrupted", tone: "text-destructive"},
  [CELL_STATE.SETTLED]: {label: "Outcome unavailable", tone: "text-text-dim"},
};

/** Each recorded call is a separate entry; only that call updates as it finishes. */
export function ToolCallList({children}: {children: ReactNode}) {
  return <List variant="plain" aria-label={TOOL_CALLS_LABEL} className="mb-md flex flex-col gap-lg space-y-0">{children}</List>;
}

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
  const copy = useMemo(() => toolCopy(name, named), [name, named]);
  const outcome = readToolResult(result);
  const state = cellState(outcome !== undefined, outcome?.status, running);
  const [shown, setShown] = useState(false);
  const scope = useFleetScope();
  const callId = outcome?.callId;
  const canShowAll = scope !== null && callId !== undefined && settled;
  if (shown && !canShowAll) setShown(false);
  return <ListItem data-tool={name} data-done={outcome !== undefined || undefined} data-state={state}
    className="flex min-w-0 flex-col gap-xs text-body-sm text-text-subtle">
    <ToolCallHeader name={name} outcome={outcome} state={state} />
    <ToolCallDetails argsText={named === undefined ? undefined : argsText} copy={copy} outcome={outcome} state={state}
      onShowAll={canShowAll ? () => setShown(true) : undefined} />
    {shown && canShowAll ? <FleetToolOutputDialog scope={scope} eventId={eventId} callId={callId} name={name}
      verb={name} target="" outcome={outcome} onClose={() => setShown(false)} /> : null}
  </ListItem>;
});

function ToolCallHeader({name, outcome, state}: {name: string; outcome: ToolResult | undefined; state: CellState}) {
  const status = CELL_COPY[state];
  return <span className="flex min-w-0 flex-wrap items-baseline gap-xs">
    <span aria-hidden="true" data-tool-bullet="" data-tool-shimmer={state === CELL_STATE.RUNNING} className={status.tone}>
      {state === CELL_STATE.RUNNING ? <SignalSpinner className="text-pulse" /> : state === CELL_STATE.SUCCEEDED ? TOOL_SUCCEEDED_GLYPH : TOOL_BULLET}
    </span>
    <span className="min-w-0 break-words font-mono text-label text-foreground">{name}</span>
    <span className={cn("text-label", status.tone)}>{status.label}</span>
    {outcome?.exitCode !== undefined && outcome.exitCode !== 0 ? <span className="text-destructive">(exit {outcome.exitCode})</span> : null}
    <CallClock running={state === CELL_STATE.RUNNING} />
  </span>;
}

function ToolCallDetails({argsText, copy, outcome, state, onShowAll}: {
  argsText: string | undefined; copy: ToolCopy; outcome: ToolResult | undefined; state: CellState; onShowAll: (() => void) | undefined;
}) {
  return <Accordion type="multiple">
    <AccordionItem value={INPUT} className="border-0">
      <AccordionTrigger className={TRIGGER_CLASS}>Input</AccordionTrigger>
      <AccordionContent className="font-mono text-label leading-mono">
        {argsText === undefined ? ARGS_NOT_RECORDED : <>
          <CopyButton value={argsText} label="Copy input" />
          <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words">{argsText}</pre>
          <ToolCallInputPreview copy={copy} onShowAll={onShowAll} />
        </>}
      </AccordionContent>
    </AccordionItem>
    <AccordionItem value={OUTPUT} className="border-0">
      <AccordionTrigger disabled={state === CELL_STATE.RUNNING} className={TRIGGER_CLASS}>
        {state === CELL_STATE.FAILED ? "Error" : "Output"}
      </AccordionTrigger>
      <AccordionContent className="font-mono text-label leading-mono"><ToolCallBody outcome={outcome} onShowAll={onShowAll} /></AccordionContent>
    </AccordionItem>
  </Accordion>;
}

/** The part clock ticks independently; ongoing seconds stay silent in the live log. */
function CallClock({running}: {running: boolean}) {
  const elapsedMs = useToolCallElapsed();
  if (elapsedMs === undefined) return null;
  return <span aria-hidden={running || undefined} className="text-label tabular-nums">
    {FIGURE_SEPARATOR}{formatSeconds(elapsedMs)}
  </span>;
}
