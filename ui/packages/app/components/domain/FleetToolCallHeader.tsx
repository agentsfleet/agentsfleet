import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { useToolCallElapsed } from "@assistant-ui/react";
import { cn } from "@agentsfleet/design-system";
import { formatSeconds } from "@/lib/utils";
import { CELL_STATE, type CellState } from "./tool-call-copy";
import { toolActivity } from "./tool-call-activity";
import { ACTIVITY_ICONS, CELL_COPY } from "./tool-call-presentation";

const FIGURE_SEPARATOR = " · ";

export function ToolCallHeader({name, args, state}: {name: string; args: ToolArgs | undefined; state: CellState}) {
  const status = CELL_COPY[state];
  const activity = toolActivity(name, args, state);
  const Icon = ACTIVITY_ICONS[activity.kind];
  return <span data-command-heading="" className="flex min-w-0 flex-1 items-center gap-sm text-left">
    <Icon aria-hidden="true" data-tool-bullet="" data-tool-shimmer={state === CELL_STATE.RUNNING} className={cn("size-3.5 shrink-0 [--tool-active-color:var(--info)]", status.tone)} />
    <span className="min-w-0 flex-1 sm:flex sm:items-center sm:gap-sm">
      <span className="block sm:shrink-0" data-chat-loading={state === CELL_STATE.RUNNING}>{activity.label}</span>
      <span className="block truncate font-mono text-label text-text-dim sm:text-inherit" title={activity.target}>{activity.target ? ` ${activity.target}` : ""}</span>
    </span>
    <span className="sr-only">{` ${status.label}`}</span>
    <CallClock running={state === CELL_STATE.RUNNING} />
  </span>;
}

/** The part clock ticks independently; ongoing seconds stay silent in the live log. */
function CallClock({running}: {running: boolean}) {
  const elapsedMs = useToolCallElapsed();
  if (elapsedMs === undefined) return null;
  return <span aria-hidden={running || undefined} className="shrink-0 whitespace-nowrap text-label tabular-nums">
    {FIGURE_SEPARATOR}{formatSeconds(elapsedMs)}
  </span>;
}
