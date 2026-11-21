"use client";

import { TIME_LABEL } from "@/lib/time-labels";
import { useId } from "react";
import nextDynamic from "next/dynamic";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  type AlertVariant,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  formatTimeAbsoluteWithZone,
  Section,
  Time,
} from "@agentsfleet/design-system";
import type { Icon as PhosphorIcon } from "@phosphor-icons/react";
import { PulseIcon, WarningIcon, CheckCircleIcon, XCircleIcon } from "@phosphor-icons/react/ssr";
import type { TurnDetail, TurnLocator, TurnRow } from "@/lib/api/turns";
import { byInstant } from "@/lib/api/decode";
import { useTurnDetail } from "./use-turn-detail";
import {
  formatRequestValue,
  isRequestContextRecord,
  parseRequestContext,
  presentRequestLabel,
  previewRequestEntries,
} from "./turn-request-context";
import {
  failurePresentationFor,
  guidanceFor,
  outcomeFor,
  SENDER,
  senderLabelFor,
} from "@/lib/turns/turn-summary";

type TurnDetailsDialogProps = {
  row: TurnRow | TurnLocator | null;
  refresh?: string;
  onOpenChange: (open: boolean) => void;
};

// Keep the markdown runtime out of initial route bundles, as in fleet chat.
const FleetMarkdown = nextDynamic(
  () => import("../shared/chat/FleetMarkdown").then((mod) => mod.FleetMarkdown),
  { ssr: false },
);

type TurnTone = {
  alertVariant: AlertVariant;
  iconLabel: string;
  Icon: PhosphorIcon;
};

const TURN_DETAILS_TITLE = "Turn details";
const TURN_RESULT_MAX_CHARS = 20_000;
const FIX_TITLE = "Fix";
// Shown only when the row predates the recorded failure cause — with a cause
// present the guidance above is already specific enough to act on.
const NO_CAUSE_ADVICE =
  "This turn did not record which check failed. Retry it once; if it fails again, ask a workspace administrator to investigate.";
const NO_RESULT = "No result recorded";
const RESULT_UNCONFIRMED = "Latest result not yet available";
const NO_REQUEST_CONTEXT = "No request context recorded";
const REQUEST_CONTEXT_TITLE = "Request context";
const REQUEST_CONTEXT_OMITTED = "Additional fields not shown";
const RUNNER_DIAGNOSTIC_TITLE = "Runner diagnostic";
const SOURCE_LABEL = "Source:";
const TYPE_LABEL = "Type:";

const WARNING_TONE: TurnTone = {
  alertVariant: "warning",
  iconLabel: "Turn with a warning",
  Icon: WarningIcon,
};

const TURN_TONES: Record<string, TurnTone> = {
  processed: {
    alertVariant: "success",
    iconLabel: "Successful turn",
    Icon: CheckCircleIcon,
  },
  fleet_error: {
    alertVariant: "destructive",
    iconLabel: "Failed turn",
    Icon: XCircleIcon,
  },
  gate_blocked: WARNING_TONE,
  received: {
    alertVariant: "info",
    iconLabel: "Turn in progress",
    Icon: PulseIcon,
  },
};

export function TurnDetailsDialog({ row, refresh, onOpenChange }: TurnDetailsDialogProps) {
  // Fetched here rather than in `TurnDetails` so the request is keyed on the
  // open row and is torn down with the dialog, not with a render.
  const supplied = row && "status" in row ? row : null;
  const revision = refresh ?? (supplied ? JSON.stringify([supplied.updated_at, supplied.status]) : undefined);
  const loaded = useTurnDetail(row, revision);
  const detail = loaded.kind === "ready" ? loaded.detail : null;
  const known = supplied && (!detail || byInstant(supplied.updated_at, detail.updated_at) > 0) ? supplied : detail;
  const current = known === detail ? detail : null;
  return (
    <Dialog open={row !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-svh max-w-3xl overflow-y-auto">
        {known ? <TurnDetails row={known} detail={current} /> : row ? <DialogHeader>
          <DialogTitle>{TURN_DETAILS_TITLE}</DialogTitle>
          <DialogDescription>{loaded.kind === "failed" ? "Run details unavailable. Close and try again." : "Loading run details…"}</DialogDescription>
        </DialogHeader> : null}
      </DialogContent>
    </Dialog>
  );
}

function TurnDetails({ row, detail }: { row: TurnRow; detail: TurnDetail | null }) {
  const response = boundedResponse(detail?.response_text ?? null);
  const failure = row.failure_label ? failurePresentationFor(row.failure_label) : null;
  // The chat and result keep a plain sentence; inspect gives operators the
  // recorded runner cause without presenting it as retry guidance.
  const recordedResult = response || (failure ? outcomeFor(row) : "") || (detail ? NO_RESULT : RESULT_UNCONFIRMED);
  const result = truncateResult(recordedResult);
  const runnerDiagnostic = row.failure_label === "runner_crash" ? row.failure_detail?.trim() : null;
  const tone = TURN_TONES[row.status] ?? WARNING_TONE;
  const guidance = response ? null : guidanceFor(row.failure_label);

  return (
    <>
      <TurnDetailsHeader row={row} result={result} />
      <div className="space-y-lg pt-lg">
        <TurnResult result={result} tone={tone} />
        {runnerDiagnostic ? <RunnerDiagnostic detail={runnerDiagnostic} /> : null}
        <RequestContext row={row} detail={detail} />
        {guidance ? <StartupFix guidance={guidance} hasCause={hasCause(row)} /> : null}
      </div>
    </>
  );
}

function RunnerDiagnostic({ detail }: { detail: string }) {
  const titleId = useId();
  return (
    <Section aria-labelledby={titleId} className="gap-md">
      <h3 id={titleId} className="text-label font-medium tracking-label text-muted-foreground">
        {RUNNER_DIAGNOSTIC_TITLE}
      </h3>
      <RequestContextFallback>{detail}</RequestContextFallback>
    </Section>
  );
}

function TurnDetailsHeader({ row, result }: { row: TurnRow; result: string }) {
  const actor = senderLabelFor(row.actor);
  const created = new Date(row.created_at);
  return (
    <DialogHeader className="gap-lg border-b border-border pb-lg pr-4xl">
      <div className="flex flex-col gap-lg sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <DialogTitle>{TURN_DETAILS_TITLE}</DialogTitle>
          <DialogDescription className="sr-only">
            {row.status}: {result}. {actor}, {row.event_type}.
          </DialogDescription>
        </div>
        <span className="flex shrink-0 items-baseline gap-sm text-label text-muted-foreground">
          <span>{TIME_LABEL.created}</span>
          <Time
            value={created}
            format="relative"
            tooltip
            tooltipContent={formatTimeAbsoluteWithZone(created)}
            className="font-mono tabular-nums text-foreground"
          />
        </span>
      </div>
    </DialogHeader>
  );
}

function TurnResult({ result, tone }: { result: string; tone: TurnTone }) {
  const resultId = useId();
  const { Icon } = tone;
  return (
    <Alert variant={tone.alertVariant} className="block" aria-labelledby={resultId}>
      <div className="flex items-start gap-md">
        <Icon size={18} className="mt-xs shrink-0" aria-label={tone.iconLabel} />
        <AlertDescription id={resultId} className="min-w-0 flex-1 text-foreground">
          <FleetMarkdown>{result}</FleetMarkdown>
        </AlertDescription>
      </div>
    </Alert>
  );
}

function RequestContext({ row, detail }: { row: TurnRow; detail: TurnDetail | null }) {
  const contextId = useId();
  const actor = senderLabelFor(row.actor);
  // `null` covers both in-flight and failed: the panel says "not recorded"
  // either way rather than claiming the turn carried no context.
  const context = detail === null ? null : parseRequestContext(detail.request_json);
  return (
    <Section aria-labelledby={contextId} className="gap-md">
      <div className="flex flex-wrap items-center justify-between gap-md">
        <h3 id={contextId} className="text-label font-medium tracking-label text-muted-foreground">
          {REQUEST_CONTEXT_TITLE}
        </h3>
        <div className="flex flex-wrap items-center gap-sm">
          <span className="text-label text-muted-foreground">{SOURCE_LABEL} <span title={row.actor}>{actor}</span></span>
          <span className="text-label text-muted-foreground">{TYPE_LABEL} <span>{row.event_type}</span></span>
        </div>
      </div>
      <RequestContextBody context={context} githubSource={actor === SENDER.GITHUB_APP} />
    </Section>
  );
}

function RequestContextBody({ context, githubSource }: { context: unknown; githubSource: boolean }) {
  if (context === null) {
    return <RequestContextFallback>{NO_REQUEST_CONTEXT}</RequestContextFallback>;
  }
  if (!isRequestContextRecord(context)) {
    return <RequestContextFallback>{formatRequestValue(context)}</RequestContextFallback>;
  }
  const { entries, hasMore } = previewRequestEntries(context);
  if (entries.length === 0) {
    return <RequestContextFallback>{NO_REQUEST_CONTEXT}</RequestContextFallback>;
  }
  return (
    <dl className="max-h-64 overflow-auto rounded-md border border-border bg-muted/30">
      {entries.map(([key, value]) => (
        <div
          key={key}
          className="flex flex-col gap-sm border-b border-border px-lg py-md last:border-b-0 sm:flex-row sm:gap-lg"
        >
          <dt className="shrink-0 text-label capitalize text-muted-foreground sm:w-40">
            {presentRequestLabel(key, githubSource)}
          </dt>
          <dd className="min-w-0 break-words font-mono text-mono leading-mono text-foreground">
            {formatRequestValue(value)}
          </dd>
        </div>
      ))}
      {hasMore ? (
        <div className="px-lg py-md text-xs text-muted-foreground">
          <dt className="sr-only">Preview limit</dt>
          <dd>{REQUEST_CONTEXT_OMITTED}</dd>
        </div>
      ) : null}
    </dl>
  );
}

function RequestContextFallback({ children }: { children: string }) {
  return (
    <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-muted/30 p-lg font-mono text-mono leading-mono text-foreground">
      {children}
    </pre>
  );
}

/**
 * The remediation panel for an actionable failure. A row that recorded its
 * cause gets the direct instruction; one from a runner too old to report a
 * cause keeps the older fall-back advice rather than pretending to know which
 * check failed.
 */
function StartupFix({ guidance, hasCause }: { guidance: string; hasCause: boolean }) {
  return (
    <Alert variant="warning" className="block">
      <AlertTitle>{FIX_TITLE}</AlertTitle>
      <AlertDescription className="space-y-md text-foreground">
        <p>{guidance}</p>
        {hasCause ? null : (
          <p>{NO_CAUSE_ADVICE}</p>
        )}
      </AlertDescription>
    </Alert>
  );
}

function hasCause(row: TurnRow): boolean {
  return (row.failure_detail ?? "").trim().length > 0;
}

function truncateResult(value: string): string {
  if (value.length <= TURN_RESULT_MAX_CHARS) return value;
  return `${value.slice(0, TURN_RESULT_MAX_CHARS - 1)}…`;
}

function boundedResponse(value: string | null): string {
  if (!value) return "";
  const prefix = value.slice(0, TURN_RESULT_MAX_CHARS + 1).trim();
  if (value.length <= TURN_RESULT_MAX_CHARS || !prefix) return prefix;
  if (prefix.length >= TURN_RESULT_MAX_CHARS) return truncateResult(prefix);
  return `${prefix}…`;
}
