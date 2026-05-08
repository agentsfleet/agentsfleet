const DEFAULT_POSTHOG_HOST = "https://us.i.posthog.com";
const DEFAULT_POSTHOG_KEY = [
  "phc_XmuRIXBST",
  "Rfxka7IgfkU0V",
  "PMD3LDRR3IqIL",
  "XNg3bXzv",
].join("");

function boolFromEnv(value, fallback) {
  if (value == null || value === "") return fallback;
  const normalized = String(value).trim().toLowerCase();
  return !(normalized === "0" || normalized === "false" || normalized === "off" || normalized === "no");
}

function resolveConfig(env = process.env) {
  const key = env.ZOMBIE_POSTHOG_KEY || DEFAULT_POSTHOG_KEY;
  const host = env.ZOMBIE_POSTHOG_HOST || DEFAULT_POSTHOG_HOST;
  // Default off. Only DISABLE_TELEMETRY=0|false|off|no opts in.
  const disabled = boolFromEnv(env.DISABLE_TELEMETRY, true);
  const enabled = !disabled && key.length > 0;
  return { key, host, enabled };
}

function sanitizeProperties(properties = {}) {
  const out = {};
  for (const [key, value] of Object.entries(properties)) {
    if (value === undefined || value === null) continue;
    out[key] = String(value);
  }
  return out;
}

export async function createCliAnalytics(env = process.env) {
  const cfg = resolveConfig(env);
  if (!cfg.enabled || !cfg.key) return null;

  try {
    const loaded = await import("posthog-node");
    const PostHogCtor = loaded.PostHog || loaded.default;
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

export function trackCliEvent(client, distinctId, event, properties = {}) {
  if (!client) return;
  try {
    client.capture({
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
// fired during this request), per M63_004 §2.
export function trackHttpRequest(client, distinctId, info = {}) {
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
export function trackHttpRetry(client, distinctId, info = {}) {
  trackCliEvent(client, distinctId, "cli_http_retry", {
    url: info.url,
    method: info.method,
    status: info.status,
    attempt: info.attempt,
    reason: info.reason,
  });
}

export function setCliAnalyticsContext(ctx, properties = {}) {
  if (!ctx) return;
  const current = ctx.analyticsContext || {};
  ctx.analyticsContext = {
    ...current,
    ...sanitizeProperties(properties),
  };
}

export function getCliAnalyticsContext(ctx) {
  return ctx?.analyticsContext ? { ...ctx.analyticsContext } : {};
}

export function queueCliAnalyticsEvent(ctx, event, properties = {}) {
  if (!ctx) return;
  if (!Array.isArray(ctx.analyticsEvents)) ctx.analyticsEvents = [];
  ctx.analyticsEvents.push({
    event,
    properties: sanitizeProperties(properties),
  });
}

export function drainCliAnalyticsEvents(ctx) {
  if (!ctx || !Array.isArray(ctx.analyticsEvents) || ctx.analyticsEvents.length === 0) return [];
  const events = ctx.analyticsEvents.slice();
  ctx.analyticsEvents = [];
  return events;
}

export async function shutdownCliAnalytics(client) {
  if (!client) return;
  try {
    await client.shutdown();
  } catch {
    // ignore shutdown failures
  }
}

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
