"use client";

const POSTHOG_DEFAULT_HOST = "https://us.i.posthog.com";
const EVENT_NAVIGATION_CLICKED = "navigation_clicked";

type AnalyticsProps = {
  source: string;
  surface: string;
  target?: string;
  path?: string;
};

type PostHogLike = {
  init: (key: string, options?: Record<string, unknown>) => void;
  capture: (event: string, properties?: Record<string, string>) => void;
};

const ALLOWED_PROP_KEYS = new Set<keyof AnalyticsProps>(["source", "surface", "target", "path"]);

let analyticsEnabled = false;
let posthogClient: PostHogLike | null = null;
let initialized = false;

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

function sanitizeProps(properties: AnalyticsProps): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(properties)) {
    const typedKey = key as keyof AnalyticsProps;
    if (!ALLOWED_PROP_KEYS.has(typedKey)) continue;
    if (value == null) continue;
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

export function trackNavigationClicked(properties: Omit<AnalyticsProps, "path">): void {
  if (!analyticsEnabled || !posthogClient || typeof window === "undefined") return;
  posthogClient.capture(
    EVENT_NAVIGATION_CLICKED,
    sanitizeProps({
      ...properties,
      path: window.location.pathname,
    }),
  );
}

export const appAnalyticsInternals = {
  sanitizeProps,
  resolveConfig,
};
