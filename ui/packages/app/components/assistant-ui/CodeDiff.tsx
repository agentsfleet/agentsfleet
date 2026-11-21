// Adapted from https://r.assistant-ui.com/elements-code-diff.json; see UPSTREAM.md.
"use client";

import { FileTextIcon } from "@phosphor-icons/react/ssr";
import { DashboardPanel, cn } from "@agentsfleet/design-system";
import { codeScroll, codeSurface, mono } from "./surfaces";

export type DiffKind = "context" | "added" | "removed";

export interface DiffLine {
  kind: DiffKind;
  text: string;
}

const GUTTER: Record<DiffKind, string> = {
  context: "",
  added: "+",
  removed: "-",
};

export function CodeDiff({
  filename,
  showCounts,
  additions,
  deletions,
  lines,
}: {
  filename: string;
  showCounts: boolean;
  additions: number;
  deletions: number;
  lines: readonly DiffLine[];
}) {
  return (
    <DashboardPanel padding="none"
      data-slot="code-diff"
      className="w-full min-w-0 overflow-hidden rounded-sm bg-surface-deep font-mono text-label"
    >
      <div className="flex min-w-0 items-center gap-sm border-b border-border px-lg py-sm font-sans text-label text-muted-foreground">
        <FileTextIcon aria-hidden="true" className="size-4 shrink-0" /><span className="min-w-0 [overflow-wrap:anywhere]">{filename}</span>
        {showCounts ? <span className={cn(mono, "ml-auto shrink-0 tabular-nums")}>
          <span className="text-success">
            +{additions}
          </span>{" "}
          <span className="text-destructive">−{deletions}</span>
        </span> : null}
      </div>
      <div className={cn(codeScroll, "bg-surface-deep py-sm")}>
        <div className={codeSurface}>
          {lines.map((line, i) => (
            <div
              key={`${i}-${line.text}`}
              data-diff={line.kind}
              className={cn(
                "flex px-lg py-xs leading-mono whitespace-pre motion-reduce:animate-none",
                line.kind === "context" && "text-muted-foreground",
                line.kind === "added" &&
                  "text-success",
                line.kind === "removed" &&
                  "text-destructive",
              )}
            >
              <span aria-hidden="true" className="w-4 shrink-0 select-none">
                {GUTTER[line.kind]}
              </span>
              <span>{line.kind === "context" ? null : <span className="sr-only">{line.kind}: </span>}{line.text}</span>
            </div>
          ))}
        </div>
      </div>
    </DashboardPanel>
  );
}
