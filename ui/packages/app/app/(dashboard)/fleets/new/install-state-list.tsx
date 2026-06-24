"use client";

import type { ReactNode } from "react";
import { Button, cn } from "@agentsfleet/design-system";
import type { StateLine } from "./install-flow";

// Shared terminal-native presentational shell for the install states. Lives in
// its own module so both the pre-create flow (InstallStates) and the SSE-driven
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
      <Button type="button" variant="ghost" size="sm" onClick={onBack}>
        ← Back to templates
      </Button>
      <div className="overflow-hidden rounded-md border border-border bg-surface-deep">
        <div className="flex items-center gap-md border-b border-border px-lg py-md">
          <span className="font-mono text-label text-muted-foreground">{title}</span>
          <span className="ml-auto font-mono text-label uppercase tracking-label text-muted-foreground">
            states
          </span>
        </div>
        {children}
      </div>
      <p className="text-sm text-muted-foreground">
        While it provisions, the fleet shows this state in your Fleets list and on its own page —
        never hidden.
      </p>
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
// flow and the SSE-driven steps so the terminal aesthetic is one source.
export function StateList({ lines }: { lines: StateLine[] }) {
  return (
    <ul aria-label="Install states" className="m-0 list-none p-0">
      {lines.map((line) => (
        <li
          key={line.id}
          data-tone={line.tone}
          className="flex items-center gap-md border-b border-border px-lg py-md text-sm last:border-b-0"
        >
          <span className={cn("w-4 text-center font-mono", TONE_CLASS[line.tone])} aria-hidden="true">
            {line.glyph}
          </span>
          <span>{line.text}</span>
        </li>
      ))}
    </ul>
  );
}
