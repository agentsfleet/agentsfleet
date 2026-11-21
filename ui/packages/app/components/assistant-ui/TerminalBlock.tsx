"use client";

import { TerminalWindowIcon, CircleNotchIcon as Loader2Icon } from "@phosphor-icons/react/ssr";
import { CopyButton, DashboardPanel, cn } from "@agentsfleet/design-system";
import { mono } from "./surfaces";
import { parseAnsi, ansiColor } from "./terminal-ansi";

// Adapted from the official assistant-ui terminal element. See UPSTREAM.md.
const plainText = (line: string) =>
  parseAnsi(line)
    .map((segment) => segment.text)
    .join("");

export function TerminalBlock({
  command,
  commandLinesHidden = 0,
  lines,
  done,
  exitCode,
  outcome,
  truncated,
}: {
  command: string;
  commandLinesHidden?: number;
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
    <DashboardPanel padding="none"
      className="w-full min-w-0 overflow-hidden rounded-sm bg-surface-deep font-mono text-label"
      data-slot="terminal-block"
      data-state={state}
    >
      <div className="flex flex-wrap items-center gap-sm border-b border-border px-lg py-sm font-sans text-label text-muted-foreground">
        <TerminalWindowIcon aria-hidden="true" className="size-4 shrink-0" /><span>Command</span>
        {done ? <>
          <span aria-hidden="true">·</span>
          <span className={failed ? "text-destructive" : undefined}>
            {exitCode !== undefined ? `exit ${exitCode}` : state === "unconfirmed" ? "Outcome unconfirmed" : state === "interrupted" ? "Interrupted" : failed ? "Failed" : "Completed"}
          </span>
          {lines.length > 0 ? <CopyButton className="ml-auto" value={lines.map(plainText).join("\n")} label="Copy output" /> : null}
        </> : <><Loader2Icon className="size-4 text-muted-foreground" aria-hidden="true" /><span>Running</span></>}
      </div>
      <div className="flex min-h-0 flex-col gap-xs bg-surface-deep px-lg py-md text-muted-foreground">
        {command ? <code className="whitespace-pre-wrap [overflow-wrap:anywhere] text-foreground">{command}</code> : null}
        {commandLinesHidden > 0 ? <span className="text-muted-foreground">{commandLinesHidden} more command {commandLinesHidden === 1 ? "line" : "lines"}</span> : null}
        <div className="flex flex-col gap-1">
          {lines.map((line, index) => (
            <div
              key={`${index}-${line}`}
              className={cn(
                "fade-in animate-in fill-mode-both [overflow-wrap:anywhere] whitespace-pre-wrap duration-300 motion-reduce:animate-none",
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
    </DashboardPanel>
  );
}
