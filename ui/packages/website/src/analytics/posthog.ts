import type { CaptureResult } from "posthog-js";

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

// Signup *completion* belongs server-side to agentsfleetd
// (`Telemetry::SignupBootstrapped`,
// rustd/crates/afd_observability/src/product/telemetry/mod.rs) — the funnel is
// redirect-based, so this origin can
// never observe it. No completion or lead-capture events exist here by
// design; see docs/architecture/product_analytics.md.
export const EVENT_SIGNUP_STARTED = "signup_started";
export const EVENT_NAVIGATION_CLICKED = "navigation_clicked";
export const EVENT_INTERACTION_CLICKED = "interaction_clicked";

type AnalyticsEventName = typeof EVENT_SIGNUP_STARTED | typeof EVENT_NAVIGATION_CLICKED | typeof EVENT_INTERACTION_CLICKED;

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
]);
const POSTHOG_INGEST_KEYS = new Set([
  "token", "distinct_id", "$device_id", "$session_id", "$window_id",
  "$is_identified", "$lib", "$lib_version", "$insert_id", "$time", "$process_person_profile",
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

function captureSafely(client: Posthog, event: AnalyticsEventName, props: AnalyticsProps): void {
  try {
    client.capture(event, props);
  } catch {
    // A blocked analytics client must not interrupt the visitor's action.
  }
}

function sanitizeOutgoingEvent(event: CaptureResult | null): CaptureResult | null {
  if (event === null) return null;
  // PostHog enriches events after capture(), so the final boundary must also filter.
  event.properties = Object.fromEntries(Object.entries(event.properties).filter(
    ([key]) => ALLOWED_PROPERTY_KEYS.has(key) || POSTHOG_INGEST_KEYS.has(key),
  ));
  event.properties.path = window.location.pathname;
  delete event.$set;
  delete event.$set_once;
  delete event.$unset;
  return event;
}

async function loadPosthog(cfg: RuntimeConfig): Promise<void> {
  // No posthogModule re-entry guard: ensureLoader gates the single call via
  // `loadPromise`, so this runs exactly once per init cycle (module still
  // unloaded). A guard here would be unreachable defensive code.
  //
  // The client is bound to `posthogModule` only AFTER init() succeeds (mirrors
  // the sibling app-package loader): if the import or init throws, the
  // module-level client stays null so a retry actually reloads, rather than
  // leaving track() to capture() against an uninitialised client.
  const mod = await import("posthog-js");
  const client = mod.default;
  client.init(cfg.key, {
    api_host: cfg.host,
    // Autocapture and pageviews retain counts and pathname attribution.
    // The final filter drops URL, referrer, campaign and element payloads.
    // The client's default
    // user-agent filter drops bot traffic, including headless browsers.
    //
    // `persistence: "localStorage"` (not "localStorage+cookie") so we
    // do NOT set a first-party tracking cookie on every page load. A
    // cookie is what would require a prior-consent banner under
    // ePrivacy/GDPR; localStorage-only analytics is the disclosure-then-
    // opt-out posture documented in Privacy.tsx. If/when a consent
    // banner ships, flip this back to "localStorage+cookie" to enable
    // cross-subdomain identity stitching with agentsfleet.net ↔ app
    // (the cookie is what crosses subdomains; localStorage does not).
    autocapture: true,
    capture_pageview: "history_change",
    capture_pageleave: true,
    persistence: "localStorage",
    // Flag requests bypass before_send and carry initial URL properties.
    advanced_disable_flags: true,
    before_send: sanitizeOutgoingEvent,
  });
  posthogModule = client;
  // Drain buffered events in arrival order; splice(0) empties the queue and
  // hands back every entry, so there is no partial-shift undefined to guard.
  for (const [event, props] of pendingEvents.splice(0)) {
    captureSafely(posthogModule, event, props);
  }
}

function ensureLoader(cfg: RuntimeConfig): void {
  if (loadPromise || !cfg.enabled) return;
  // Reset loadPromise on failure (blocked/offline chunk load, an init throw) so
  // a later ensureLoader/track() retries instead of being wedged forever on a
  // permanently-rejected promise. The `.catch` also handles the rejection, so a
  // failed load never surfaces an unhandled promise rejection. Mirrors the
  // sibling app-package loader's try/catch failure isolation (lib/analytics).
  loadPromise = loadPosthog(cfg).catch(() => {
    loadPromise = null;
  });
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
    captureSafely(posthogModule, event, sanitized);
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

export function trackNavigationClicked(properties: Omit<AnalyticsProps, "path">): void {
  track(EVENT_NAVIGATION_CLICKED, {
    ...properties,
    path: window.location.pathname,
  });
}

export function trackInteractionClicked(properties: Omit<AnalyticsProps, "path">): void {
  track(EVENT_INTERACTION_CLICKED, { ...properties, path: window.location.pathname });
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
