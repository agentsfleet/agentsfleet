import type { ReactNode } from "react";
import { Button, cn } from "@agentsfleet/design-system";

import type { FileChange } from "@/lib/streaming/fleet-stream-tool-trace";
import type { ToolResult } from "../replies/fleetReplyMessage";
import { moreLinesLabel, outputPreview } from "./copy/tool-call-copy";
import type { DiffRow } from "./diff/tool-call-diff";
import { CodeDiff } from "@/components/assistant-ui/CodeDiff";

// Recorded input previews and returned output stay in separate disclosures.
// Output is rendered as text nodes, never markup.

export const SHOW_ALL_LABEL = "show all";
/** The minus a count of removed lines reads with. */
export const MINUS = "−";
/** Under output the runner kept only the edges of: a long line, or a few. */
export const OUTPUT_CUT_LABEL = "… output continues";
export const OUTPUT_RAIL = "└";
const RAIL_ROW = "flex min-w-0 gap-xs";
const RAIL_TEXT = "min-w-0 whitespace-pre-wrap break-words";

type OnShowAll = (() => void) | undefined;

/** Only the recorded result belongs under Output or Error. */
export function ToolCallBody({outcome, onShowAll}: {outcome: ToolResult | undefined; onShowAll: OnShowAll}) {
  return <div className="flex min-w-0 flex-col pl-md"><OutputPreview outcome={outcome} onShowAll={onShowAll} /></div>;
}

/** Each file an edit changed and the lines it gained and lost, as the runner
 * recorded them. */
export function ChangedFiles({changes}: {changes: readonly FileChange[]}) {
  return <>
    {changes.map((change, index) => <RailRow key={`${index}-${change.path}`} glyph={index === 0 ? OUTPUT_RAIL : null}>
      <span data-changed-file="">{change.path}</span>{" "}
      <span className="tabular-nums text-success">+{change.added}</span>{" "}
      <span className="tabular-nums text-destructive">{MINUS}{change.removed}</span>
    </RailRow>)}
  </>;
}

/** An edit's text as the full-call dialog draws it from the untrimmed call,
 * tinted by side; the header counts are the recorded `changes`' totals, and
 * absent when none were recorded. */
export function DiffRows({rows, changes}: {rows: readonly DiffRow[]; changes: readonly FileChange[]}) {
  const counted = changes.length > 0;
  const added = changes.reduce((sum, change) => sum + change.added, 0);
  const removed = changes.reduce((sum, change) => sum + change.removed, 0);
  return <CodeDiff filename="Changes" {...(counted ? { additions: added, deletions: removed } : {})} lines={rows} />;
}

function OutputPreview({ outcome, onShowAll }: { outcome: ToolResult | undefined; onShowAll: OnShowAll }) {
  const preview = outputPreview(outcome?.outputHead, outcome?.outputLineCount, outcome?.status, outcome?.outputTail);
  return (
    <>
      {preview.rows.map((row, index) => <RailRow key={index} glyph={index === 0 ? OUTPUT_RAIL : null}>{row}</RailRow>)}
      {preview.note === null ? null : <RailRow glyph={OUTPUT_RAIL}>{preview.note}</RailRow>}
      {preview.hiddenLines > 0 ? (
        <RailRow glyph={null}>{moreLinesLabel(preview.hiddenLines)}<ShowAll onShowAll={onShowAll} /></RailRow>
      ) : null}
      {preview.cut ? <RailRow glyph={null}>{OUTPUT_CUT_LABEL}<ShowAll onShowAll={onShowAll} /></RailRow> : null}
    </>
  );
}

/** One line under the header; a row with no glyph keeps the rail's width so
 * its text lines up with the row above. */
export function RailRow({ glyph, children }: { glyph: string | null; children: ReactNode }) {
  return (
    <span className={RAIL_ROW}>
      <span aria-hidden="true" className={cn("shrink-0 text-text-dim", glyph === null && "invisible")}>{glyph ?? OUTPUT_RAIL}</span>
      <span className={RAIL_TEXT}>{children}</span>
    </span>
  );
}

export function ShowAll({ onShowAll }: { onShowAll: OnShowAll }) {
  if (onShowAll === undefined) return null;
  return (
    <>
      {" "}
      <Button type="button" variant="link" size="eyebrow" className="px-0 text-label" onClick={onShowAll}>
        {SHOW_ALL_LABEL}
      </Button>
    </>
  );
}
