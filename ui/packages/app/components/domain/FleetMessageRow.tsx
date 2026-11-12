"use client";

import { createContext, useContext, type ReactNode } from "react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  Badge,
  cn,
} from "@agentsfleet/design-system";

import { RelativeNowProvider, Timestamp } from "./FleetTimestamp";
import { ChatMessageRow } from "./chat/ChatMessageRow";
export {ChatActivityRow as FleetActivityRow, type ChatActivityRowProps as FleetActivityRowProps} from "./chat/ChatActivityRow";

const ROW_ENTER =
  "motion-safe:animate-in motion-safe:fade-in-0 motion-safe:duration-stream";

export const ROW_TONE = {
  OPERATOR: "operator",
  FLEET: "fleet",
} as const;

export type RowTone = (typeof ROW_TONE)[keyof typeof ROW_TONE];

// The console's own fleet, so a fleet reply is labelled with the fleet's name
// rather than the word "fleet". Rows are rendered by a callback the thread
// primitive owns, so the name reaches them through context rather than props.
const SenderLabelContext = createContext<string>("");

export function SenderLabelProvider({
  senderLabel,
  children,
}: {
  senderLabel: string;
  children: ReactNode;
}) {
  return (
    <SenderLabelContext.Provider value={senderLabel}>
      <RelativeNowProvider>{children}</RelativeNowProvider>
    </SenderLabelContext.Provider>
  );
}

export function useSenderLabel(): string {
  return useContext(SenderLabelContext);
}

export type FleetMessageRowProps = {
  /** Accessible sender label retained when visible conversation chrome is absent. */
  sender: string;
  tone: RowTone;
  children: ReactNode;
  /** The message's conversational role. Named apart from the ARIA `role`
   * attribute it would otherwise be mistaken for; it lands on `data-role`. */
  messageRole: string;
  dimmed?: boolean;
  failed?: boolean;
  /** Names the sender above the bubble: someone else's turn in a shared thread. */
  showSender?: boolean;
  /** When the turn was sent, on the row that opens it: one time per turn. */
  createdAt?: Date;
  annotation?: ReactNode;
};

export function FleetMessageRow({
  sender,
  tone,
  children,
  messageRole,
  dimmed,
  failed,
  showSender,
  annotation,
  createdAt,
}: FleetMessageRowProps) {
  return <ChatMessageRow sender={sender} operator={tone === ROW_TONE.OPERATOR} messageRole={messageRole}
    dimmed={dimmed} failed={failed} showSender={showSender} annotation={annotation} timestamp={createdAt ? <Timestamp createdAt={createdAt} compact /> : undefined}>
    {children}
  </ChatMessageRow>;
}

const TICK_SEPARATOR = "·";

export type FleetGroupRowProps = {
  sender: string;
  headline: string;
  outcome?: string;
  failed?: boolean;
  /** How many deliveries this row stands for — always ≥ 2. */
  count: number;
  /** The newest delivery represented by the group. */
  last: Date;
  expanded: boolean;
  onToggle: () => void;
  /** The individual rows, rendered only while expanded. */
  children?: ReactNode;
};

/**
 * A run of identical deliveries as one row: "headline ×N · newest time".
 * Collapsed by default and expandable in place, so the count is a summary the
 * operator can always open — never a replacement for the events themselves.
 */
export function FleetGroupRow({
  sender,
  headline,
  outcome,
  failed,
  count,
  last,
  expanded,
  onToggle,
  children,
}: FleetGroupRowProps) {
  return (
    <div
      className={cn("w-full", ROW_ENTER)}
      data-role="system"
      data-group="true"
      data-failed={failed || undefined}
    >
      <Accordion
        type="single"
        collapsible
        value={expanded ? GROUP_VALUE : ""}
        onValueChange={onToggle}
      >
        <AccordionItem value={GROUP_VALUE} className="border-0">
          <AccordionTrigger className="px-lg py-md font-sans text-conversation leading-mono text-muted-foreground hover:no-underline">
            <span className="flex min-w-0 flex-1 flex-wrap items-baseline gap-sm text-left">
              <Badge
                variant={failed ? "destructive" : "default"}
                className="shrink-0 tabular-nums"
                data-testid="group-count"
              >
                ×{count}
              </Badge>
              <span className="shrink-0">{sender}</span>
              <span aria-hidden="true">{TICK_SEPARATOR}</span>
              <span className="min-w-0 break-words text-foreground">
                {headline}
              </span>
              {outcome ? (
                <>
                  <span aria-hidden="true">{TICK_SEPARATOR}</span>
                  <span
                    className={cn(
                      "min-w-0 break-words",
                      failed && "text-foreground",
                    )}
                  >
                    {outcome}
                  </span>
                </>
              ) : null}
              <span className="ml-auto shrink-0 tabular-nums">
                <Timestamp createdAt={last} />
              </span>
            </span>
          </AccordionTrigger>
          <AccordionContent className="border-t border-border">
            {children}
          </AccordionContent>
        </AccordionItem>
      </Accordion>
    </div>
  );
}

const GROUP_VALUE = "group";
