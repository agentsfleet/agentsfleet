"use client";

import type { CSSProperties, ReactNode } from "react";
import type { MessageState } from "@assistant-ui/react";
import { Badge, cn, type BadgeVariant } from "@usezombie/design-system";

const ACTOR_STEER_PREFIX = "steer:";
const ACTOR_WEBHOOK_PREFIX = "webhook:";
const ACTOR_AGENT = "agent";
const ACTOR_CRON = "cron";
const ACTOR_CONTINUATION = "continuation";
const ACTOR_CONFIG_RELOAD = "config_reload";
const ACTOR_GATE_BLOCKED = "gate_blocked";

const STATUS_OPTIMISTIC = "optimistic";
const STATUS_FAILED = "failed";
const STATUS_AGENT_ERROR = "agent_error";
const RUN_STATUS_RUNNING = "running";

const STEER_GLYPH = "›";
const STREAM_CURSOR = "▍";
const FRAME_ENTER = "animate-in fade-in-0 duration-150";

// Single source of truth for the actor-rail layout. The grid's first
// column is the rail; the webhook payload indent is rail + the parent
// gap. Defined as CSS variables so every dependent class derives from
// one literal — no repeated 72px / 84px arbitraries in the renderers.
const ACTOR_RAIL_VARS = {
  "--actor-rail-w": "72px",
  "--actor-rail-gap": "12px",
} as CSSProperties;
const GRID_2 = "grid-cols-1 md:grid-cols-[var(--actor-rail-w)_1fr]";
const GRID_3 = "grid-cols-1 md:grid-cols-[var(--actor-rail-w)_1fr_auto]";
const PAYLOAD_OFFSET =
  "md:ml-[calc(var(--actor-rail-w)+var(--actor-rail-gap))]";

/**
 * Render function passed to `<ThreadPrimitive.Messages>` in `ZombieThread`.
 * Switches on `message.role`; for system messages, switches further on
 * `metadata.custom.actor` to pick a chip-or-webhook treatment.
 */
export function renderZombieMessage({
  message,
}: {
  message: MessageState;
}): ReactNode {
  if (message.role === "user") return <UserRow message={message} />;
  if (message.role === "assistant") return <AssistantRow message={message} />;
  return <SystemRow message={message} />;
}

// ── Row components ────────────────────────────────────────────────────────

function UserRow({ message }: { message: MessageState }) {
  const actor = readActor(message);
  const text = readText(message);
  const status = readCustomStatus(message);
  const optimistic = status === STATUS_OPTIMISTIC;
  const failed = status === STATUS_FAILED;
  return (
    <div
      style={ACTOR_RAIL_VARS}
      className={cn(
        "grid gap-xs md:gap-lg px-xl py-md",
        GRID_2,
        "hover:bg-card",
        FRAME_ENTER,
        optimistic && "opacity-60",
      )}
      data-role="user"
      data-optimistic={optimistic || undefined}
      data-failed={failed || undefined}
    >
      <ActorRail label={formatActorLabel(actor)} createdAt={message.createdAt} />
      <div className="font-mono text-mono leading-mono text-foreground">
        <span className="text-pulse mr-xs" aria-hidden="true">
          {STEER_GLYPH}
        </span>
        {text}
        {optimistic ? (
          <Badge variant="evidence" className="ml-md">
            queued
          </Badge>
        ) : null}
        {failed ? (
          <Badge variant="destructive" className="ml-md">
            failed
          </Badge>
        ) : null}
      </div>
    </div>
  );
}

function AssistantRow({ message }: { message: MessageState }) {
  const text = readText(message);
  const isError = readCustomStatus(message) === STATUS_AGENT_ERROR;
  const isStreaming = message.status?.type === RUN_STATUS_RUNNING;
  if (isError) return <AssistantErrorRow message={message} text={text} />;
  return (
    <div
      style={ACTOR_RAIL_VARS}
      className={cn(
        "grid gap-xs md:gap-lg px-xl py-md hover:bg-card",
        GRID_2,
        FRAME_ENTER,
      )}
      data-role="assistant"
    >
      <ActorRail
        label="agent"
        createdAt={message.createdAt}
        labelClassName="text-success"
      />
      <div className="text-sm text-foreground">
        {text}
        {isStreaming ? (
          <span className="ml-xs text-pulse animate-pulse" aria-label="streaming">
            {STREAM_CURSOR}
          </span>
        ) : null}
      </div>
    </div>
  );
}

function AssistantErrorRow({
  message,
  text,
}: {
  message: MessageState;
  text: string;
}) {
  return (
    <MetaRow
      createdAt={message.createdAt}
      chipLabel="agent_error"
      chipVariant="destructive"
      content={text}
    />
  );
}

function SystemRow({ message }: { message: MessageState }) {
  const actor = readActor(message);
  const text = readText(message);
  if (actor.startsWith(ACTOR_WEBHOOK_PREFIX)) {
    return <WebhookRow message={message} actor={actor} text={text} />;
  }
  const { chipLabel, chipVariant } = systemChipFor(actor);
  return (
    <MetaRow
      createdAt={message.createdAt}
      chipLabel={chipLabel}
      chipVariant={chipVariant}
      content={text}
    />
  );
}

function WebhookRow({
  message,
  actor,
  text,
}: {
  message: MessageState;
  actor: string;
  text: string;
}) {
  const source = actor.slice(ACTOR_WEBHOOK_PREFIX.length);
  const requestJson = readRequestJson(message);
  return (
    <div
      style={ACTOR_RAIL_VARS}
      className={cn("px-xl py-md", FRAME_ENTER)}
      data-role="system"
      data-system="webhook"
    >
      <details className="group">
        <summary
          className={cn(
            "grid gap-xs md:items-center md:gap-lg",
            GRID_3,
            "cursor-pointer list-none rounded-sm py-xs",
            "font-mono text-mono leading-mono text-foreground",
            "hover:bg-card",
            "[&::-webkit-details-marker]:hidden",
          )}
        >
          <Timestamp createdAt={message.createdAt} />
          <span className="flex items-center gap-md">
            <span
              className={cn(
                "rounded-sm bg-accent px-md py-xs",
                "font-mono text-label font-semibold uppercase tracking-label",
                "text-foreground",
              )}
            >
              {source}
            </span>
            <span className="text-foreground">{text}</span>
          </span>
          <span className="text-muted-foreground font-mono text-label">
            <span className="hidden group-open:inline">▾ collapse</span>
            <span className="group-open:hidden">▸ payload</span>
          </span>
        </summary>
        {requestJson ? (
          <pre
            className={cn(
              "mt-xs overflow-auto rounded-sm border border-border",
              "max-h-64",
              PAYLOAD_OFFSET,
              "bg-surface-deep p-lg",
              "font-mono text-mono leading-mono text-foreground",
            )}
          >
            {requestJson}
          </pre>
        ) : null}
      </details>
    </div>
  );
}

// ── Shared sub-components ────────────────────────────────────────────────

function ActorRail({
  label,
  createdAt,
  labelClassName,
}: {
  label: string;
  createdAt: Date;
  labelClassName?: string;
}) {
  return (
    <div className="font-mono leading-mono">
      <div
        className={cn(
          "text-mono lowercase tracking-label text-muted-foreground",
          labelClassName,
        )}
      >
        {label}
      </div>
      <Timestamp createdAt={createdAt} />
    </div>
  );
}

function MetaRow({
  createdAt,
  chipLabel,
  chipVariant,
  content,
}: {
  createdAt: Date;
  chipLabel: string;
  chipVariant: BadgeVariant;
  content: string;
}) {
  return (
    <div
      style={ACTOR_RAIL_VARS}
      className={cn(
        "grid gap-xs md:items-center md:gap-lg px-xl py-md",
        GRID_3,
        "font-mono text-mono leading-mono text-muted-foreground",
        FRAME_ENTER,
      )}
      data-role="system"
      data-system={chipLabel}
    >
      <Timestamp createdAt={createdAt} />
      <span className="flex items-center gap-md">
        <span
          aria-hidden="true"
          className="inline-block h-px w-lg shrink-0 bg-border-strong"
        />
        <Badge variant={chipVariant}>{chipLabel}</Badge>
        <span className="text-foreground">{content}</span>
      </span>
      <span />
    </div>
  );
}

function Timestamp({ createdAt }: { createdAt: Date }) {
  return (
    <time
      className="block font-mono text-label leading-mono text-muted-foreground"
      dateTime={createdAt.toISOString()}
    >
      {formatTimestamp(createdAt)}
    </time>
  );
}

// ── helpers ──────────────────────────────────────────────────────────────

function readText(message: MessageState): string {
  for (const part of message.content) {
    if (part.type === "text") return part.text;
  }
  return "";
}

function readActor(message: MessageState): string {
  const raw = message.metadata.custom["actor"];
  return typeof raw === "string" ? raw : "";
}

function readCustomStatus(message: MessageState): string {
  const raw = message.metadata.custom["status"];
  return typeof raw === "string" ? raw : "";
}

function readRequestJson(message: MessageState): string | null {
  const raw = message.metadata.custom["requestJson"];
  return typeof raw === "string" && raw.length > 0 ? raw : null;
}

function systemChipFor(actor: string): { chipLabel: string; chipVariant: BadgeVariant } {
  if (actor === ACTOR_CRON) return { chipLabel: ACTOR_CRON, chipVariant: "cyan" };
  if (actor === ACTOR_CONTINUATION) {
    return { chipLabel: ACTOR_CONTINUATION, chipVariant: "cyan" };
  }
  if (actor === ACTOR_GATE_BLOCKED) {
    return { chipLabel: ACTOR_GATE_BLOCKED, chipVariant: "amber" };
  }
  if (actor === ACTOR_CONFIG_RELOAD) {
    return { chipLabel: ACTOR_CONFIG_RELOAD, chipVariant: "default" };
  }
  return { chipLabel: actor || "system", chipVariant: "default" };
}

export function formatActorLabel(actor: string): string {
  if (actor.startsWith(ACTOR_STEER_PREFIX)) {
    const rest = actor.slice(ACTOR_STEER_PREFIX.length);
    const local = rest.split("@", 1)[0] ?? rest;
    const first = local.split(".", 1)[0] ?? local;
    return first.toLowerCase();
  }
  if (actor === ACTOR_AGENT) return ACTOR_AGENT;
  if (actor.startsWith(ACTOR_WEBHOOK_PREFIX)) {
    return `webhook · ${actor.slice(ACTOR_WEBHOOK_PREFIX.length)}`;
  }
  return actor.toLowerCase();
}

function formatTimestamp(d: Date): string {
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}
