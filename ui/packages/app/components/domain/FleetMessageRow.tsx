"use client";

import { createContext, useContext, type ReactNode } from "react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  DashboardRow,
  DashboardRowGroup,
  Time,
  cn,
} from "@agentsfleet/design-system";
import { senderInitialsFor } from "@/lib/events/event-summary";

// The approved conversation row (designs/fleet-workspace-20260721/variant-A):
// a sender chip, the sender's name, the time on the far right, the body
// underneath at full width, and a hairline between rows. Every role renders
// this same shape — an operator message, a fleet reply and an integration
// event differ in their chip tone and their body, never in their skeleton.

const ROW_ENTER = "animate-in fade-in-0 motion-safe:slide-in-from-bottom-1 duration-150";

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
  const isOperator = tone === ROW_TONE.OPERATOR;
  return (
    <div
      className={cn("w-full", ROW_ENTER, dimmed && "opacity-60")}
      data-role={messageRole}
      data-optimistic={dimmed || undefined}
      data-failed={failed || undefined}
    >
      <DashboardRow
        data-dashboard-row=""
        icon={<SenderChip sender={sender} tone={tone} />}
        title={
          <div className="flex min-w-0 items-center gap-sm">
            <span className="min-w-0 truncate font-mono text-label text-foreground">{sender}</span>
            {annotation}
          </div>
        }
        description={
          <div className="min-w-0 break-words font-mono text-mono leading-mono text-foreground">
            {children}
          </div>
        }
        action={<Timestamp createdAt={createdAt} />}
        className={cn(
          "min-w-0",
          isOperator
            ? "ml-auto w-full max-w-4xl rounded-lg border border-border bg-card"
            : "w-full max-w-5xl",
        )}
      />
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
  /** Remediation shown directly below a failure, outside the details disclosure. */
  guidance?: ReactNode;
  /** Rendered inline after the headline — an action `Badge`, a link. */
  annotation?: ReactNode;
  /** Disclosure and any expansion, rendered under the tick line. */
  children?: ReactNode;
  messageRole: string;
};

/**
 * A compact evidence line for an integration delivery. It stays in the same
 * chronological column as conversation turns, while verbose guidance and
 * payloads remain available behind one disclosure.
 */
export function FleetActivityRow({
  sender,
  createdAt,
  headline,
  outcome,
  failed,
  guidance,
  annotation,
  children,
  messageRole,
}: FleetActivityRowProps) {
  return (
    <DashboardRow
      data-dashboard-row=""
      data-role={messageRole}
      data-compact="true"
      data-failed={failed || undefined}
      icon={<SenderChip sender={sender} tone={ROW_TONE.EVENT} />}
      title={
        <div className="min-w-0 font-mono leading-mono">
          <div className="flex min-w-0 items-center gap-sm text-label text-muted-foreground">
            <span className="shrink-0">{sender}</span>
            {annotation}
          </div>
          <div className="mt-xs min-w-0 break-words text-mono text-foreground" title={headline}>
            {headline}
          </div>
        </div>
      }
      description={
        outcome || guidance ? (
          <div>
            {outcome ? (
          <p
            className={cn(
              "font-mono text-mono leading-mono",
              failed ? "text-destructive" : "text-muted-foreground",
            )}
          >
            {outcome}
          </p>
            ) : null}
            {guidance}
          </div>
        ) : undefined
      }
      meta={
        children ? (
          <Accordion type="single" collapsible>
            <AccordionItem value={DETAILS_VALUE} className="border-0">
              <AccordionTrigger className="py-xs font-mono text-label text-muted-foreground hover:no-underline">
                {DETAILS_LABEL}
              </AccordionTrigger>
              <AccordionContent>{children}</AccordionContent>
            </AccordionItem>
          </Accordion>
        ) : undefined
      }
      action={<Timestamp createdAt={createdAt} />}
      className={cn("w-full", ROW_ENTER)}
    >
    </DashboardRow>
  );
}

const TICK_SEPARATOR = "·";
const DETAILS_LABEL = "Details";
const DETAILS_VALUE = "details";

export type FleetGroupRowProps = {
  sender: string;
  headline: string;
  outcome?: string;
  failed?: boolean;
  /** How many deliveries this row stands for — always ≥ 2. */
  count: number;
  /** The span the run covers, rendered as "11:38–12:03". */
  first: Date;
  last: Date;
  expanded: boolean;
  onToggle: () => void;
  /** The individual rows, rendered only while expanded. */
  children?: ReactNode;
};

/**
 * A run of identical deliveries as one row: "headline ×N · first–last".
 * Collapsed by default and expandable in place, so the count is a summary the
 * operator can always open — never a replacement for the events themselves.
 */
export function FleetGroupRow({
  sender,
  headline,
  outcome,
  failed,
  count,
  first,
  last,
  expanded,
  onToggle,
  children,
}: FleetGroupRowProps) {
  return (
    <div className={cn("w-full", ROW_ENTER)} data-role="system" data-group="true">
      <DashboardRowGroup>
        <Accordion
          type="single"
          collapsible
          value={expanded ? GROUP_VALUE : ""}
          onValueChange={onToggle}
        >
          <AccordionItem value={GROUP_VALUE} className="border-0">
            <AccordionTrigger className="px-lg py-md font-mono text-label leading-mono text-muted-foreground hover:no-underline">
              <span className="flex min-w-0 flex-1 flex-wrap items-baseline gap-sm text-left">
                <span
                  className="shrink-0 rounded-sm border border-border px-xs text-foreground tabular-nums"
                  data-testid="group-count"
                >
                  ×{count}
                </span>
                <span className="shrink-0">{sender}</span>
                <span aria-hidden="true">{TICK_SEPARATOR}</span>
                <span className="min-w-0 break-words text-foreground">{headline}</span>
                {outcome ? (
                  <>
                    <span aria-hidden="true">{TICK_SEPARATOR}</span>
                    <span className={cn("min-w-0 break-words", failed && "text-destructive")}>
                      {outcome}
                    </span>
                  </>
                ) : null}
                <span className="ml-auto shrink-0 tabular-nums">
                  <Time value={first} format="clock" className="font-mono text-label text-muted-foreground" />
                  {RANGE_SEPARATOR}
                  <Time value={last} format="clock" className="font-mono text-label text-muted-foreground" />
                </span>
              </span>
            </AccordionTrigger>
            <AccordionContent className="px-lg">{children}</AccordionContent>
          </AccordionItem>
        </Accordion>
      </DashboardRowGroup>
    </div>
  );
}

const RANGE_SEPARATOR = "–";
const GROUP_VALUE = "group";

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
