import posthog from "posthog-js";

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

export function initAnalytics(): void {
  if (initialized) return;
  initialized = true;

  const cfg = readRuntimeConfig();
  analyticsEnabled = cfg.enabled;
  if (!cfg.enabled) return;

  posthog.init(cfg.key, {
    api_host: cfg.host,
    autocapture: false,
    capture_pageview: false,
    persistence: "localStorage",
  });
}

export function track(event: AnalyticsEventName, properties: AnalyticsProps): void {
  if (!initialized) initAnalytics();
  if (!analyticsEnabled) return;
  posthog.capture(event, sanitizeProps(properties));
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

export function resetAnalyticsForTests(): void {
  initialized = false;
  analyticsEnabled = false;
}
