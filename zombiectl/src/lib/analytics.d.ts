// Ambient declaration for the still-JS analytics module. This is a
// boundary file — analytics.js will migrate to TS in a later §14 wave
// (alongside the other lib/* files that command modules consume). For
// now D37's typecheck needs honest signatures at the import seam.
//
// Shapes match the JS implementation verbatim (src/lib/analytics.js).
// Adding fields here without adding them to the .js file is a lie;
// removing fields here that the .js file produces is also a lie.

export type AnalyticsClient = unknown;

export interface CliAnalyticsContext {
  readonly client: AnalyticsClient | null;
  readonly distinctId: string;
  readonly queuedEvents: ReadonlyArray<unknown>;
}

export interface HttpRequestInfo {
  url: string;
  method: string;
  status: number | undefined;
  duration_ms: number;
  attempt: number;
  retry_count: number;
}

export interface HttpRetryInfo {
  url: string;
  method: string;
  status: number | undefined;
  attempt: number;
  reason: string;
}

export function createCliAnalytics(env?: NodeJS.ProcessEnv): Promise<AnalyticsClient | null>;
export function shutdownCliAnalytics(client: AnalyticsClient | null | undefined): Promise<void>;

// `cliAnalytics` is the namespace object exported by analytics.js
// (not a function). Mirrors the literal object at the bottom of
// src/lib/analytics.js so the call sites `cliAnalytics.trackCliEvent(...)`
// + `cliAnalytics.shutdownCliAnalytics(...)` typecheck honestly.
export const cliAnalytics: {
  readonly createCliAnalytics: typeof createCliAnalytics;
  readonly trackCliEvent: typeof trackCliEvent;
  readonly trackHttpRequest: typeof trackHttpRequest;
  readonly trackHttpRetry: typeof trackHttpRetry;
  readonly shutdownCliAnalytics: typeof shutdownCliAnalytics;
};
export function getCliAnalyticsContext(
  ctx: { analyticsContext?: Record<string, unknown> | null } | null | undefined,
): Record<string, unknown>;
export function setCliAnalyticsContext(
  ctx: { analyticsClient?: AnalyticsClient | null; distinctId?: string },
  patch: Record<string, unknown>,
): void;
export function queueCliAnalyticsEvent(
  ctx: { analyticsClient?: AnalyticsClient | null; distinctId?: string },
  event: string,
  properties?: Record<string, unknown>,
): void;
export function drainCliAnalyticsEvents(client: AnalyticsClient | null): Promise<void>;
export function trackHttpRequest(
  client: AnalyticsClient | null,
  distinctId: string,
  info: HttpRequestInfo,
): void;
export function trackHttpRetry(
  client: AnalyticsClient | null,
  distinctId: string,
  info: HttpRetryInfo,
): void;
export function trackCliEvent(
  client: AnalyticsClient | null,
  distinctId: string,
  event: string,
  properties?: Record<string, unknown>,
): void;
