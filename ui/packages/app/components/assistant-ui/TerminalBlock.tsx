"use client";

import type { ComponentProps } from "react";
import { useId, useState } from "react";
import { CheckIcon, XIcon, CircleNotchIcon as Loader2Icon } from "@phosphor-icons/react/ssr";
import { CopyButton, cn } from "@agentsfleet/design-system";
import { ghostButton, mono, paper } from "./surfaces";
import { clamp, take } from "./range";
import { parseAnsi, ansiColor } from "./terminal-ansi";

// Adapted from the official assistant-ui terminal element. See UPSTREAM.md.
const formatDuration = (durationMs: number) => {
  const duration = Math.floor(clamp(durationMs, 0, Number.MAX_SAFE_INTEGER));
  if (duration < 1000) return `${duration}ms`;
  if (duration < 60_000) {
    const seconds = Math.round(duration / 100) / 10;
    return `${Number.isInteger(seconds) ? seconds : seconds.toFixed(1)}s`;
  }

  const minutes = Math.floor(duration / 60_000);
  const seconds = Math.floor((duration % 60_000) / 1000);
  return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
};

const plainText = (line: string) =>
  parseAnsi(line)
    .map((segment) => segment.text)
    .join("");

export function TerminalBlock({
  command,
  lines,
  visibleCount,
  done,
  variant = "paper",
  exitCode: exitCodeProp,
  outcome,
  stderr,
  cwd,
  durationMs,
  truncated,
  maxCollapsedLines,
  className,
  ...props
}: Omit<
  ComponentProps<"div">,
  | "children"
  | "command"
  | "lines"
  | "visibleCount"
  | "done"
  | "variant"
  | "exitCode"
  | "stderr"
  | "cwd"
  | "durationMs"
  | "truncated"
  | "maxCollapsedLines"
> & {
  command: string;
  lines: readonly string[];
  visibleCount: number;
  done: boolean;
  variant?: "paper" | "ink" | undefined;
  exitCode?: number | undefined;
  outcome?: "succeeded" | "failed" | "interrupted" | "unconfirmed" | undefined;
  stderr?: readonly string[] | undefined;
  cwd?: string | undefined;
  durationMs?: number | undefined;
  truncated?: boolean | undefined;
  maxCollapsedLines?: number | undefined;
}) {
  const ink = variant === "ink";
  const exitCode = exitCodeProp;
  const failed = done && outcome !== "interrupted" && (outcome === "failed" || (exitCode !== undefined && exitCode !== 0));
  const confirmed = outcome !== "interrupted" && (outcome === "succeeded" || exitCode === 0);
  const state = !done ? "running" : failed ? "failed" : outcome === "interrupted" ? "interrupted" : confirmed ? "done" : "unconfirmed";
  const output = [
    ...lines.map((line) => ({ line, stderr: false })),
    ...(stderr ?? []).map((line) => ({ line, stderr: true })),
  ];
  const revealedLines = take(output, visibleCount);
  const collapsedLines = Math.floor(
    clamp(maxCollapsedLines ?? revealedLines.length, 0, revealedLines.length),
  );
  const canCollapse =
    done &&
    maxCollapsedLines !== undefined &&
    revealedLines.length > collapsedLines;
  const [expanded, setExpanded] = useState(false);
  const displayedLines =
    canCollapse && !expanded
      ? take(revealedLines, collapsedLines)
      : revealedLines;
  const outputId = useId();

  return (
    <div
      className={cn(
        ink ? "bg-foreground dark:bg-popover" : paper,
        "w-full min-w-0 overflow-hidden rounded-md font-mono text-label",
        className,
      )}
      {...props}
      data-slot="terminal-block"
      data-state={state}
    >
      <div className="flex flex-col items-start justify-between gap-2 px-4 pt-3 pb-1.5 sm:flex-row sm:items-center sm:gap-3">
        <span
          className={cn(
            "min-w-0 break-words",
            ink
              ? "text-background/90 dark:text-foreground/90"
              : "text-foreground/90",
          )}
        >
          {cwd ? (
            <span
              className={cn(
                ink
                  ? "text-background/40 dark:text-muted-foreground"
                  : "text-muted-foreground",
              )}
            >
              {cwd} ${" "}
            </span>
          ) : null}
          <code>{command}</code>
        </span>
        {done ? (
          <div className="flex shrink-0 items-center gap-1">
            {failed ? (
              <XIcon className="size-3 text-red-600 dark:text-red-400" />
            ) : confirmed ? (
              <CheckIcon className="size-4 text-success" />
            ) : null}
            <span
              className={cn(
                mono,
                failed
                  ? "text-red-600 dark:text-red-400"
                  : ink
                    ? "text-background/40 dark:text-muted-foreground"
                    : "text-muted-foreground",
              )}
            >
              {exitCode !== undefined ? `exit ${exitCode}` : state === "unconfirmed" ? "Outcome unconfirmed" : state === "interrupted" ? "Interrupted" : failed ? "Failed" : "Completed"}
            </span>
            {durationMs !== undefined ? (
              <span
                className={cn(
                  mono,
                  ink
                    ? "text-background/40 dark:text-muted-foreground"
                    : "text-muted-foreground",
                )}
              >
                {formatDuration(durationMs)}
              </span>
            ) : null}
            {output.length > 0 ? (
              <CopyButton value={output.map(({line}) => plainText(line)).join("\n")} label="Copy output" />
            ) : null}
          </div>
        ) : (
          <Loader2Icon
            className={cn(
              "size-4",
              ink
                ? "text-background/35 dark:text-muted-foreground"
                : "text-muted-foreground",
            )}
          />
        )}
      </div>
      <div
        className={cn(
          "flex min-h-0 flex-col gap-xs px-4 pt-1 pb-3.5",
          ink
            ? "text-background/55 dark:text-muted-foreground"
            : "text-muted-foreground",
        )}
      >
        <div id={outputId} className="flex flex-col gap-1">
          {displayedLines.map(({ line, stderr: isStderr }, index) => {
            const isLast = index === revealedLines.length - 1;
            return (
              <div
                key={`${index}-${line}`}
                className={cn(
                  "fade-in animate-in fill-mode-both break-words whitespace-pre-wrap duration-300 motion-reduce:animate-none",
                  isStderr
                    ? "text-red-600 dark:text-red-400"
                    : isLast &&
                        (ink
                          ? "text-background/90 dark:text-foreground/90"
                          : "text-foreground/90"),
                )}
              >
                {parseAnsi(line).map((segment, segmentIndex) => (
                  <span
                    key={segmentIndex}
                    className={cn(
                      !isStderr &&
                        segment.color !== undefined &&
                        ansiColor(segment.color, ink),
                      segment.bold && "font-bold",
                      segment.dim && "opacity-60",
                    )}
                  >
                    {segment.text}
                  </span>
                ))}
              </div>
            );
          })}
        </div>
        {canCollapse ? (
          <button
            type="button"
            aria-expanded={expanded}
            aria-controls={outputId}
            onClick={() => setExpanded((value) => !value)}
            className={cn(
              ghostButton,
              "h-7 self-start px-2 text-xs",
              ink &&
                "text-background/45 hover:text-background dark:text-muted-foreground dark:hover:text-foreground",
            )}
          >
            {expanded ? "Show less" : `Show all ${revealedLines.length} lines`}
          </button>
        ) : null}
        {truncated ? (
          <span
            className={cn(
              mono,
              ink
                ? "text-background/35 dark:text-muted-foreground"
                : "text-muted-foreground",
            )}
          >
            output truncated
          </span>
        ) : null}
        {!done && (
          <span
            aria-hidden
            className="inline-block h-3 w-1.5 bg-blue-500/70"
          />
        )}
      </div>
    </div>
  );
}
