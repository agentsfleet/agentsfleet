"use client";

import { lazy, memo, Suspense, useMemo, useState, type ReactNode } from "react";
import { useToolCallElapsed } from "@assistant-ui/react";
import { CheckIcon, CircleAlertIcon, CircleHelpIcon, FileTextIcon, GitPullRequestIcon, GlobeIcon, ListIcon, PencilIcon, SquareTerminalIcon, WrenchIcon, XIcon } from "lucide-react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, CopyButton, DashboardPanel, List, ListItem, SignalSpinner, cn } from "@agentsfleet/design-system";
import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { formatSeconds } from "@/lib/utils";
import { ToolCallBody, ToolCallInputPreview } from "./FleetToolCallBody";
import { useFleetScope } from "./FleetScope";
import { readToolResult, type ToolResult } from "./fleetReplyMessage";
import { CELL_STATE, cellState, toolCopy, type CellState } from "./tool-call-copy";
import { ARGS_NOT_RECORDED } from "./tool-call-shape";
import type { ToolActivity } from "./tool-call-activity";
import { toolActivity } from "./tool-call-activity";

export const TOOL_CALLS_LABEL = "Tool activity";
export const TOOL_SUCCEEDED_GLYPH = "✓";
const FIGURE_SEPARATOR = " · ";
const COMMANDS = "commands";
const COMMAND = "command";
const TRIGGER_CLASS = "min-h-11 justify-start gap-sm rounded-sm py-sm text-label text-muted-foreground hover:no-underline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-info sm:min-h-0";
const CELL_COPY: Record<CellState, {label: string; tone: string; icon: ReactNode}> = {
  [CELL_STATE.RUNNING]: {label: "Running", tone: "text-info", icon: <SignalSpinner className="size-3.5 text-current" aria-hidden="true" />},
  [CELL_STATE.SUCCEEDED]: {label: "Success", tone: "text-success", icon: <span className="inline-flex size-3.5 shrink-0 items-center justify-center leading-none" aria-hidden="true">{TOOL_SUCCEEDED_GLYPH}</span>},
  [CELL_STATE.FAILED]: {label: "Failed", tone: "text-destructive", icon: <XIcon className="size-3.5" aria-hidden="true" />},
  [CELL_STATE.INTERRUPTED]: {label: "Interrupted", tone: "text-warning", icon: <CircleAlertIcon className="size-3.5" aria-hidden="true" />},
  [CELL_STATE.SETTLED]: {label: "Outcome unavailable", tone: "text-text-dim", icon: <CircleHelpIcon className="size-3.5" aria-hidden="true" />},
};
const FleetToolOutputDialog = lazy(() => import("./FleetToolOutputDialog").then(module => ({default: module.FleetToolOutputDialog})));
const FULL_OUTPUT_LOADING = <output className="inline-flex items-center gap-xs text-label text-muted-foreground"><SignalSpinner className="text-pulse" aria-hidden="true" />Loading full command output…</output>;
const ACTIVITY_ICONS = {read: FileTextIcon, write: PencilIcon, list: ListIcon, command: SquareTerminalIcon,
  request: GlobeIcon, "pull-request": GitPullRequestIcon, check: CheckIcon, tool: WrenchIcon};

/** Adjacent calls share one compact disclosure; each keeps its own identity. */
export function ToolCallList({children, activity}: {children: ReactNode; activity: ToolActivity}) {
  const running = activity.state === CELL_STATE.RUNNING;
  const Icon = ACTIVITY_ICONS[activity.kind];
  const label = [activity.label, activity.target].filter(Boolean).join(" ");
  return <Accordion type="single" collapsible className="mb-md">
    <AccordionItem value={COMMANDS} className="border-0">
      <AccordionTrigger className={TRIGGER_CLASS} aria-label={`${TOOL_CALLS_LABEL}: ${label}`} title={label}>
        <span aria-hidden="true" className={cn("inline-flex size-3.5 shrink-0 items-center justify-center", CELL_COPY[activity.state].tone)}>{running ? <SignalSpinner className="size-3.5 text-current" /> : <Icon className="size-3.5 shrink-0" />}</span>
        <span className="min-w-0 flex-1 text-left sm:truncate" role={running ? "status" : undefined}>
          <span className="block sm:inline">{activity.label}</span>{activity.target ? <> <span className="block truncate text-text-dim sm:inline sm:text-inherit">{activity.target}</span></> : null}
        </span>
      </AccordionTrigger>
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

function ToolCallHeader({name, args, state}: {name: string; args: ToolArgs | undefined; state: CellState}) {
  const status = CELL_COPY[state];
  const activity = toolActivity(name, args, state);
  const Icon = ACTIVITY_ICONS[activity.kind];
  return <span data-command-heading="" className="flex min-w-0 flex-1 items-center gap-sm text-left">
    <Icon aria-hidden="true" data-tool-bullet="" data-tool-shimmer={state === CELL_STATE.RUNNING} className={cn("size-3.5 shrink-0 [--tool-active-color:var(--info)]", status.tone)} />
    <span className="min-w-0 flex-1 sm:flex sm:items-center sm:gap-xs">
      <span className="block sm:shrink-0">{activity.label}</span>
      <span className="block truncate font-mono text-text-dim sm:text-inherit" title={activity.target}>{activity.target ? ` ${activity.target}` : ""}</span>
    </span>
    <span className="sr-only">{` ${status.label}`}</span>
    <CallClock running={state === CELL_STATE.RUNNING} />
  </span>;
}

function ToolCallDetails({name, args, argsText, outcome, state, onShowAll}: {
  name: string; args: ToolArgs | undefined; argsText: string | undefined; outcome: ToolResult | undefined; state: CellState; onShowAll: (() => void) | undefined;
}) {
  const copy = useMemo(() => toolCopy(name, args), [name, args]);
  const status = CELL_COPY[state];
  return <DashboardPanel padding="none" className="overflow-hidden rounded-lg bg-card">
      <div className="flex items-center justify-between gap-sm border-b border-border px-lg py-sm text-label text-muted-foreground"><span>Action · {name}</span>{argsText === undefined ? null : <CopyButton value={argsText} label="Copy input" />}</div>
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
  return <span aria-hidden={running || undefined} className="shrink-0 whitespace-nowrap text-label tabular-nums">
    {FIGURE_SEPARATOR}{formatSeconds(elapsedMs)}
  </span>;
}
