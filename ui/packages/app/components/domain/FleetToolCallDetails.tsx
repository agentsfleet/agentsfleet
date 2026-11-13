import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import type { ToolResult } from "./fleetReplyMessage";
import type { CellState } from "./tool-call-copy";
import { useToolCallElapsed } from "@assistant-ui/react";
import { formatMs } from "@/lib/utils";
import { useMemo } from "react";
import { CopyButton, DashboardPanel, cn } from "@agentsfleet/design-system";
import { ToolCallBody } from "./FleetToolCallBody";
import { ToolCallInputPreview } from "./FleetToolInputElements";
import { CELL_STATE, toolCopy } from "./tool-call-copy";
import { ARGS_NOT_RECORDED, TOOL_BODY } from "./tool-call-shape";
import { CELL_COPY } from "./tool-call-presentation";
import { FleetCommandResult } from "./FleetCommandResult";

export function ToolCallDetails({name, args, argsText, outcome, state, onShowAll}: {
  name: string; args: ToolArgs | undefined; argsText: string | undefined; outcome: ToolResult | undefined; state: CellState; onShowAll: (() => void) | undefined;
}) {
  const copy = useMemo(() => toolCopy(name, args), [name, args]);
  const status = CELL_COPY[state];
  return <DashboardPanel padding="none" className="overflow-hidden rounded-lg bg-card">
      <div className="flex items-center justify-between gap-sm border-b border-border px-lg py-sm text-label text-muted-foreground"><span>Action · {name}</span>{argsText === undefined ? null : <CopyButton value={argsText} label="Copy input" />}</div>
      <div className="space-y-sm px-lg py-md font-mono text-label leading-mono">
        <CallDuration running={state === CELL_STATE.RUNNING} />
        <h4 className="font-sans text-muted-foreground">Input</h4>
        {argsText === undefined ? ARGS_NOT_RECORDED : <>
          <pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words">{argsText}</pre>
          {copy.body.kind === TOOL_BODY.COMMAND ? null : <ToolCallInputPreview copy={copy} onShowAll={onShowAll} />}
        </>}
        {copy.body.kind === TOOL_BODY.COMMAND ? <>
          <h4 className="font-sans text-muted-foreground">{state === CELL_STATE.FAILED ? "Error" : "Output"}</h4>
          <FleetCommandResult command={[copy.target, ...copy.body.rail].join("\n")} outcome={outcome} state={state} onShowAll={onShowAll} />
        </> : state === CELL_STATE.RUNNING ? null : <><h4 className="font-sans text-muted-foreground">{state === CELL_STATE.FAILED ? "Error" : "Output"}</h4><ToolCallBody outcome={outcome} onShowAll={onShowAll} /></>}
      </div>
      <div className={cn("flex items-center justify-end gap-xs border-t border-border px-lg py-sm text-label", status.tone)} data-command-outcome="">
        {status.icon}
        <span>{status.label}</span>
        {outcome?.exitCode !== undefined && outcome.exitCode !== 0 ? <span>(exit {outcome.exitCode})</span> : null}
      </div>
    </DashboardPanel>;
}


/** The native part clock only runs while its details are mounted. */
function CallDuration({running}: {running: boolean}) {
  const elapsedMs = useToolCallElapsed();
  if (elapsedMs === undefined) return null;
  return <p data-tool-duration="" aria-hidden={running || undefined} className="font-sans text-label tabular-nums text-muted-foreground">
    {running ? "Elapsed" : "Duration"} {formatMs(elapsedMs)}
  </p>;
}
