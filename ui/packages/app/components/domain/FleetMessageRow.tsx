"use client";

import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { BracesIcon, CircleXIcon } from "lucide-react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  Badge,
  Time,
  cn,
  formatTimeAbsolute,
  formatTimeRelative,
} from "@agentsfleet/design-system";

const ROW_ENTER =
  "motion-safe:animate-in motion-safe:fade-in-0 motion-safe:duration-stream";
const RELATIVE_TIME_REFRESH_MS = 30_000;

export const ROW_TONE = {
  OPERATOR: "operator",
  FLEET: "fleet",
} as const;

export type RowTone = (typeof ROW_TONE)[keyof typeof ROW_TONE];

// The console's own fleet, so a fleet reply is labelled with the fleet's name
// rather than the word "fleet". Rows are rendered by a callback the thread
// primitive owns, so the name reaches them through context rather than props.
const FleetNameContext = createContext<string>("");
const RelativeNowContext = createContext<Date | null>(null);

export function FleetNameProvider({
  fleetName,
  children,
}: {
  fleetName: string;
  children: ReactNode;
}) {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const timer = window.setInterval(
      () => setNow(new Date()),
      RELATIVE_TIME_REFRESH_MS,
    );
    return () => window.clearInterval(timer);
  }, []);

  return (
    <FleetNameContext.Provider value={fleetName}>
      <RelativeNowContext.Provider value={now}>
        {children}
      </RelativeNowContext.Provider>
    </FleetNameContext.Provider>
  );
}

export function useFleetName(): string {
  return useContext(FleetNameContext);
}

export type FleetMessageRowProps = {
  /** Accessible sender label retained when visible conversation chrome is absent. */
  sender: string;
  tone: RowTone;
  children: ReactNode;
  /** Transient delivery state rendered above the operator bubble. */
  annotation?: ReactNode;
  /** The message's conversational role. Named apart from the ARIA `role`
   * attribute it would otherwise be mistaken for; it lands on `data-role`. */
  messageRole: string;
  dimmed?: boolean;
  failed?: boolean;
};

export function FleetMessageRow({
  sender,
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
      <div
        className={cn(
          "flex w-full px-lg py-md",
          isOperator ? "justify-end" : "justify-start",
        )}
      >
        <div
          className={cn(
            "flex min-w-0 max-w-prose flex-col gap-xs",
            isOperator ? "items-end" : "w-full items-start",
          )}
        >
          {annotation ? (
            <div className="font-mono text-label text-muted-foreground">
              {annotation}
            </div>
          ) : null}
          <div
            className={cn(
              "min-w-0 max-w-full break-words font-mono text-mono leading-mono text-foreground",
              isOperator
                ? "w-fit rounded-lg rounded-br-sm border border-border-strong bg-accent px-md py-sm"
                : "w-full",
            )}
          >
            <span className="sr-only">{sender}: </span>
            {children}
          </div>
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
      data-role={messageRole}
      data-compact="true"
      data-failed={failed || undefined}
      className={cn("w-full border-b border-border", ROW_ENTER)}
    >
      <div className="flex min-w-0 items-start gap-md px-lg py-md">
        <div className="min-w-0 flex-1 font-mono leading-mono">
          <div className="flex min-w-0 flex-wrap items-center gap-sm text-label">
            <span className="shrink-0 text-muted-foreground">{sender}</span>
            {annotation}
            <span aria-hidden="true" className="text-muted-foreground">
              {TICK_SEPARATOR}
            </span>
            <span
              className="min-w-0 break-words text-muted-foreground"
              title={headline}
            >
              {headline}
            </span>
          </div>
          {outcome ? (
            <div className="mt-xs">
              <p
                className={cn(
                  "font-mono",
                  failed
                    ? "flex min-h-6 items-start gap-xs text-label font-medium leading-label text-foreground"
                    : "text-mono leading-mono text-muted-foreground",
                )}
              >
                {failed ? (
                  <>
                    <span className="flex size-4 shrink-0 items-center justify-center">
                      <CircleXIcon
                        size={12}
                        className="text-destructive"
                        aria-hidden="true"
                      />
                    </span>
                    <span>{outcome}</span>
                  </>
                ) : (
                  outcome
                )}
              </p>
            </div>
          ) : null}
          {children ? (
            <Accordion type="single" collapsible className="w-fit">
              <AccordionItem value={DETAILS_VALUE} className="border-0">
                <AccordionTrigger className="min-h-11 w-fit flex-none gap-xs py-0 font-mono text-label leading-none text-muted-foreground hover:no-underline sm:min-h-6 [&>svg]:ml-0 [&>svg]:size-3">
                  <span className="flex size-4 shrink-0 items-center justify-center">
                    <BracesIcon size={12} aria-hidden="true" />
                  </span>
                  {DETAILS_LABEL}
                </AccordionTrigger>
                <AccordionContent>{children}</AccordionContent>
              </AccordionItem>
            </Accordion>
          ) : null}
        </div>
        <Timestamp createdAt={createdAt} />
      </div>
    </div>
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
      className={cn("w-full border-b border-border", ROW_ENTER)}
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
          <AccordionTrigger className="px-lg py-md font-mono text-label leading-mono text-muted-foreground hover:no-underline">
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

// Relative time stays visual-only so the 30-second refresh cannot repeatedly
// announce the live region. Assistive technology gets one stable exact instant.
function Timestamp({ createdAt }: { createdAt: Date }) {
  const now = useContext(RelativeNowContext);
  const absolute = useMemo(() => formatTimeAbsolute(createdAt), [createdAt]);

  return (
    <>
      <Time
        aria-hidden="true"
        value={createdAt}
        format="relative"
        label={now ? formatTimeRelative(createdAt, now) : undefined}
        tooltip={false}
        title={absolute}
        className="shrink-0 font-mono text-label leading-mono text-muted-foreground tabular-nums"
      />
      <span className="sr-only">Occurred {absolute}</span>
    </>
  );
}
