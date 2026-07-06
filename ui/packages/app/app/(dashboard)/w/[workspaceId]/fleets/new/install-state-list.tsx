"use client";

import type { ReactNode } from "react";
import { Button, Spinner, TerminalPanel, cn } from "@agentsfleet/design-system";
import type { StateLine } from "./install-flow";

// Shared terminal-native presentational shell for the install states. Lives in
// its own module so both the pre-create flow (InstallStates) and the live-event
// steps (InstallStreamSteps) can compose it without an import cycle.

export function InstallShell({
  title,
  onBack,
  children,
}: {
  title: string;
  onBack: () => void;
  children: ReactNode;
}) {
  return (
    <div className="space-y-4">
      <Button type="button" variant="link" size="sm" onClick={onBack}>
        ← Back to library
      </Button>
      <TerminalPanel title={title} tag="states">
        {children}
      </TerminalPanel>
      <div className="flex flex-wrap items-center gap-md">
        <Spinner size="sm" label="installing" />
        <p className="text-body-sm leading-body-sm text-muted-foreground">
          You can leave this page — your fleet keeps installing and shows up in Fleets when it&apos;s ready.
        </p>
      </div>
    </div>
  );
}

const TONE_CLASS: Record<StateLine["tone"], string> = {
  run: "text-pulse",
  ok: "text-success",
  err: "text-destructive",
  wait: "text-warning",
};

// Renders the ordered state lines as a semantic list. Shared by the pre-create
// flow and the live-event steps so the terminal aesthetic is one source.
export function StateList({ lines }: { lines: StateLine[] }) {
  return (
    <ul aria-label="Install states" data-terminal-reveal className="m-0 list-none p-0">
      {lines.map((line) => (
        <li
          key={line.id}
          data-tone={line.tone}
          className="flex items-center gap-md border-b border-border px-lg py-md text-sm last:border-b-0"
        >
          <span className={cn("w-4 text-center font-mono", TONE_CLASS[line.tone])} aria-hidden="true">
            {line.tone === "run" ? (
              <Spinner size="sm" srLabel="Running" className="justify-center text-pulse" />
            ) : (
              line.glyph
            )}
          </span>
          <span>{line.text}</span>
        </li>
      ))}
    </ul>
  );
}
