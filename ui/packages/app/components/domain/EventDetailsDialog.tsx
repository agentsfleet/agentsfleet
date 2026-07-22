"use client";

import {
  Alert,
  AlertDescription,
  AlertTitle,
  type AlertVariant,
  Badge,
  CopyButton,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  formatTimeAbsolute,
  Section,
  Time,
} from "@agentsfleet/design-system";
import {
  ActivityIcon,
  AlertTriangleIcon,
  CheckCircle2Icon,
  CircleXIcon,
  type LucideIcon,
} from "lucide-react";
import type { EventRow } from "@/lib/api/events";
import { presentEventFailure } from "./eventFailurePresentation";
import { presentFleetActor } from "./fleetActorPresentation";

type EventDetailsDialogProps = {
  row: EventRow | null;
  onOpenChange: (open: boolean) => void;
};

type EventTone = {
  alertVariant: AlertVariant;
  iconLabel: string;
  Icon: LucideIcon;
};

const COPY_DIAGNOSTIC_LABEL = "Copy diagnostic";
const COPIED_REQUEST_CONTEXT_OMITTED =
  "Omitted from copied diagnostic because webhook data may contain private or secret values.";
const COPY_EVENT_ID_LABEL = "Copy event ID";
const CREATED_LABEL = "Created";
const EVENT_DETAILS_TITLE = "Event details";
const EVENT_RESULT_MAX_CHARS = 20_000;
const FIX_TITLE = "Fix";
const LOCAL_TIME_FALLBACK = "Local time";
const NO_RESULT = "No result recorded";
const NO_REQUEST_CONTEXT = "No request context recorded";
const REQUEST_CONTEXT_TITLE = "Request context";
const REQUEST_CONTEXT_MAX_CHARS = 10_000;
const REQUEST_CONTEXT_MAX_ENTRIES = 100;
const REQUEST_CONTEXT_OMITTED = "Additional fields not shown";
const REQUEST_CONTEXT_LABELS: Record<string, string> = {
  action: "Action",
  author: "Author",
  base_ref: "Base branch",
  draft: "Draft",
  head_ref: "Head branch",
  head_sha: "Head commit",
  number: "Number",
  pull_request: "Pull request",
  received_at: "Received",
  repo: "Repository",
  state: "State",
  title: "Title",
};

const WARNING_TONE: EventTone = {
  alertVariant: "warning",
  iconLabel: "Warning event",
  Icon: AlertTriangleIcon,
};

const EVENT_TONES: Record<string, EventTone> = {
  processed: {
    alertVariant: "success",
    iconLabel: "Successful event",
    Icon: CheckCircle2Icon,
  },
  fleet_error: {
    alertVariant: "destructive",
    iconLabel: "Failed event",
    Icon: CircleXIcon,
  },
  gate_blocked: WARNING_TONE,
  received: {
    alertVariant: "info",
    iconLabel: "Event in progress",
    Icon: ActivityIcon,
  },
};

export function EventDetailsDialog({ row, onOpenChange }: EventDetailsDialogProps) {
  return (
    <Dialog open={row !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-svh max-w-3xl overflow-y-auto">
        {row ? <EventDetails row={row} /> : null}
      </DialogContent>
    </Dialog>
  );
}

function EventDetails({ row }: { row: EventRow }) {
  const response = row.response_text?.trim() ?? "";
  const failure = row.failure_label ? presentEventFailure(row.failure_label) : null;
  const recordedResult = response || failure?.label || NO_RESULT;
  const result = truncateResult(recordedResult);
  const tone = EVENT_TONES[row.status] ?? WARNING_TONE;
  const diagnostic = formatEventDetailsForCopy(row, recordedResult);

  return (
    <>
      <EventDetailsHeader row={row} result={result} />
      <div className="space-y-lg pt-lg">
        <EventResult result={result} tone={tone} />
        <RequestContext row={row} />
        {failure?.guidance === "startup" && !response ? <StartupFix /> : null}
        <DialogFooter className="border-t border-border pt-lg">
          <CopyButton value={diagnostic} label={COPY_DIAGNOSTIC_LABEL} />
        </DialogFooter>
      </div>
    </>
  );
}

function EventDetailsHeader({ row, result }: { row: EventRow; result: string }) {
  const actor = presentFleetActor(row.actor);
  const created = new Date(row.created_at);
  return (
    <DialogHeader className="gap-lg border-b border-border pb-lg pr-4xl">
      <div className="flex flex-col gap-lg sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-md">
            <DialogTitle>{EVENT_DETAILS_TITLE}</DialogTitle>
            <div className="flex min-w-0 max-w-full items-center gap-sm rounded-sm border border-border bg-muted/30 px-md py-sm">
              <span className="font-mono text-label uppercase tracking-label text-muted-foreground">ID</span>
              <span className="min-w-0 max-w-xs truncate font-mono text-xs text-foreground" title={row.event_id}>
                {row.event_id}
              </span>
              <CopyButton value={row.event_id} label={COPY_EVENT_ID_LABEL} />
            </div>
          </div>
          <DialogDescription className="sr-only">
            {row.status}: {result}. {actor}, {row.event_type}.
          </DialogDescription>
        </div>
        <span className="flex shrink-0 items-baseline gap-sm font-mono text-label text-muted-foreground">
          <span>{CREATED_LABEL}</span>
          <Time
            value={created}
            format="relative"
            tooltip
            tooltipContent={formatCreatedTooltip(created)}
            className="tabular-nums text-foreground"
          />
        </span>
      </div>
    </DialogHeader>
  );
}

function EventResult({ result, tone }: { result: string; tone: EventTone }) {
  const { Icon } = tone;
  return (
    <Alert variant={tone.alertVariant} className="block" aria-labelledby="event-result">
      <div className="flex items-start gap-md">
        <Icon size={18} className="mt-xs shrink-0" aria-label={tone.iconLabel} />
        <AlertTitle id="event-result" className="whitespace-pre-wrap text-foreground">
          {result}
        </AlertTitle>
      </div>
    </Alert>
  );
}

function RequestContext({ row }: { row: EventRow }) {
  const actor = presentFleetActor(row.actor);
  const context = parseRequestContext(row.request_json);
  return (
    <Section aria-labelledby="request-context" className="gap-md">
      <div className="flex flex-wrap items-center justify-between gap-md">
        <h3 id="request-context" className="font-mono text-label uppercase tracking-label text-muted-foreground">
          {REQUEST_CONTEXT_TITLE}
        </h3>
        <div className="flex flex-wrap items-center gap-sm">
          <Badge title={row.actor}>{actor}</Badge>
          <Badge>{row.event_type}</Badge>
        </div>
      </div>
      <RequestContextBody context={context} githubSource={isGitHubSource(row.actor)} />
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
          <dt className="shrink-0 font-mono text-label capitalize text-muted-foreground sm:w-40">
            {presentRequestLabel(key, githubSource)}
          </dt>
          <dd className="min-w-0 break-words font-mono text-xs leading-mono text-foreground">
            {formatRequestValue(value)}
          </dd>
        </div>
      ))}
      {hasMore ? (
        <div className="px-lg py-md font-mono text-xs text-muted-foreground">
          {REQUEST_CONTEXT_OMITTED}
        </div>
      ) : null}
    </dl>
  );
}

function previewRequestEntries(context: Record<string, unknown>): {
  entries: Array<[string, unknown]>;
  hasMore: boolean;
} {
  const entries = Object.entries(context);
  const hasMore = entries.length > REQUEST_CONTEXT_MAX_ENTRIES;
  return {
    entries: entries.slice(0, REQUEST_CONTEXT_MAX_ENTRIES),
    hasMore,
  };
}

function RequestContextFallback({ children }: { children: string }) {
  return (
    <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-muted/30 p-lg font-mono text-xs leading-mono text-foreground">
      {children}
    </pre>
  );
}

function StartupFix() {
  return (
    <Alert variant="warning" className="block">
      <AlertTitle>{FIX_TITLE}</AlertTitle>
      <AlertDescription className="space-y-md text-foreground">
        <p>Nothing specific can be fixed from this event because it did not record which startup check failed.</p>
        <p>Retry it once. If it fails again, use the copy icon below and ask a coding agent to inspect the diagnostic.</p>
      </AlertDescription>
    </Alert>
  );
}

function formatCreatedTooltip(created: Date): string {
  const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone || LOCAL_TIME_FALLBACK;
  return `${formatTimeAbsolute(created)} · ${timeZone}`;
}

function formatEventDetailsForCopy(row: EventRow, result: string): string {
  const created = new Date(row.created_at);
  return JSON.stringify({
    event_id: row.event_id,
    fleet_id: row.fleet_id,
    workspace_id: row.workspace_id,
    status: row.status,
    result,
    recorded_response: row.response_text,
    created_at: Number.isNaN(created.getTime()) ? String(row.created_at) : created.toISOString(),
    source: {
      actor: row.actor,
      event_type: row.event_type,
    },
    usage: {
      tokens: row.tokens,
      cost_nanos: row.cost_nanos,
      wall_ms: row.wall_ms,
    },
    request_context: copiedRequestContext(row.request_json),
    internal_diagnostics: {
      failure_class: row.failure_label,
      checkpoint_id: row.checkpoint_id,
      resumes_event_id: row.resumes_event_id,
    },
  }, null, 2);
}

function copiedRequestContext(raw: string): string | null {
  return raw.trim() ? COPIED_REQUEST_CONTEXT_OMITTED : null;
}

function parseRequestContext(raw: string): unknown {
  const request = raw.trim();
  if (!request) return null;
  try {
    const parsed: unknown = JSON.parse(request);
    return parsed;
  } catch {
    return request;
  }
}

function isRequestContextRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function presentRequestLabel(key: string, githubSource: boolean): string {
  if (key === "url") return githubSource ? "Pull request" : "URL";
  return REQUEST_CONTEXT_LABELS[key] ?? key.replaceAll("_", " ");
}

function isGitHubSource(actor: string): boolean {
  const normalized = actor.trim().toLowerCase();
  return normalized === "github-app" || normalized === "webhook:github";
}

function truncateResult(value: string): string {
  if (value.length <= EVENT_RESULT_MAX_CHARS) return value;
  return `${value.slice(0, EVENT_RESULT_MAX_CHARS - 1)}…`;
}

function formatRequestValue(value: unknown): string {
  if (value === null) return "—";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "string") return value.slice(0, REQUEST_CONTEXT_MAX_CHARS);
  if (typeof value === "number") return String(value);
  return String(JSON.stringify(value)).slice(0, REQUEST_CONTEXT_MAX_CHARS);
}
