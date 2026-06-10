"use client";

import type { EventName, EventProps } from "./events";

const POSTHOG_DEFAULT_HOST = "https://us.i.posthog.com";
const EVENT_NAVIGATION_CLICKED = "navigation_clicked";
// Set on identify, cleared on reset — lets a signed-out mount after a hard
// navigation or session expiry detect "this browser still carries a prior
// session's identity" without reaching into posthog-js internals.
const IDENTIFIED_MARKER_KEY = "uz_analytics_identified";

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
  has_error?: boolean;
  has_pr_url?: boolean;
  error_code?: string;
  error_message?: string;
  reason?: string;
};

type PostHogLike = {
  init: (key: string, options?: Record<string, unknown>) => void;
  capture: (event: string, properties?: Record<string, AnalyticsValue>) => void;
  identify?: (distinctId: string, properties?: Record<string, AnalyticsValue>) => void;
  group?: (groupType: string, groupKey: string, properties?: Record<string, AnalyticsValue>) => void;
  reset?: () => void;
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
  "has_error",
  "has_pr_url",
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
  const key = env.NEXT_PUBLIC_POSTHOG_KEY ?? "";
  const host = env.NEXT_PUBLIC_POSTHOG_HOST ?? POSTHOG_DEFAULT_HOST;
  const enabled = boolFromEnv(env.NEXT_PUBLIC_POSTHOG_ENABLED, key.length > 0);
  return { key, host, enabled };
}

function sanitizeProps(properties: Partial<AnalyticsProps>): Record<string, AnalyticsValue> {
  const out: Record<string, AnalyticsValue> = {};
  // `Object.entries` widens optional props to a non-null value type, but at
  // runtime a caller can pass `{ key: undefined }`; keep the null check so an
  // explicit-undefined prop is dropped rather than stringified to "undefined".
  const entries = Object.entries(properties) as Array<[string, AnalyticsValue | undefined]>;
  for (const [key, value] of entries) {
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

// localStorage can be absent (server, stubbed test windows) or throw
// (locked-down privacy modes) — treat both as "no marker store". lib.dom
// types it non-nullish, so model the maybe-absent reality structurally.
function markerStore(): Storage | null {
  if (typeof window === "undefined") return null;
  try {
    return (window as { localStorage?: Storage }).localStorage ?? null;
  } catch {
    return null;
  }
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
  markerStore()?.setItem(IDENTIFIED_MARKER_KEY, "1");
}

// True when this browser still carries an identified analytics session —
// identified during this page load, or flagged by the persisted marker from a
// prior load (hard navigation / expired session).
export function hasStaleAnalyticsIdentity(): boolean {
  if (identifiedUserId !== null) return true;
  return markerStore()?.getItem(IDENTIFIED_MARKER_KEY) != null;
}

// Clears identity state unconditionally (marker + cached user id) so a later
// sign-in re-identifies; the posthog reset itself only runs when a client is
// live and exposes it.
export function resetAnalyticsIdentity(): void {
  markerStore()?.removeItem(IDENTIFIED_MARKER_KEY);
  identifiedUserId = null;
  if (!analyticsEnabled || !posthogClient?.reset) return;
  posthogClient.reset();
}

// Typed product-event capture. Catalog props deliberately bypass sanitizeProps:
// its closed ALLOWED_PROP_KEYS allowlist would silently drop event-specific
// keys (zombie_id, api_key_id, …); the EventProps types are the guard.
export function captureProductEvent<E extends EventName>(event: E, props: EventProps[E]): void {
  if (!analyticsEnabled || !posthogClient || typeof window === "undefined") return;
  const payload: Record<string, AnalyticsValue> = { path: window.location.pathname };
  const entries = Object.entries(props) as Array<[string, AnalyticsValue | undefined]>;
  for (const [key, value] of entries) {
    if (value == null) continue;
    payload[key] = value;
  }
  posthogClient.capture(event, payload);
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
