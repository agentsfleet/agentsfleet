"use client";

import { EVENT_PROP_KEYS, type EventName, type EventProps } from "./events";

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

// Group type for PostHog group analytics — every event/pageview captured after a
// group() call carries this group, so funnels can be sliced per workspace
// (mirrors Supabase's `$groups: { organization, project }`).
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
// True when env config says analytics is on, even while the posthog-js chunk
// is still loading (or failed) — distinguishes "off by config" from "not live
// yet", which the deferred reset/identify below depend on.
let analyticsConfigured = false;
let posthogClient: PostHogLike | null = null;
let initialized = false;
let identifiedUserId: string | null = null;
// Identity work that raced the posthog-js chunk load; initAnalytics flushes
// both the moment the client lands so neither is silently lost.
let pendingIdentify: { id: string; email?: string | null } | null = null;
let pendingReset = false;
// The workspace currently bound as the PostHog group — skip redundant group()
// calls and let resetAnalyticsIdentity() rebind on the next session.
let groupedWorkspaceId: string | null = null;
// Group/person context that raced the posthog-js chunk load; initAnalytics
// flushes it after identify so person props attach to the identified user.
let pendingContext: AnalyticsContext | null = null;
// Product events that raced the posthog-js chunk load. identify/context are
// single-slot (latest wins); product events are distinct, so this is a bounded
// FIFO. Mount-fired call sites (fleet_viewed) can capture before the dynamic
// import resolves on a cold/direct load — action-fired sites can't, they run
// after a server round-trip. Without this the cold-load captures would be
// silently dropped, systematically under-counting first views. Bounded so a
// stuck or failed load can't grow it without limit.
const MAX_PENDING_PRODUCT_EVENTS = 20;
let pendingProductEvents: Array<{ event: string; payload: Record<string, AnalyticsValue> }> = [];

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

// localStorage can be absent (server, stubbed test windows) or throw on read
// AND on write (locked-down privacy modes; full quota — posthog-js itself
// fills storage) — every marker operation is best-effort. lib.dom types it
// non-nullish, so model the maybe-absent reality structurally.
function withMarkerStore<T>(fn: (store: Storage) => T): T | null {
  if (typeof window === "undefined") return null;
  try {
    const store = (window as { localStorage?: Storage }).localStorage;
    return store ? fn(store) : null;
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
  analyticsConfigured = true;

  let posthog: PostHogLike;
  try {
    const loaded = await import("posthog-js");
    posthog = loaded.default as PostHogLike;
  } catch {
    // Chunk load failed (offline, blocked client): analytics is off for this
    // session. A deferred reset keeps its marker, so the identity sweep
    // retries on the next load instead of being silently lost.
    analyticsEnabled = false;
    return;
  }
  posthog.init(cfg.key, {
    api_host: cfg.host,
    person_profiles: "identified_only",
    autocapture: true,
    capture_pageview: true,
    capture_pageleave: true,
  });
  posthogClient = posthog;
  // Flush identity work that raced the chunk load — replay the reset first,
  // then any identify that arrived after it. The queue must be captured
  // before the replay: resetAnalyticsIdentity() clears pendingIdentify, but
  // a queued identify is always newer than the queued reset (a reset that
  // follows an identify already cancelled it at call time), so the sign-in
  // survives the replayed sign-out.
  const queuedIdentify = pendingIdentify;
  pendingIdentify = null;
  if (pendingReset) resetAnalyticsIdentity();
  if (queuedIdentify !== null) identifyAnalyticsUser(queuedIdentify);
  // Flush group/person context after identify so person props land on the
  // identified user, not the pre-identify anonymous id.
  const queuedContext = pendingContext;
  pendingContext = null;
  if (queuedContext !== null) setAnalyticsContext(queuedContext);
  // Flush product events captured before the chunk resolved (mount-fired call
  // sites like fleet_viewed) — after identify + context so they attach to the
  // identified person and workspace group. Load-bearing ordering: this capture
  // MUST stay below the `if (pendingReset) resetAnalyticsIdentity()` above —
  // that reset empties pendingProductEvents, so reading queuedEvents post-reset
  // drops a signed-out session's buffer instead of stitching it to the next
  // identity. Do not hoist this assignment above the reset.
  const queuedEvents = pendingProductEvents;
  pendingProductEvents = [];
  for (const queued of queuedEvents) posthogClient.capture(queued.event, queued.payload);
}

export function identifyAnalyticsUser(user: { id: string; email?: string | null }): void {
  if (!analyticsEnabled) return;
  if (posthogClient === null) {
    // posthog-js is still loading: queue the identify — the effect deps never
    // change again this session, so dropping it here would leave the whole
    // single-page session anonymous. initAnalytics flushes it.
    pendingIdentify = user;
    return;
  }
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

// Bind the active workspace as the PostHog group + set org-level person
// properties (workspace_count / plan), so every subsequent event and pageview
// is sliceable per workspace and the identified person carries org context.
// Mirrors Supabase's group() + identify($set) split. Best-effort: queues until
// posthog-js loads (initAnalytics flushes it after identify), no-ops when off.
export function setAnalyticsContext(ctx: AnalyticsContext): void {
  if (!analyticsEnabled) return;
  if (posthogClient === null) {
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

// True when this browser still carries an identified analytics session —
// identified during this page load, or flagged by the persisted marker from a
// prior load (hard navigation / expired session).
export function hasStaleAnalyticsIdentity(): boolean {
  if (identifiedUserId !== null) return true;
  return withMarkerStore((store) => store.getItem(IDENTIFIED_MARKER_KEY)) != null;
}

// Clears identity state (marker + cached user id) so a later sign-in
// re-identifies. The marker is consumed only when the posthog reset can
// actually run — burning it while the chunk is still loading would silently
// re-open the cross-session stitching this exists to close.
export function resetAnalyticsIdentity(): void {
  identifiedUserId = null;
  pendingIdentify = null;
  // posthog.reset() clears the bound groups too; drop our cache so the next
  // session's setAnalyticsContext rebinds the workspace group.
  groupedWorkspaceId = null;
  pendingContext = null;
  // Drop any product events buffered against the prior session — they must not
  // stitch onto the next (anonymous or other) identity.
  pendingProductEvents = [];
  if (analyticsConfigured && posthogClient === null) {
    // posthog-js is still loading (or its import failed — the next load
    // retries): keep the marker and let initAnalytics complete the reset.
    pendingReset = true;
    return;
  }
  pendingReset = false;
  withMarkerStore((store) => store.removeItem(IDENTIFIED_MARKER_KEY));
  if (!analyticsEnabled || !posthogClient?.reset) return;
  posthogClient.reset();
}

// Typed product-event capture. Catalog props deliberately bypass sanitizeProps
// (its closed ALLOWED_PROP_KEYS allowlist would silently drop event-specific
// keys like fleet_id); instead the payload is allowlisted against the
// catalog's own EVENT_PROP_KEYS mirror — compile-time excess-property checks
// only cover object literals, so a spread or widened argument must not be
// able to smuggle extra fields (a raw token is one property away at several
// call sites). Events that race the posthog-js chunk load are buffered (bounded
// FIFO) and flushed on init — mount-fired call sites (fleet_viewed) can capture
// before the chunk lands on a cold load; action-fired sites never do.
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
    if (posthogClient === null) {
      // Racing the chunk load (a mount-fired capture): buffer, don't drop. The
      // FIFO is bounded — events beyond the cap are dropped rather than grow
      // memory if the load stalls.
      if (pendingProductEvents.length < MAX_PENDING_PRODUCT_EVENTS) {
        pendingProductEvents.push({ event, payload: finalPayload });
      }
      return;
    }
    posthogClient.capture(event, finalPayload);
  } catch {
    // Analytics must never break the product flow it instruments — several
    // call sites sit beside one-time secret reveals.
  }
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
