"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import {
  listZombieEvents,
  streamZombieEventsUrl,
  FRAME_KIND,
  type EventRow,
  type LiveFrame,
} from "@/lib/api/events";

const RECONNECT_BACKOFF_BASE_MS = 1_000;
const RECONNECT_BACKOFF_CAP_MS = 15_000;
const RECONNECT_MAX_BACKOFF_ATTEMPTS = 5;
const BACKFILL_LIMIT = 50;

export const CONNECTION_STATUS = {
  CONNECTING: "connecting",
  LIVE: "live",
  RECONNECTING: "reconnecting",
} as const;

export type ConnectionStatus = (typeof CONNECTION_STATUS)[keyof typeof CONNECTION_STATUS];

const STATUS_OPTIMISTIC = "optimistic";

export type ZombieEventStatus =
  | "received"
  | "processed"
  | "agent_error"
  | "gate_blocked"
  | typeof STATUS_OPTIMISTIC;

// Single internal representation that backfill rows and live frames both
// collapse into. The boundary to assistant-ui is `convertEvent`, which
// returns a `ThreadMessageLike` and never the other way round.
export type ZombieEvent = {
  id: string;
  role: "user" | "assistant" | "system";
  actor: string;
  text: string;
  createdAt: Date;
  status: ZombieEventStatus;
  custom?: { requestJson?: string | null; reason?: string };
};

export type UseZombieEventStreamResult = {
  events: ZombieEvent[];
  connectionStatus: ConnectionStatus;
  isRunning: boolean;
  appendOptimistic: (text: string, actor: string) => string;
  reconcileOptimistic: (tempId: string, realEventId: string) => void;
  convertEvent: (event: ZombieEvent) => ThreadMessageLike;
};

export function useZombieEventStream(
  workspaceId: string,
  zombieId: string,
  token: string | null,
): UseZombieEventStreamResult {
  const [events, setEvents] = useState<ZombieEvent[]>([]);
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>(
    CONNECTION_STATUS.CONNECTING,
  );
  const tempCounterRef = useRef(0);

  useEffect(() => {
    if (!token) return;
    let cancelled = false;
    void (async () => {
      try {
        const page = await listZombieEvents(workspaceId, zombieId, token, {
          limit: BACKFILL_LIMIT,
        });
        if (!cancelled) setEvents((prev) => mergeBackfill(prev, page.items));
      } catch {
        // Backfill failures don't surface — the live stream's connection
        // state is the authoritative health signal.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [workspaceId, zombieId, token]);

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
        if (!cancelled) setConnectionStatus(CONNECTION_STATUS.LIVE);
      };
      es.onmessage = (e) => {
        let parsed: LiveFrame | null = null;
        try {
          parsed = JSON.parse(e.data) as LiveFrame;
        } catch {
          return;
        }
        if (!parsed || typeof parsed !== "object" || typeof parsed.kind !== "string") {
          return;
        }
        const frame = parsed;
        setEvents((prev) => applyLiveFrame(prev, frame));
      };
      es.onerror = () => {
        es?.close();
        es = null;
        if (cancelled) return;
        setConnectionStatus(CONNECTION_STATUS.RECONNECTING);
        attempts += 1;
        const delayMs = Math.min(
          RECONNECT_BACKOFF_BASE_MS * 2 ** Math.min(attempts, RECONNECT_MAX_BACKOFF_ATTEMPTS),
          RECONNECT_BACKOFF_CAP_MS,
        );
        retryTimer = setTimeout(connect, delayMs);
      };
    };

    connect();
    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
      if (es) es.close();
    };
  }, [workspaceId, zombieId]);

  const appendOptimistic = useCallback((text: string, actor: string): string => {
    tempCounterRef.current += 1;
    const tempId = `optim-${tempCounterRef.current}`;
    setEvents((prev) => [
      ...prev,
      {
        id: tempId,
        role: "user",
        actor,
        text,
        createdAt: new Date(),
        status: STATUS_OPTIMISTIC,
      },
    ]);
    return tempId;
  }, []);

  const reconcileOptimistic = useCallback((tempId: string, realEventId: string) => {
    setEvents((prev) =>
      prev.map((ev) =>
        ev.id === tempId ? { ...ev, id: realEventId, status: "received" } : ev,
      ),
    );
  }, []);

  return {
    events,
    connectionStatus,
    isRunning: events.some((ev) => ev.status === "received"),
    appendOptimistic,
    reconcileOptimistic,
    convertEvent,
  };
}

function mergeBackfill(prev: ZombieEvent[], rows: EventRow[]): ZombieEvent[] {
  const seen = new Set(prev.map((e) => e.id));
  const fromBackfill = rows.filter((r) => !seen.has(r.event_id)).map(rowToEvent);
  return [...fromBackfill, ...prev].sort(
    (a, b) => a.createdAt.getTime() - b.createdAt.getTime(),
  );
}

function rowToEvent(row: EventRow): ZombieEvent {
  return {
    id: row.event_id,
    role: actorToRole(row.actor),
    actor: row.actor,
    text: row.response_text ?? "",
    createdAt: new Date(row.created_at),
    status: row.status as ZombieEventStatus,
    custom: { requestJson: row.request_json },
  };
}

function applyLiveFrame(prev: ZombieEvent[], frame: LiveFrame): ZombieEvent[] {
  switch (frame.kind) {
    case FRAME_KIND.EVENT_RECEIVED:
      return prev.some((e) => e.id === frame.event_id)
        ? prev
        : [
            ...prev,
            {
              id: frame.event_id,
              role: actorToRole(frame.actor),
              actor: frame.actor,
              text: "",
              createdAt: new Date(),
              status: "received",
            },
          ];
    case FRAME_KIND.CHUNK: {
      const existing = prev.find((e) => e.id === frame.event_id);
      if (!existing) {
        return [
          ...prev,
          {
            id: frame.event_id,
            role: "assistant",
            actor: "agent",
            text: frame.text,
            createdAt: new Date(),
            status: "received",
          },
        ];
      }
      return prev.map((e) =>
        e === existing
          ? {
              ...e,
              role: e.role === "user" ? "user" : "assistant",
              text: e.text + frame.text,
            }
          : e,
      );
    }
    case FRAME_KIND.EVENT_COMPLETE: {
      const existing = prev.find((e) => e.id === frame.event_id);
      if (!existing) return prev;
      return prev.map((e) =>
        e === existing
          ? { ...e, status: (frame.status as ZombieEventStatus) ?? "processed" }
          : e,
      );
    }
    default:
      return prev;
  }
}

function actorToRole(actor: string): "user" | "assistant" | "system" {
  if (actor.startsWith("steer:")) return "user";
  if (actor === "agent") return "assistant";
  return "system";
}

function convertEvent(event: ZombieEvent): ThreadMessageLike {
  return {
    role: event.role,
    id: event.id,
    createdAt: event.createdAt,
    content: [{ type: "text", text: event.text }],
    metadata: {
      custom: {
        actor: event.actor,
        requestJson: event.custom?.requestJson,
        reason: event.custom?.reason,
        status: event.status,
      },
    },
  };
}
