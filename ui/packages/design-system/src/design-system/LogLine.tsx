import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * LogLine — content-side coloring for log/event streams rendered
 * inside `<Terminal>`. Mirrors the colored log specimen at
 * `~/.gstack/projects/usezombie/designs/design-system-20260508-0831/preview.html`
 * §03 (CODE / LOGS): each severity gets a distinct token-driven
 * colour without breaking the surrounding monochrome chrome.
 *
 *   INFO     — text-foreground (default)
 *   DEBUG    — text-muted-foreground
 *   DONE     — text-success
 *   EVIDENCE — text-evidence
 *   WARN     — text-warning
 *   ERROR    — text-destructive
 *   PULSE    — text-pulse (rare; reserved for genuine wake-on-event
 *              callouts, never decorative)
 *
 * The component is a thin <span> wrapper so a single log line can mix
 * a colored severity tag with monochrome metadata text:
 *
 *   <LogLine severity="evidence">
 *     2026-05-08T03:14:23.802Z <LogToken severity="evidence">EVIDENCE</LogToken>{" "}
 *     cd_logs:281-294 — "npm ERR! ENOSPC: no space left on device"
 *   </LogLine>
 */

export type LogSeverity =
  | "info"
  | "debug"
  | "done"
  | "evidence"
  | "warn"
  | "error"
  | "pulse";

const severityClass: Record<LogSeverity, string> = {
  info: "text-foreground",
  debug: "text-muted-foreground",
  done: "text-success",
  evidence: "text-evidence",
  warn: "text-warning",
  error: "text-destructive",
  pulse: "text-pulse",
};

export type LogLineProps = ComponentProps<"div"> & {
  severity?: LogSeverity;
};

export function LogLine({
  severity = "info",
  className,
  ...rest
}: LogLineProps) {
  return (
    <div
      className={cn("font-mono", severityClass[severity], className)}
      {...rest}
    />
  );
}

export type LogTokenProps = ComponentProps<"span"> & {
  severity?: LogSeverity;
};

/*
 * LogToken — inline colored span for the severity tag itself
 * (e.g. `INFO` / `EVIDENCE` / `DONE`) when the rest of the line
 * stays monochrome. Mirrors the preview's pattern of coloring the
 * severity word + leaving the timestamp + metadata in --text.
 */
export function LogToken({
  severity = "info",
  className,
  ...rest
}: LogTokenProps) {
  return (
    <span
      className={cn("font-mono", severityClass[severity], className)}
      {...rest}
    />
  );
}

export default LogLine;
