import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { cn } from "@agentsfleet/design-system";
import { CELL_STATE, type CellState } from "./tool-call-copy";
import { toolActivity, toolActivityLabel } from "./tool-call-activity";
import { ACTIVITY_ICONS, CELL_COPY } from "./tool-call-presentation";

export function ToolCallHeader({name, args, state}: {name: string; args: ToolArgs | undefined; state: CellState}) {
  const status = CELL_COPY[state];
  const activity = toolActivity(name, args, state);
  const Icon = ACTIVITY_ICONS[activity.kind];
  return <span data-command-heading="" className="inline-flex min-w-0 items-center gap-md text-left">
    <Icon aria-hidden="true" data-tool-bullet="" className={cn("size-4 shrink-0", state === CELL_STATE.RUNNING || state === CELL_STATE.SUCCEEDED ? "text-muted-foreground" : status.tone)} />
    <span className="min-w-0" data-chat-loading={state === CELL_STATE.RUNNING}>{toolActivityLabel(activity)}</span>
    <span className="sr-only">{` ${status.label}`}</span>
  </span>;
}
