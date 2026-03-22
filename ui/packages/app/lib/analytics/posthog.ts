"use client";

const POSTHOG_DEFAULT_HOST = "https://us.i.posthog.com";
const EVENT_NAVIGATION_CLICKED = "navigation_clicked";

type AnalyticsValue = string | number | boolean;

type AnalyticsProps = {
  source: string;
  surface: string;
  target?: string;
  path?: string;
  user_id?: string;
  email?: string;
  workspace_id?: string;
  workspace_count?: number;
  workspace_plan?: string;
  paused?: boolean;
  run_id?: string;
  run_status?: string;
  run_attempts?: number;
  has_error?: boolean;
  has_pr_url?: boolean;
  active_run_id?: string;
  active_run_status?: string;
  error_code?: string;
  error_message?: string;
  reason?: string;
};

type PostHogLike = {
  init: (key: string, options?: Record<string, unknown>) => void;
  capture: (event: string, properties?: Record<string, AnalyticsValue>) => void;
  identify?: (distinctId: string, properties?: Record<string, AnalyticsValue>) => void;
  group?: (groupType: string, groupKey: string, properties?: Record<string, AnalyticsValue>) => void;
};

const ALLOWED_PROP_KEYS = new Set<keyof AnalyticsProps>([
  "source",
  "surface",
  "target",
  "path",
  "user_id",
  "email",
  "workspace_id",
  "workspace_count",
  "workspace_plan",
  "paused",
  "run_id",
  "run_status",
  "run_attempts",
  "has_error",
  "has_pr_url",
  "active_run_id",
  "active_run_status",
  "error_code",
  "error_message",
  "reason",
]);

let analyticsEnabled = false;
let posthogClient: PostHogLike | null = null;
let initialized = false;
let identifiedUserId: string | null = null;

function boolFromEnv(value: string | undefined, fallback: boolean): boolean {
  if (value == null || value === "") return fallback;
  const normalized = value.trim().toLowerCase();
  return !(normalized === "0" || normalized === "false" || normalized === "off" || normalized === "no");
}

function resolveConfig(env: Record<string, string | undefined>) {
  const key = env.NEXT_PUBLIC_POSTHOG_KEY || "";
  const host = env.NEXT_PUBLIC_POSTHOG_HOST || POSTHOG_DEFAULT_HOST;
  const enabled = boolFromEnv(env.NEXT_PUBLIC_POSTHOG_ENABLED, key.length > 0);
  return { key, host, enabled };
}

function sanitizeProps(properties: Partial<AnalyticsProps>): Record<string, AnalyticsValue> {
  const out: Record<string, AnalyticsValue> = {};
  for (const [key, value] of Object.entries(properties)) {
    const typedKey = key as keyof AnalyticsProps;
    if (!ALLOWED_PROP_KEYS.has(typedKey)) continue;
    if (value == null) continue;
    if (typeof value === "boolean" || typeof value === "number") {
      out[key] = value;
      continue;
    }
    const safeValue = String(value).trim();
    if (safeValue.length === 0) continue;
    out[key] = safeValue;
  }
  return out;
}

export async function initAnalytics(): Promise<void> {
  if (initialized) return;
  initialized = true;

  if (typeof window === "undefined") return;

  const cfg = resolveConfig(process.env);
  analyticsEnabled = cfg.enabled;
  if (!cfg.enabled || !cfg.key) return;

  const loaded = await import("posthog-js");
  const posthog = loaded.default as PostHogLike;
  posthog.init(cfg.key, {
    api_host: cfg.host,
    person_profiles: "identified_only",
    autocapture: true,
    capture_pageview: true,
    capture_pageleave: true,
  });
  posthogClient = posthog;
}

export function identifyAnalyticsUser(user: { id: string; email?: string | null }): void {
  if (!analyticsEnabled || !posthogClient?.identify) return;
  if (identifiedUserId === user.id) return;

  const properties = sanitizeProps({
    user_id: user.id,
    email: user.email ?? undefined,
  });

  posthogClient.identify(user.id, properties);
  identifiedUserId = user.id;
}

export function trackAppEvent(event: string, properties: Partial<AnalyticsProps> = {}): void {
  if (!analyticsEnabled || !posthogClient || typeof window === "undefined") return;
  const payload = sanitizeProps({
    ...properties,
    path: properties.path ?? window.location.pathname,
  });
  posthogClient.capture(event, payload);
}

export function trackNavigationClicked(properties: Omit<AnalyticsProps, "path">): void {
  trackAppEvent(EVENT_NAVIGATION_CLICKED, properties);
}

export const appAnalyticsInternals = {
  sanitizeProps,
  resolveConfig,
};
