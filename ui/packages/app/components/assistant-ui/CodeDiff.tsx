// Adapted from https://r.assistant-ui.com/elements-code-diff.json; see UPSTREAM.md.
"use client";

import type { ComponentProps } from "react";
import { cn } from "@agentsfleet/design-system";
import { codeScroll, codeSurface, mono, paper } from "./surfaces";

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
  additions,
  deletions,
  lines,
  cycle,
  className,
  ...props
}: Omit<
  ComponentProps<"div">,
  "children" | "filename" | "additions" | "deletions" | "lines" | "cycle"
> & {
  filename: string;
  additions: number;
  deletions: number;
  lines: readonly DiffLine[];
  cycle: number;
}) {
  return (
    <div
      data-slot="code-diff"
      className={cn(
        paper,
        "w-full min-w-0 overflow-hidden rounded-md font-mono text-label",
        className,
      )}

      {...props}
    >
      <div className="flex items-center justify-between px-4 pt-3 pb-2">
        <span className="text-foreground/90">{filename}</span>
        <span className={cn(mono, "tabular-nums")}>
          <span className="text-emerald-600 dark:text-emerald-400">
            +{additions}
          </span>{" "}
          <span className="text-red-600 dark:text-red-400">−{deletions}</span>
        </span>
      </div>
      <div className={codeScroll}>
        <div className={codeSurface}>
          {lines.map((line, i) => (
            <div
              key={`${cycle}-${i}-${line.text}`}
              data-diff={line.kind}
              className={cn(
                "flex px-sm py-xs leading-mono whitespace-pre motion-reduce:animate-none",
                line.kind === "context" && "text-muted-foreground",
                line.kind === "added" &&
                  "bg-success/10 text-success",
                line.kind === "removed" &&
                  "bg-destructive/10 text-destructive",
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
    </div>
  );
}
