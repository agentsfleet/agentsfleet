/*
 * posthog-js is lazy-loaded to keep it off the landing first-load bundle.
 * initAnalytics() schedules the import on idle (so it doesn't compete
 * with LCP work), and track() queues events that arrive before the
 * module resolves. Once loaded, buffered events flush in order.
 *
 * Call-sites stay synchronous (track returns void). Tests can await
 * flushAnalyticsForTests() to drain the queue before asserting.
 */

// Module-local type alias so the lazy loader's return type is named
// without bringing the runtime import along.
type Posthog = typeof import("posthog-js").default;

export const EVENT_SIGNUP_STARTED = "signup_started";
export const EVENT_SIGNUP_COMPLETED = "signup_completed";
export const EVENT_NAVIGATION_CLICKED = "navigation_clicked";
export const EVENT_LEAD_CAPTURE_CLICKED = "lead_capture_clicked";
export const EVENT_LEAD_CAPTURE_OPENED = "lead_capture_opened";
export const EVENT_LEAD_CAPTURE_SUBMITTED = "lead_capture_submitted";
export const EVENT_LEAD_CAPTURE_FAILED = "lead_capture_failed";

type AnalyticsEventName =
  | typeof EVENT_SIGNUP_STARTED
  | typeof EVENT_SIGNUP_COMPLETED
  | typeof EVENT_NAVIGATION_CLICKED
  | typeof EVENT_LEAD_CAPTURE_CLICKED
  | typeof EVENT_LEAD_CAPTURE_OPENED
  | typeof EVENT_LEAD_CAPTURE_SUBMITTED
  | typeof EVENT_LEAD_CAPTURE_FAILED;

type AnalyticsPrimitive = string | number | boolean;
type AnalyticsProps = Record<string, AnalyticsPrimitive>;

type RuntimeConfig = {
  enabled: boolean;
  key: string;
  host: string;
};

const POSTHOG_DEFAULT_HOST = "https://us.i.posthog.com";
const ALLOWED_PROPERTY_KEYS = new Set([
  "source",
  "surface",
  "mode",
  "target",
  "path",
  "component",
  "page",
  "cta_id",
  "plan_interest",
  "status",
  "utm_source",
  "utm_medium",
  "utm_campaign",
]);

let initialized = false;
let analyticsEnabled = false;
let posthogModule: Posthog | null = null;
let loadPromise: Promise<void> | null = null;
const pendingEvents: Array<[AnalyticsEventName, AnalyticsProps]> = [];

function readRuntimeConfig(): RuntimeConfig {
  const globalCfg = (globalThis as { __UZ_ANALYTICS_CONFIG__?: Partial<RuntimeConfig> }).__UZ_ANALYTICS_CONFIG__;
  const key = (globalCfg?.key ?? import.meta.env.VITE_POSTHOG_KEY ?? "").trim();
  const host = (globalCfg?.host ?? import.meta.env.VITE_POSTHOG_HOST ?? POSTHOG_DEFAULT_HOST).trim();
  const rawEnabled = globalCfg?.enabled ?? import.meta.env.VITE_POSTHOG_ENABLED !== "false";
  return {
    enabled: rawEnabled && key.length > 0,
    key,
    host,
  };
}

function sanitizeProps(properties: AnalyticsProps): AnalyticsProps {
  const sanitized: AnalyticsProps = {};
  for (const [key, value] of Object.entries(properties)) {
    if (!ALLOWED_PROPERTY_KEYS.has(key)) continue;
    if (typeof value === "string" && value.length > 256) {
      sanitized[key] = value.slice(0, 256);
      continue;
    }
    sanitized[key] = value;
  }
  return sanitized;
}

async function loadPosthog(cfg: RuntimeConfig): Promise<void> {
  if (posthogModule) return;
  const mod = await import("posthog-js");
  posthogModule = mod.default;
  posthogModule.init(cfg.key, {
    api_host: cfg.host,
    // Autocapture covers clicks/changes/submits with element metadata
    // (text, href, css path, surrounding form). Pageviews fire on every
    // SPA route change so the funnel is anchored. Bots are auto-tagged
    // by posthog-js via $browser_type — filter them out in the PostHog
    // UI rather than dropping them at the SDK so we keep the count.
    autocapture: true,
    capture_pageview: "history_change",
    capture_pageleave: true,
    persistence: "localStorage+cookie",
  });
  while (pendingEvents.length > 0) {
    const next = pendingEvents.shift();
    if (next) posthogModule.capture(next[0], next[1]);
  }
}

function ensureLoader(cfg: RuntimeConfig): void {
  if (loadPromise || !cfg.enabled) return;
  loadPromise = loadPosthog(cfg);
}

function scheduleIdle(fn: () => void): void {
  const ric = (
    globalThis as { requestIdleCallback?: (cb: () => void, opts?: { timeout: number }) => number }
  ).requestIdleCallback;
  if (typeof ric === "function") {
    ric(fn, { timeout: 1500 });
    return;
  }
  setTimeout(fn, 0);
}

export function initAnalytics(): void {
  if (initialized) return;
  initialized = true;

  const cfg = readRuntimeConfig();
  analyticsEnabled = cfg.enabled;
  if (!cfg.enabled) return;

  // Prefetch posthog-js on idle so the first track() finds it hot. Never
  // blocks the landing LCP critical path.
  scheduleIdle(() => ensureLoader(cfg));
}

export function track(event: AnalyticsEventName, properties: AnalyticsProps): void {
  if (!initialized) initAnalytics();
  if (!analyticsEnabled) return;

  const sanitized = sanitizeProps(properties);
  if (posthogModule) {
    posthogModule.capture(event, sanitized);
    return;
  }
  pendingEvents.push([event, sanitized]);
  ensureLoader(readRuntimeConfig());
}

export function trackSignupStarted(properties: Omit<AnalyticsProps, "path">): void {
  track(EVENT_SIGNUP_STARTED, {
    ...properties,
    path: window.location.pathname,
  });
}

export function trackSignupCompleted(properties: Omit<AnalyticsProps, "path">): void {
  track(EVENT_SIGNUP_COMPLETED, {
    ...properties,
    path: window.location.pathname,
  });
}

export function trackNavigationClicked(properties: Omit<AnalyticsProps, "path">): void {
  track(EVENT_NAVIGATION_CLICKED, {
    ...properties,
    path: window.location.pathname,
  });
}

export function trackLeadCaptureClicked(properties: Omit<AnalyticsProps, "path">): void {
  track(EVENT_LEAD_CAPTURE_CLICKED, {
    ...properties,
    path: window.location.pathname,
  });
}

export function trackLeadCaptureOpened(properties: Omit<AnalyticsProps, "path">): void {
  track(EVENT_LEAD_CAPTURE_OPENED, {
    ...properties,
    path: window.location.pathname,
  });
}

export function trackLeadCaptureSubmitted(properties: Omit<AnalyticsProps, "path">): void {
  track(EVENT_LEAD_CAPTURE_SUBMITTED, {
    ...properties,
    path: window.location.pathname,
  });
}

export function trackLeadCaptureFailed(properties: Omit<AnalyticsProps, "path">): void {
  track(EVENT_LEAD_CAPTURE_FAILED, {
    ...properties,
    path: window.location.pathname,
  });
}

/** Await-able test hook: resolves once the lazy posthog-js import has
 * settled and any buffered events have flushed through `capture()`. */
export async function flushAnalyticsForTests(): Promise<void> {
  if (loadPromise) await loadPromise;
}

export function resetAnalyticsForTests(): void {
  initialized = false;
  analyticsEnabled = false;
  posthogModule = null;
  loadPromise = null;
  pendingEvents.length = 0;
}
