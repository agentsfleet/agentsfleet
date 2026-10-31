"use client";

import { lazy, memo, Suspense, useMemo, useState, type ReactNode } from "react";
import { useToolCallElapsed } from "@assistant-ui/react";
import { CircleAlertIcon, CircleHelpIcon, SquareTerminalIcon, XIcon } from "lucide-react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, CopyButton, DashboardPanel, List, ListItem, SignalSpinner, cn } from "@agentsfleet/design-system";
import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { formatSeconds } from "@/lib/utils";
import { ToolCallBody, ToolCallInputPreview } from "./FleetToolCallBody";
import { useFleetScope } from "./FleetScope";
import { readToolResult, type ToolResult } from "./fleetReplyMessage";
import { CELL_STATE, cellState, toolCopy, type CellState } from "./tool-call-copy";
import { ARGS_NOT_RECORDED } from "./tool-call-shape";

export const TOOL_CALLS_LABEL = "Ran commands";
export const TOOL_SUCCEEDED_GLYPH = "✓";
const FIGURE_SEPARATOR = " · ";
const COMMANDS = "commands";
const COMMAND = "command";
const TRIGGER_CLASS = "justify-start gap-xs py-xs text-label text-muted-foreground hover:no-underline";
const CELL_COPY: Record<CellState, {label: string; tone: string; icon: ReactNode}> = {
  [CELL_STATE.RUNNING]: {label: "Running", tone: "text-text-dim", icon: <SignalSpinner aria-hidden="true" />},
  [CELL_STATE.SUCCEEDED]: {label: "Success", tone: "text-success", icon: <span aria-hidden="true">{TOOL_SUCCEEDED_GLYPH}</span>},
  [CELL_STATE.FAILED]: {label: "Failed", tone: "text-destructive", icon: <XIcon className="size-3.5" aria-hidden="true" />},
  [CELL_STATE.INTERRUPTED]: {label: "Interrupted", tone: "text-destructive", icon: <CircleAlertIcon className="size-3.5" aria-hidden="true" />},
  [CELL_STATE.SETTLED]: {label: "Outcome unavailable", tone: "text-text-dim", icon: <CircleHelpIcon className="size-3.5" aria-hidden="true" />},
};
const FleetToolOutputDialog = lazy(() => import("./FleetToolOutputDialog").then(module => ({default: module.FleetToolOutputDialog})));
const FULL_OUTPUT_LOADING = <output className="inline-flex items-center gap-xs text-label text-muted-foreground"><SignalSpinner className="text-pulse" aria-hidden="true" />Loading full command output…</output>;

/** Adjacent calls share one compact disclosure; each keeps its own identity. */
export function ToolCallList({children, running}: {children: ReactNode; running: boolean}) {
  return <Accordion type="single" collapsible className="mb-md">
    <AccordionItem value={COMMANDS} className="border-0">
      <AccordionTrigger className={TRIGGER_CLASS} title={running ? "Running commands" : TOOL_CALLS_LABEL}><span aria-hidden="true">{running ? <SignalSpinner className="text-pulse" /> : <SquareTerminalIcon className="size-3.5 shrink-0" />}</span>{TOOL_CALLS_LABEL}</AccordionTrigger>
      <AccordionContent className="pb-0"><List variant="plain" aria-label={TOOL_CALLS_LABEL} className="flex flex-col gap-xs space-y-0">{children}</List></AccordionContent>
    </AccordionItem>
  </Accordion>;
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
          <ToolCallHeader name={name} state={state} />
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

function ToolCallHeader({name, state}: {name: string; state: CellState}) {
  const status = CELL_COPY[state];
  return <span data-command-heading="" className="flex min-w-0 flex-1 items-center gap-xs text-left">
    <SquareTerminalIcon aria-hidden="true" data-tool-bullet="" data-tool-shimmer={state === CELL_STATE.RUNNING} className={cn("size-3.5 shrink-0", status.tone)} />
    <span className="shrink-0">{state === CELL_STATE.RUNNING ? "Running command" : "Ran command"}</span>
    <span className="min-w-0 truncate font-mono">{name}</span>
    <span className="sr-only">{status.label}</span>
    <CallClock running={state === CELL_STATE.RUNNING} />
  </span>;
}

function ToolCallDetails({name, args, argsText, outcome, state, onShowAll}: {
  name: string; args: ToolArgs | undefined; argsText: string | undefined; outcome: ToolResult | undefined; state: CellState; onShowAll: (() => void) | undefined;
}) {
  const copy = useMemo(() => toolCopy(name, args), [name, args]);
  const status = CELL_COPY[state];
  return <DashboardPanel padding="none" className="overflow-hidden rounded-lg bg-card">
      <div className="flex items-center justify-between gap-sm border-b border-border px-lg py-sm text-label text-muted-foreground"><span>Command</span>{argsText === undefined ? null : <CopyButton value={argsText} label="Copy input" />}</div>
      <div className="space-y-sm px-lg py-md font-mono text-label leading-mono">
        <h4 className="font-sans text-muted-foreground">Input</h4>
        {argsText === undefined ? ARGS_NOT_RECORDED : <>
          <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words">{argsText}</pre>
          <ToolCallInputPreview copy={copy} onShowAll={onShowAll} />
        </>}
        {state === CELL_STATE.RUNNING ? null : <><h4 className="font-sans text-muted-foreground">{state === CELL_STATE.FAILED ? "Error" : "Output"}</h4><ToolCallBody outcome={outcome} onShowAll={onShowAll} /></>}
      </div>
      <div className={cn("flex items-center justify-end gap-xs border-t border-border px-lg py-sm text-label", status.tone)} data-command-outcome="">
        {status.icon}
        <span>{status.label}</span>
        {outcome?.exitCode !== undefined && outcome.exitCode !== 0 ? <span>(exit {outcome.exitCode})</span> : null}
      </div>
    </DashboardPanel>;
}

/** The part clock ticks independently; ongoing seconds stay silent in the live log. */
function CallClock({running}: {running: boolean}) {
  const elapsedMs = useToolCallElapsed();
  if (elapsedMs === undefined) return null;
  return <span aria-hidden={running || undefined} className="text-label tabular-nums">
    {FIGURE_SEPARATOR}{formatSeconds(elapsedMs)}
  </span>;
}
