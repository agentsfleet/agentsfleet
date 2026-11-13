"use client";

import { CheckIcon, XIcon, CircleNotchIcon as Loader2Icon } from "@phosphor-icons/react/ssr";
import { CopyButton, cn } from "@agentsfleet/design-system";
import { mono, paper } from "./surfaces";
import { parseAnsi, ansiColor } from "./terminal-ansi";

// Adapted from the official assistant-ui terminal element. See UPSTREAM.md.
const plainText = (line: string) =>
  parseAnsi(line)
    .map((segment) => segment.text)
    .join("");

export function TerminalBlock({
  command,
  lines,
  done,
  exitCode,
  outcome,
  truncated,
}: {
  command: string;
  lines: readonly string[];
  done: boolean;
  exitCode?: number | undefined;
  outcome?: "succeeded" | "failed" | "interrupted" | "unconfirmed" | undefined;
  truncated?: boolean | undefined;
}) {
  const failed = done && outcome !== "interrupted" && (outcome === "failed" || (exitCode !== undefined && exitCode !== 0));
  const confirmed = outcome !== "interrupted" && (outcome === "succeeded" || exitCode === 0);
  const state = !done ? "running" : failed ? "failed" : outcome === "interrupted" ? "interrupted" : confirmed ? "done" : "unconfirmed";

  return (
    <div
      className={cn(paper, "w-full min-w-0 overflow-hidden rounded-md font-mono text-label")}
      data-slot="terminal-block"
      data-state={state}
    >
      <div className="flex flex-col items-start justify-between gap-2 px-4 pt-3 pb-1.5 sm:flex-row sm:items-center sm:gap-3">
        <span className="min-w-0 break-words text-foreground/90">
          <code>{command}</code>
        </span>
        {done ? (
          <div className="flex shrink-0 items-center gap-1">
            {failed ? (
              <XIcon className="size-3 text-red-600 dark:text-red-400" />
            ) : confirmed ? (
              <CheckIcon className="size-4 text-success" />
            ) : null}
            <span className={cn(mono, failed ? "text-red-600 dark:text-red-400" : "text-muted-foreground")}>
              {exitCode !== undefined ? `exit ${exitCode}` : state === "unconfirmed" ? "Outcome unconfirmed" : state === "interrupted" ? "Interrupted" : failed ? "Failed" : "Completed"}
            </span>
            {lines.length > 0 ? (
              <CopyButton value={lines.map(plainText).join("\n")} label="Copy output" />
            ) : null}
          </div>
        ) : (
          <Loader2Icon className="size-4 text-muted-foreground" />
        )}
      </div>
      <div className="flex min-h-0 flex-col gap-xs px-4 pt-1 pb-3.5 text-muted-foreground">
        <div className="flex flex-col gap-1">
          {lines.map((line, index) => (
            <div
              key={`${index}-${line}`}
              className={cn(
                "fade-in animate-in fill-mode-both break-words whitespace-pre-wrap duration-300 motion-reduce:animate-none",
                index === lines.length - 1 && "text-foreground/90",
              )}
            >
              {parseAnsi(line).map((segment, segmentIndex) => (
                <span
                  key={segmentIndex}
                  className={cn(
                    segment.color !== undefined && ansiColor(segment.color),
                    segment.bold && "font-bold",
                    segment.dim && "opacity-60",
                  )}
                >
                  {segment.text}
                </span>
              ))}
            </div>
          ))}
        </div>
        {truncated ? <span className={cn(mono, "text-muted-foreground")}>output truncated</span> : null}
        {!done && <span aria-hidden className="inline-block h-3 w-1.5 bg-blue-500/70" />}
      </div>
    </div>
  );
}
