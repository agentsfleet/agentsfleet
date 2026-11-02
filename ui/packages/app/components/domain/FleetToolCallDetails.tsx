import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import type { ToolResult } from "./fleetReplyMessage";
import type { CellState } from "./tool-call-copy";
import { useMemo } from "react";
import { CopyButton, DashboardPanel, cn } from "@agentsfleet/design-system";
import { ToolCallBody, ToolCallInputPreview } from "./FleetToolCallBody";
import { CELL_STATE, toolCopy } from "./tool-call-copy";
import { ARGS_NOT_RECORDED } from "./tool-call-shape";
import { CELL_COPY } from "./tool-call-presentation";

export function ToolCallDetails({name, args, argsText, outcome, state, onShowAll}: {
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

