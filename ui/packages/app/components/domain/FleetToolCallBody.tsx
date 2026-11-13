import type { ReactNode } from "react";
import { Button, cn } from "@agentsfleet/design-system";

import type { ToolResult } from "./fleetReplyMessage";
import { moreLinesLabel, outputPreview } from "./tool-call-copy";
import type { LineDiff } from "./tool-call-diff";
import { CodeDiff } from "@/components/assistant-ui/CodeDiff";

// Recorded input previews and returned output stay in separate disclosures.
// Output is rendered as text nodes, never markup.

export const SHOW_ALL_LABEL = "show all";
export const CLIPPED_EDIT_NOTE = "Too long to diff here";
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

/** An edit's lines, indented under its cell, tinted by side. Exported for the
 * full-call dialog, which draws the same rows from the untrimmed edit. */
export function DiffRows({diff}: {diff: LineDiff}) {
  return <CodeDiff filename="Changes" additions={diff.added} deletions={diff.removed} lines={diff.rows} />;
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
