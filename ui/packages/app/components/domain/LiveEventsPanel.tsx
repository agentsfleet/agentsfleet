"use client";

import { useEffect, useRef, useState } from "react";
import {
  Badge,
  type BadgeVariant,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  List,
  ListItem,
} from "@usezombie/design-system";
import { FRAME_KIND, streamZombieEventsUrl, type FrameKind, type LiveFrame } from "@/lib/api/events";

export type LiveEventsPanelProps = {
  workspaceId: string;
  zombieId: string;
  /** Maximum frames to keep in the rolling buffer. Older frames drop off. */
  bufferSize?: number;
};

const STATUS = {
  CONNECTING: "connecting",
  LIVE: "live",
  RECONNECTING: "reconnecting",
} as const;

type ConnectionStatus = (typeof STATUS)[keyof typeof STATUS];

const STATUS_VARIANT: Record<ConnectionStatus, BadgeVariant> = {
  [STATUS.CONNECTING]: "cyan",
  [STATUS.LIVE]: "green",
  [STATUS.RECONNECTING]: "amber",
};

const STATUS_LABEL: Record<ConnectionStatus, string> = {
  [STATUS.CONNECTING]: "Connecting…",
  [STATUS.LIVE]: "Live",
  [STATUS.RECONNECTING]: "Reconnecting…",
};

const KIND_VARIANT: Record<FrameKind, BadgeVariant> = {
  [FRAME_KIND.EVENT_RECEIVED]: "cyan",
  [FRAME_KIND.TOOL_CALL_STARTED]: "default",
  [FRAME_KIND.TOOL_CALL_PROGRESS]: "default",
  [FRAME_KIND.CHUNK]: "default",
  [FRAME_KIND.TOOL_CALL_COMPLETED]: "green",
  [FRAME_KIND.EVENT_COMPLETE]: "green",
};

// Reconnect tuning: exponential backoff capped both in attempts (so the
// exponent stops growing) and in absolute delay (so a long outage still
// retries in bounded wall time).
const RECONNECT_BACKOFF_BASE_MS = 1_000;
const RECONNECT_BACKOFF_CAP_MS = 15_000;
const RECONNECT_MAX_BACKOFF_ATTEMPTS = 5;

export function LiveEventsPanel({
  workspaceId,
  zombieId,
  bufferSize = 20,
}: LiveEventsPanelProps) {
  const [frames, setFrames] = useState<Array<{ id: string; frame: LiveFrame }>>([]);
  const [status, setStatus] = useState<ConnectionStatus>(STATUS.CONNECTING);
  const idCounter = useRef(0);

  useEffect(() => {
    let es: EventSource | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;
    let attempts = 0;
    let cancelled = false;

    const url = streamZombieEventsUrl(workspaceId, zombieId);

    const connect = () => {
      if (cancelled) return;
      es = new EventSource(url);
      es.onopen = () => {
        attempts = 0;
        if (!cancelled) setStatus(STATUS.LIVE);
      };
      es.onmessage = (e) => {
        let parsed: LiveFrame | null = null;
        try {
          parsed = JSON.parse(e.data) as LiveFrame;
        } catch {
          return;
        }
        if (!parsed || typeof parsed !== "object" || typeof parsed.kind !== "string") return;
        idCounter.current += 1;
        const id = `${idCounter.current}`;
        setFrames((prev) => {
          const next = [...prev, { id, frame: parsed as LiveFrame }];
          return next.length > bufferSize ? next.slice(-bufferSize) : next;
        });
      };
      es.onerror = () => {
        es?.close();
        es = null;
        if (cancelled) return;
        setStatus(STATUS.RECONNECTING);
        attempts += 1;
        const delayMs = Math.min(
          RECONNECT_BACKOFF_BASE_MS * 2 ** Math.min(attempts, RECONNECT_MAX_BACKOFF_ATTEMPTS),
          RECONNECT_BACKOFF_CAP_MS,
        );
        retryTimer = setTimeout(connect, delayMs);
      };
    };

    // status defaults to "connecting" via useState; onopen/onerror drive
    // every subsequent transition asynchronously so we never setState
    // synchronously inside the effect body.
    connect();

    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
      if (es) es.close();
    };
  }, [workspaceId, zombieId, bufferSize]);

  return (
    <Card asChild>
      <article aria-label="Live activity stream" className="p-4">
        <CardHeader className="flex flex-row items-center justify-between gap-2 space-y-0 p-0 pb-3">
          <CardTitle className="text-sm font-medium">Live activity</CardTitle>
          <Badge variant={STATUS_VARIANT[status]}>{STATUS_LABEL[status]}</Badge>
        </CardHeader>
        <CardContent className="p-0">
          {frames.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              Waiting for activity. Tool calls, chunks, and completions will appear here as the zombie runs.
            </p>
          ) : (
            <List variant="ordered" className="flex flex-col gap-1.5 list-none pl-0 space-y-0">
              {frames.map(({ id, frame }) => (
                <ListItem key={id}>
                  <FrameLine frame={frame} />
                </ListItem>
              ))}
            </List>
          )}
        </CardContent>
      </article>
    </Card>
  );
}

function FrameLine({ frame }: { frame: LiveFrame }) {
  const variant: BadgeVariant = KIND_VARIANT[frame.kind] ?? "default";
  return (
    <div className="flex items-start gap-2 text-sm">
      <Badge variant={variant} className="mt-0.5 font-mono text-[10px] uppercase">
        {frame.kind}
      </Badge>
      <span className="flex-1 truncate font-mono text-xs text-muted-foreground" title={frameSummary(frame)}>
        {frameSummary(frame)}
      </span>
    </div>
  );
}

function frameSummary(frame: LiveFrame): string {
  switch (frame.kind) {
    case FRAME_KIND.EVENT_RECEIVED:
      return `${frame.actor} → ${frame.event_id}`;
    case FRAME_KIND.TOOL_CALL_STARTED:
      return frame.name;
    case FRAME_KIND.TOOL_CALL_PROGRESS:
      return `${frame.name} · ${frame.elapsed_ms}ms`;
    case FRAME_KIND.CHUNK:
      return frame.text;
    case FRAME_KIND.TOOL_CALL_COMPLETED:
      return `${frame.name} · ${frame.ms}ms`;
    case FRAME_KIND.EVENT_COMPLETE:
      return `${frame.event_id} · ${frame.status}`;
    default:
      return "";
  }
}
