"use client";

import { createContext, useContext, type ReactNode } from "react";
import { cn } from "@agentsfleet/design-system";
import { senderInitialsFor } from "@/lib/events/event-summary";

// The approved conversation row (designs/fleet-workspace-20260721/variant-A):
// a sender chip, the sender's name, the time on the far right, the body
// underneath at full width, and a hairline between rows. Every role renders
// this same shape — an operator message, a fleet reply and an integration
// event differ in their chip tone and their body, never in their skeleton.

const ROW_ENTER = "animate-in fade-in-0 duration-150";

// Which side of the conversation a row belongs to. Drives only the chip tone;
// the layout is identical for all three so the thread reads as one column.
export const ROW_TONE = {
  OPERATOR: "operator",
  FLEET: "fleet",
  EVENT: "event",
} as const;

export type RowTone = (typeof ROW_TONE)[keyof typeof ROW_TONE];

const CHIP_TONE: Record<RowTone, string> = {
  [ROW_TONE.OPERATOR]: "border-border-strong text-foreground",
  [ROW_TONE.FLEET]: "border-pulse/40 text-pulse",
  [ROW_TONE.EVENT]: "border-border text-muted-foreground",
};

// The visible clock label is browser-local — an operator scans their own
// timeline in their own zone — and the machine-readable instant rides the
// `dateTime` attribute, so the exact moment is never lost to formatting.
const CLOCK_FORMAT = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
});

// The console's own fleet, so a fleet reply is labelled with the fleet's name
// rather than the word "fleet". Rows are rendered by a callback the thread
// primitive owns, so the name reaches them through context rather than props.
const FleetNameContext = createContext<string>("");

export function FleetNameProvider({
  fleetName,
  children,
}: {
  fleetName: string;
  children: ReactNode;
}) {
  return <FleetNameContext.Provider value={fleetName}>{children}</FleetNameContext.Provider>;
}

export function useFleetName(): string {
  return useContext(FleetNameContext);
}

export type FleetMessageRowProps = {
  sender: string;
  createdAt: Date;
  tone: RowTone;
  children: ReactNode;
  /** Rendered on the header line, right of the sender — status, chips. */
  annotation?: ReactNode;
  role: string;
  dimmed?: boolean;
  failed?: boolean;
};

export function FleetMessageRow({
  sender,
  createdAt,
  tone,
  children,
  annotation,
  role,
  dimmed,
  failed,
}: FleetMessageRowProps) {
  return (
    <div
      className={cn(
        "flex items-start gap-md border-b border-border px-xl py-lg",
        "hover:bg-card",
        ROW_ENTER,
        dimmed && "opacity-60",
      )}
      data-role={role}
      data-optimistic={dimmed || undefined}
      data-failed={failed || undefined}
    >
      <SenderChip sender={sender} tone={tone} />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-md">
          <span className="min-w-0 truncate font-mono text-mono text-foreground">{sender}</span>
          {annotation}
          <span className="flex-1" />
          <Timestamp createdAt={createdAt} />
        </div>
        <div className="mt-xs min-w-0 break-words font-mono text-mono leading-mono text-foreground">
          {children}
        </div>
      </div>
    </div>
  );
}

function SenderChip({ sender, tone }: { sender: string; tone: RowTone }) {
  return (
    <span
      aria-hidden="true"
      data-chip={tone}
      className={cn(
        "inline-flex size-7 shrink-0 items-center justify-center",
        "rounded-sm border bg-surface-deep",
        "font-mono text-label tracking-label",
        CHIP_TONE[tone],
      )}
    >
      {senderInitialsFor(sender)}
    </span>
  );
}

function Timestamp({ createdAt }: { createdAt: Date }) {
  return (
    <time
      className="shrink-0 font-mono text-label leading-mono text-muted-foreground tabular-nums"
      dateTime={createdAt.toISOString()}
    >
      {CLOCK_FORMAT.format(createdAt)}
    </time>
  );
}
