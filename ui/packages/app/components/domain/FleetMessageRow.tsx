"use client";

import { createContext, useContext, type ReactNode } from "react";
import { Time, cn } from "@agentsfleet/design-system";
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
  /** The message's conversational role. Named apart from the ARIA `role`
   * attribute it would otherwise be mistaken for; it lands on `data-role`. */
  messageRole: string;
  dimmed?: boolean;
  failed?: boolean;
};

export function FleetMessageRow({
  sender,
  createdAt,
  tone,
  children,
  annotation,
  messageRole,
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
      data-role={messageRole}
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

export type FleetActivityRowProps = {
  /** Who the delivery came from — a word, never an identifier. */
  sender: string;
  createdAt: Date;
  /** The one-line headline: what arrived. */
  headline: string;
  /** How it ended — rendered muted after the headline, omitted while working. */
  outcome?: string;
  /** True when the outcome is a failure, which is the one thing that shouts. */
  failed?: boolean;
  /** Rendered inline after the headline — an action `Badge`, a link. */
  annotation?: ReactNode;
  /** Disclosure and any expansion, rendered under the tick line. */
  children?: ReactNode;
  messageRole: string;
};

/**
 * The compact "rail tick" an integration delivery renders as (approved variant
 * B). Same chronological column as the conversation rows and in the same
 * order — activity recedes visually, it never moves. One line: sender,
 * headline, outcome, time. No chip, no second outcome row.
 */
export function FleetActivityRow({
  sender,
  createdAt,
  headline,
  outcome,
  failed,
  annotation,
  children,
  messageRole,
}: FleetActivityRowProps) {
  return (
    <div
      className={cn(
        "border-b border-border px-xl py-sm",
        "hover:bg-card",
        ROW_ENTER,
      )}
      data-role={messageRole}
      data-compact="true"
      data-failed={failed || undefined}
    >
      <div className="flex items-baseline gap-sm font-mono text-label leading-mono text-muted-foreground">
        <span className="shrink-0">{sender}</span>
        <span aria-hidden="true">{TICK_SEPARATOR}</span>
        <span className="min-w-0 truncate text-foreground">{headline}</span>
        {annotation}
        {outcome ? (
          <>
            <span aria-hidden="true">{TICK_SEPARATOR}</span>
            <span className={cn("min-w-0 truncate", failed && "text-destructive")}>{outcome}</span>
          </>
        ) : null}
        <span className="flex-1" />
        <Timestamp createdAt={createdAt} />
      </div>
      {children}
    </div>
  );
}

const TICK_SEPARATOR = "·";

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

// Time of day alone: every row in a conversation shares its day, so the date
// would be repeated noise. The canonical instant still rides the `datetime`
// attribute. No tooltip — one Radix instance per message is a real cost on a
// long thread, and the exact moment is already in the markup.
function Timestamp({ createdAt }: { createdAt: Date }) {
  return (
    <Time
      value={createdAt}
      format="clock"
      className="shrink-0 font-mono text-label leading-mono text-muted-foreground tabular-nums"
    />
  );
}
