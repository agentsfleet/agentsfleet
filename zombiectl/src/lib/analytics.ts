// PostHog-backed CLI analytics. Telemetry is opt-out (default off);
// only DISABLE_TELEMETRY=0|false|off|no opts in. Boundaries below
// stay narrow: every helper handles a single failure mode (env parse,
// client construction, event capture, shutdown) so CLI UX never
// blocks or aborts on a telemetry fault.

export type AnalyticsClient = unknown;

// The posthog-node module exposes `PostHog` as the named export
// constructor. The legacy `default` fallback the JS shim carried has
// no caller in posthog-node 5.x; the named export is the only entry.
type PostHogConstructor = new (
  key: string,
  opts: { host: string; flushAt: number; flushInterval: number },
) => AnalyticsClient & {
  capture: (event: {
    distinctId: string;
    event: string;
    properties: Record<string, string>;
  }) => void;
  shutdown: () => Promise<void>;
};

export interface CliAnalyticsContext {
  readonly client: AnalyticsClient | null;
  readonly distinctId: string;
  readonly queuedEvents: ReadonlyArray<unknown>;
}

export interface HttpRequestInfo {
  url: string;
  method: string;
  status: number | undefined;
  duration_ms: number;
  attempt: number;
  retry_count: number;
}

export interface HttpRetryInfo {
  url: string;
  method: string;
  status: number | undefined;
  attempt: number;
  reason: string;
}

export interface QueuedAnalyticsEvent {
  event: string;
  properties?: Record<string, string>;
}

// analyticsContext is typed `Record<string, unknown>` (not `string`) so that
// CommandCtx — which carries this field through the dispatch lifecycle —
// can flow into set/get/queue without a coercion at the call site. The
// runtime values are always sanitized strings; the wider type just
// accommodates writer-side variance before sanitizeProperties() runs.
interface AnalyticsCtxLike {
  analyticsContext?: Record<string, unknown> | null;
  analyticsEvents?: QueuedAnalyticsEvent[];
}

interface ResolveConfigResult {
  key: string;
  host: string;
  enabled: boolean;
}

const DEFAULT_POSTHOG_HOST = "https://us.i.posthog.com";
const DEFAULT_POSTHOG_KEY = [
  "phc_XmuRIXBST",
  "Rfxka7IgfkU0V",
  "PMD3LDRR3IqIL",
  "XNg3bXzv",
].join("");

function boolFromEnv(value: string | undefined, fallback: boolean): boolean {
  if (value == null || value === "") return fallback;
  const normalized = String(value).trim().toLowerCase();
  return !(normalized === "0" || normalized === "false" || normalized === "off" || normalized === "no");
}

function resolveConfig(env: NodeJS.ProcessEnv = process.env): ResolveConfigResult {
  const key = env.ZOMBIE_POSTHOG_KEY || DEFAULT_POSTHOG_KEY;
  const host = env.ZOMBIE_POSTHOG_HOST || DEFAULT_POSTHOG_HOST;
  // Default off. Only DISABLE_TELEMETRY=0|false|off|no opts in.
  const disabled = boolFromEnv(env.DISABLE_TELEMETRY, true);
  const enabled = !disabled && key.length > 0;
  return { key, host, enabled };
}

function sanitizeProperties(
  properties: Record<string, unknown> = {},
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(properties)) {
    if (value === undefined || value === null) continue;
    out[key] = String(value);
  }
  return out;
}

export async function createCliAnalytics(
  env: NodeJS.ProcessEnv = process.env,
): Promise<AnalyticsClient | null> {
  const cfg = resolveConfig(env);
  if (!cfg.enabled || !cfg.key) return null;

  try {
    const loaded = await import("posthog-node");
    const PostHogCtor: PostHogConstructor = loaded.PostHog;
    const client = new PostHogCtor(cfg.key, {
      host: cfg.host,
      flushAt: 1,
      flushInterval: 0,
    });
    return client;
  } catch {
    return null;
  }
}

export function trackCliEvent(
  client: AnalyticsClient | null,
  distinctId: string | null | undefined,
  event: string,
  properties: Record<string, unknown> = {},
): void {
  if (!client) return;
  try {
    (client as { capture: (e: { distinctId: string; event: string; properties: Record<string, string> }) => void }).capture({
      distinctId: distinctId || "anonymous",
      event,
      properties: sanitizeProperties(properties),
    });
  } catch {
    // Telemetry must never block or break CLI UX.
  }
}

// Per-HTTP-request span emitted on every terminal attempt (success or
// fatal). retry_count = attempt - 1 (count of cli_http_retry events
// fired during this request).
export function trackHttpRequest(
  client: AnalyticsClient | null,
  distinctId: string,
  info: HttpRequestInfo,
): void {
  trackCliEvent(client, distinctId, "cli_http_request", {
    url: info.url,
    method: info.method,
    status: info.status,
    duration_ms: info.duration_ms,
    attempt: info.attempt,
    retry_count: info.retry_count,
  });
}

// Fired once per failed-and-will-retry attempt, before the backoff
// sleep. reason ∈ {network|timeout|5xx|429|server_marked_retryable}.
export function trackHttpRetry(
  client: AnalyticsClient | null,
  distinctId: string,
  info: HttpRetryInfo,
): void {
  trackCliEvent(client, distinctId, "cli_http_retry", {
    url: info.url,
    method: info.method,
    status: info.status,
    attempt: info.attempt,
    reason: info.reason,
  });
}

export function setCliAnalyticsContext(
  ctx: AnalyticsCtxLike | null | undefined,
  properties: Record<string, unknown> = {},
): void {
  if (!ctx) return;
  const current = ctx.analyticsContext || {};
  ctx.analyticsContext = {
    ...current,
    ...sanitizeProperties(properties),
  };
}

export function getCliAnalyticsContext(
  ctx: AnalyticsCtxLike | null | undefined,
): Record<string, unknown> {
  return ctx?.analyticsContext ? { ...ctx.analyticsContext } : {};
}

export function queueCliAnalyticsEvent(
  ctx: AnalyticsCtxLike | null | undefined,
  event: string,
  properties: Record<string, unknown> = {},
): void {
  if (!ctx) return;
  if (!Array.isArray(ctx.analyticsEvents)) ctx.analyticsEvents = [];
  ctx.analyticsEvents.push({
    event,
    properties: sanitizeProperties(properties),
  });
}

export function drainCliAnalyticsEvents(
  ctx: AnalyticsCtxLike | null | undefined,
): QueuedAnalyticsEvent[] {
  if (!ctx || !Array.isArray(ctx.analyticsEvents) || ctx.analyticsEvents.length === 0) return [];
  const events = ctx.analyticsEvents.slice();
  ctx.analyticsEvents = [];
  return events;
}

export async function shutdownCliAnalytics(
  client: AnalyticsClient | null | undefined,
): Promise<void> {
  if (!client) return;
  try {
    await (client as { shutdown: () => Promise<void> }).shutdown();
  } catch {
    // ignore shutdown failures
  }
}

// Mutable on purpose — test DI replaces members in place.
export const cliAnalyticsInternals = {
  DEFAULT_POSTHOG_KEY,
  drainCliAnalyticsEvents,
  getCliAnalyticsContext,
  queueCliAnalyticsEvent,
  resolveConfig,
  sanitizeProperties,
  setCliAnalyticsContext,
};

export const cliAnalytics = {
  createCliAnalytics,
  trackCliEvent,
  trackHttpRequest,
  trackHttpRetry,
  shutdownCliAnalytics,
};
