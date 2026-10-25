"use client";

import { EVENT_PROP_KEYS, type EventName, type EventProps } from "./events";
import { filterAuthenticationEvent, onAuthenticationRoute } from "./auth-privacy";
import { AnalyticsIdentityGate, type AnalyticsIdentityReader } from "./identity-gate";

const POSTHOG_DEFAULT_HOST = "https://us.i.posthog.com";
const EVENT_NAVIGATION_CLICKED = "navigation_clicked";
const ANALYTICS_IDLE_TIMEOUT_MS = 1_500;
// Lets a signed-out mount detect a prior session without PostHog internals.
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
  visibility?: string;
  source_kind?: string;
  outcome?: string;
};

type PostHogLike = {
  init: (key: string, options?: Record<string, unknown>) => void;
  capture: (event: string, properties?: Record<string, AnalyticsValue>) => void;
  identify?: (distinctId: string, properties?: Record<string, AnalyticsValue>) => void;
  group?: (groupType: string, groupKey: string, properties?: Record<string, AnalyticsValue>) => void;
  setPersonProperties?: (props: Record<string, AnalyticsValue>) => void;
  reset?: () => void;
};

// Subsequent PostHog events carry the active workspace group.
const GROUP_TYPE_WORKSPACE = "workspace";

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
  "visibility",
  "source_kind",
  "outcome",
]);

let analyticsEnabled = false;
// Distinguishes disabled config from a client that has not loaded yet.
let analyticsConfigured = false;
let posthogClient: PostHogLike | null = null;
let initialization: Promise<void> | null = null;
const identityGate = new AnalyticsIdentityGate(() => { pendingContext = null; pendingEvents = []; });
let identifiedUserId: string | null = null;
// Identity work that raced the PostHog client load.
let pendingIdentify: { id: string; email?: string | null } | null = null;
let pendingReset = false;
// The workspace currently bound as the PostHog group.
let groupedWorkspaceId: string | null = null;
// Group/person context that raced the PostHog client load.
let pendingContext: AnalyticsContext | null = null;
// Distinct events queue in arrival order and stay bounded if loading stalls.
const MAX_PENDING_EVENTS = 20;
let pendingEvents: Array<{ event: string; payload: Record<string, AnalyticsValue> }> = [];

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

// localStorage can be absent or throw in locked-down privacy modes, so every
// marker operation is best-effort despite the non-null lib.dom type.
function withMarkerStore<T>(fn: (store: Storage) => T): T | null {
  if (typeof window === "undefined") return null;
  try {
    const store = (window as { localStorage?: Storage }).localStorage;
    return store ? fn(store) : null;
  } catch {
    return null;
  }
}

function waitForAnalyticsIdle(): Promise<void> {
  return new Promise((resolveIdle) => {
    const requestIdle = (
      globalThis as {
        requestIdleCallback?: (callback: () => void, options: { timeout: number }) => number;
      }
    ).requestIdleCallback;
    if (typeof requestIdle === "function") {
      requestIdle(resolveIdle, { timeout: ANALYTICS_IDLE_TIMEOUT_MS });
      return;
    }
    setTimeout(resolveIdle, ANALYTICS_IDLE_TIMEOUT_MS);
  });
}

function captureOrQueue(event: string, payload: Record<string, AnalyticsValue>): void {
  if (filterAuthenticationEvent(payload) === null || !identityGate.queueIsCurrent()) return;
  if (posthogClient !== null && identityReady()) {
    posthogClient.capture(event, payload);
    return;
  }
  if (pendingEvents.length < MAX_PENDING_EVENTS) pendingEvents.push({ event, payload });
}

// The committed bootstrap owns identity; unresolved or cancelled effects hold telemetry.
function identityReady(): boolean {
  return identityGate.prepare((identity) => {
    const priorId = identifiedUserId ?? pendingIdentify?.id;
    if (pendingReset || (priorId && priorId !== identity?.id)) resetAnalyticsIdentity();
    pendingIdentify = null;
    if (identity) identifyAnalyticsUser(identity);
    else if (hasStaleAnalyticsIdentity()) resetAnalyticsIdentity();
  });
}

export async function initAnalytics(prepare?: AnalyticsIdentityReader): Promise<void> {
  identityGate.bind(prepare);
  if (onAuthenticationRoute() || typeof window === "undefined") return;
  if (posthogClient) { flushPending(); return; }
  if (initialization) return initialization;
  const cfg = resolveConfig(process.env);
  analyticsEnabled = cfg.enabled;
  if (!cfg.enabled || !cfg.key) return;
  analyticsConfigured = true;
  initialization = initializeClient(cfg).catch(() => {
    // A blocked SDK must not break navigation or disable an already loaded client.
    if (!posthogClient) analyticsEnabled = false;
  }).finally(() => { initialization = null; });
  return initialization;
}

async function initializeClient(cfg: ReturnType<typeof resolveConfig>): Promise<void> {
  await waitForAnalyticsIdle();
  if (onAuthenticationRoute() || !identityGate.ready()) return;
  const loaded = await import("posthog-js");
  if (onAuthenticationRoute() || !identityGate.ready()) return;
  const posthog = loaded.default as PostHogLike;
  posthog.init(cfg.key, {
    before_send: (event: unknown) => identityGate.allowsEvents() ? filterAuthenticationEvent(event) : null,
    // PostHog invokes loaded before enabling its queue or capturing the initial pageview.
    loaded: (client: PostHogLike) => { posthogClient = client; flushPending(); },
    api_host: cfg.host,
    person_profiles: "identified_only",
    autocapture: true,
    capture_pageview: true,
    capture_pageleave: true,
  });
  posthogClient = posthog;
  flushPending();
}

function flushPending(): void {
  if (!posthogClient || !identityGate.queueIsCurrent() || !identityReady()) return;
  // Newer queued identity survives an older sign-out reset.
  const queuedIdentify = pendingIdentify;
  pendingIdentify = null;
  if (pendingReset) resetAnalyticsIdentity();
  if (queuedIdentify !== null) identifyAnalyticsUser(queuedIdentify);
  // Person context must follow identity.
  const queuedContext = pendingContext;
  pendingContext = null;
  if (queuedContext !== null) setAnalyticsContext(queuedContext);
  // Reset discards events from the prior session; remaining events follow context.
  const queuedEvents = pendingEvents;
  pendingEvents = [];
  for (const queued of queuedEvents) posthogClient.capture(queued.event, queued.payload);
}

export function identifyAnalyticsUser(user: { id: string; email?: string | null }): void {
  if (onAuthenticationRoute()) return;
  if (!analyticsEnabled) return;
  if (posthogClient === null) {
    // Preserve identity work until the SDK loads.
    pendingIdentify = user;
    return;
  }
  pendingIdentify = null;
  if (!posthogClient.identify) return;
  if (identifiedUserId === user.id) return;

  const properties = sanitizeProps({
    user_id: user.id,
    email: user.email ?? undefined,
  });

  posthogClient.identify(user.id, properties);
  identifiedUserId = user.id;
  withMarkerStore((store) => store.setItem(IDENTIFIED_MARKER_KEY, "1"));
}

export type AnalyticsContext = {
  workspaceId?: string | null;
  workspaceCount?: number;
  plan?: string;
};

// Bind the workspace group and person context after the SDK and current identity
// are ready. Disabled analytics is a no-op.
export function setAnalyticsContext(ctx: AnalyticsContext): void {
  if (!analyticsEnabled || onAuthenticationRoute() || !identityGate.queueIsCurrent()) return;
  if (posthogClient === null || !identityReady()) {
    pendingContext = { ...pendingContext, ...ctx };
    return;
  }
  applyAnalyticsContext(ctx);
}

function applyAnalyticsContext(ctx: AnalyticsContext): void {
  if (ctx.workspaceId && groupedWorkspaceId !== ctx.workspaceId && posthogClient?.group) {
    posthogClient.group(GROUP_TYPE_WORKSPACE, ctx.workspaceId);
    groupedWorkspaceId = ctx.workspaceId;
  }
  const personProps = sanitizeProps({ workspace_count: ctx.workspaceCount, workspace_plan: ctx.plan });
  if (Object.keys(personProps).length > 0 && posthogClient?.setPersonProperties) {
    posthogClient.setPersonProperties(personProps);
  }
}

// Detect identity from this page load or its persisted marker.
export function hasStaleAnalyticsIdentity(): boolean {
  if (identifiedUserId !== null) return true;
  return withMarkerStore((store) => store.getItem(IDENTIFIED_MARKER_KEY)) != null;
}

// Consume the marker only after reset succeeds, so failed resets remain retryable.
export function resetAnalyticsIdentity(): void {
  identifiedUserId = null;
  pendingIdentify = null;
  // Reset also clears groups, so the next session must bind its workspace.
  groupedWorkspaceId = null;
  pendingContext = null;
  // Drop any product events buffered against the prior session — they must not
  // stitch onto the next (anonymous or other) identity.
  pendingEvents = [];
  if (analyticsConfigured && posthogClient === null) {
    // posthog-js is still loading (or its import failed — the next load
    // retries): keep the marker and let initAnalytics complete the reset.
    pendingReset = true;
    return;
  }
  if (analyticsEnabled && posthogClient?.reset) {
    pendingReset = true;
    posthogClient.reset();
  }
  pendingReset = false;
  withMarkerStore((store) => store.removeItem(IDENTIFIED_MARKER_KEY));
}

// Runtime catalog allowlists prevent extra fields from escaping widened objects;
// sanitizeProps would discard legitimate event-specific keys. Mount-time events
// stay in a bounded queue until the SDK and current identity are ready.
export function captureProductEvent<E extends EventName>(
  event: E,
  props: EventProps[E],
  // Optional person-properties set atomically with the event via PostHog's
  // `$set` reserved key — lets a capture also update the identified person
  // (e.g. last_integration_requested) without a separate identify call.
  options?: { setPersonProperties?: Record<string, AnalyticsValue> },
): void {
  if (!analyticsEnabled || typeof window === "undefined") return;
  try {
    const payload: Record<string, AnalyticsValue | Record<string, AnalyticsValue>> = {
      path: window.location.pathname,
    };
    const bag = props as Record<string, unknown>;
    for (const key of EVENT_PROP_KEYS[event]) {
      const value = bag[key];
      if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
        payload[key] = value;
      }
    }
    if (options?.setPersonProperties) payload.$set = options.setPersonProperties;
    const finalPayload = payload as Record<string, AnalyticsValue>;
    captureOrQueue(event, finalPayload);
  } catch {
    // Analytics must never break the product flow it instruments — several
    // call sites sit beside one-time secret reveals.
  }
}

export function trackAppEvent(event: string, properties: Partial<AnalyticsProps> = {}): void {
  if (!analyticsEnabled || typeof window === "undefined") return;
  const payload = sanitizeProps({
    ...properties,
    path: properties.path ?? window.location.pathname,
  });
  captureOrQueue(event, payload);
}

export function trackNavigationClicked(properties: Omit<AnalyticsProps, "path">): void {
  trackAppEvent(EVENT_NAVIGATION_CLICKED, properties);
}

export const appAnalyticsInternals = {
  sanitizeProps,
  resolveConfig,
};
