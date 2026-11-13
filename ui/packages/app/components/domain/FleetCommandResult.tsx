import type { ToolResult } from "./fleetReplyMessage";
import type { CellState } from "./tool-call-copy";
import { Button } from "@agentsfleet/design-system";
import { TerminalBlock } from "@/components/assistant-ui/TerminalBlock";
import { CELL_STATE, moreLinesLabel, outputPreview } from "./tool-call-copy";

/** Recorded results drive the upstream terminal; a missing exit code stays missing. */
export function FleetCommandResult({command, outcome, state, onShowAll}: {
  command: string; outcome: ToolResult | undefined; state: CellState; onShowAll: (() => void) | undefined;
}) {
  const preview = outputPreview(outcome?.outputHead, outcome?.outputLineCount, outcome?.status, outcome?.outputTail);
  return <div className="space-y-xs" aria-label="Command result">
    <TerminalBlock command={command} lines={preview.rows}
      done={state !== CELL_STATE.RUNNING} outcome={state === CELL_STATE.SETTLED ? "unconfirmed" : state === CELL_STATE.RUNNING ? undefined : state}
      exitCode={outcome?.exitCode} truncated={preview.hiddenLines > 0 || preview.cut} />
    {preview.note ? <p className="text-label text-muted-foreground">{preview.note}</p> : null}
    {preview.hiddenLines > 0 ? <span className="text-label text-muted-foreground">{moreLinesLabel(preview.hiddenLines)}</span> : null}
    {onShowAll && (preview.hiddenLines > 0 || preview.cut) ? <Button variant="link" size="sm" onClick={onShowAll}>show all</Button> : null}
  </div>;
}
