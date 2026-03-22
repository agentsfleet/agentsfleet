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
  const enabled = boolFromEnv(env.ZOMBIE_POSTHOG_ENABLED, key.length > 0);
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
  resolveConfig,
  sanitizeProperties,
};

export const cliAnalytics = {
  createCliAnalytics,
  trackCliEvent,
  shutdownCliAnalytics,
};
