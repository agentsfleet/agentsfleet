// run-command — generic per-command boundary for zombiectl handlers.
// Owns ApiError formatting (matching printApiError's
// `error: <code> <message>` + `request_id:` shape), fetch-failed →
// API_UNREACHABLE, unknown → UNEXPECTED, and the
// cli_command_started / cli_command_finished / cli_error analytics
// triplet.
//
// Rendering is self-contained: the wrapper writes directly to
// ctx.stderr via deps.printJson / deps.writeLine / deps.ui, so callers
// don't need to pre-wire io.js's positional writeError.

import { ApiError, type RetryConfig } from "./http.ts";
import {
  trackCliEvent as defaultTrackCliEvent,
  getCliAnalyticsContext,
  type AnalyticsClient,
} from "./analytics.ts";
import { appendTrace } from "./state.ts";
import type { PresetMap } from "./error-map-presets.ts";

const API_UNREACHABLE_CODE = "API_UNREACHABLE";
const UNEXPECTED_CODE = "UNEXPECTED";

// Handler context shape — the real Deps shape lands in D39 when
// commands themselves migrate. runCommand reads a handful of known
// fields and mutates `retryConfig`; the index signature keeps room for
// the handler's own state without re-typing at every D38 boundary.
export interface HandlerCtx {
  stderr?: NodeJS.WritableStream | null;
  jsonMode?: boolean;
  apiUrl?: string;
  analyticsClient?: AnalyticsClient | null;
  distinctId?: string;
  analyticsContext?: Record<string, unknown> | null;
  retryConfig?: RetryConfig | null;
  // CLI telemetry session/device identity. Field names match the local
  // camelCase convention; the wire-format snake_case rename happens at
  // emit/append sites (buildProps + appendTrace).
  cliSessionId?: string | null;
  cliDeviceId?: string | null;
  [key: string]: unknown;
}

export type Handler = (
  ctx: HandlerCtx,
) => number | void | Promise<number | void>;

type TrackEventFn = typeof defaultTrackCliEvent;

export interface RunCommandDeps {
  analyticsClient?: AnalyticsClient | null | undefined;
  distinctId?: string | undefined;
  trackCliEvent?: TrackEventFn | undefined;
  printJson?: ((stream: NodeJS.WritableStream, value: unknown) => void) | undefined;
  writeLine?: ((stream: NodeJS.WritableStream, line: string) => void) | undefined;
  ui?: { err?: (s: string) => string } | null | undefined;
}

export interface RunCommandOptions {
  name: string;
  handler: Handler;
  retry?: boolean | RetryConfig | null;
  instrument?: boolean;
  errorMap?: PresetMap;
  ctx?: HandlerCtx | null;
  deps?: RunCommandDeps;
}

interface RenderOpts {
  handlerCtx: HandlerCtx;
  printJson: RunCommandDeps["printJson"];
  writeLine: RunCommandDeps["writeLine"];
  ui: RunCommandDeps["ui"];
  instrument: boolean;
  trackEvent: TrackEventFn;
  analyticsClient: AnalyticsClient | null;
  distinctId: string;
  buildProps: () => Record<string, unknown>;
}

function isFetchFailed(err: unknown): err is TypeError {
  return (
    err instanceof TypeError &&
    typeof err.message === "string" &&
    err.message.toLowerCase().includes("fetch failed")
  );
}

function resolveRetryConfig(
  retry: boolean | RetryConfig | null | undefined,
): RetryConfig | null {
  // retry: undefined → caller (apiRequestWithRetry) picks the default.
  // retry: true → same as undefined.
  // retry: false → collapse to 1 attempt for the handler's scope.
  // retry: { maxAttempts: N } → propagate verbatim.
  if (retry === undefined || retry === null || retry === true) return null;
  if (retry === false) return { maxAttempts: 1 };
  return retry;
}

function emitCliError(opts: RenderOpts, errorCode: string): void {
  if (!opts.instrument) return;
  opts.trackEvent(opts.analyticsClient, opts.distinctId, "cli_error", {
    ...opts.buildProps(),
    error_code: errorCode,
    exit_code: "1",
  });
}

function renderApi(
  opts: RenderOpts,
  code: string,
  message: string,
  err: ApiError,
): void {
  const { handlerCtx, printJson, writeLine } = opts;
  const stderr = handlerCtx.stderr;
  if (!stderr || typeof writeLine !== "function") return;
  if (handlerCtx.jsonMode) {
    if (typeof printJson !== "function") return;
    printJson(stderr, {
      error: {
        code,
        message,
        status: err.status ?? null,
        request_id: err.requestId ?? null,
      },
    });
    return;
  }
  writeLine(stderr, `error: ${code} ${message}`);
  if (err.requestId) writeLine(stderr, `request_id: ${err.requestId}`);
}

function renderPlain(opts: RenderOpts, code: string, message: string): void {
  const { handlerCtx, printJson, writeLine, ui } = opts;
  const stderr = handlerCtx.stderr;
  if (!stderr || typeof writeLine !== "function") return;
  if (handlerCtx.jsonMode) {
    if (typeof printJson !== "function") return;
    printJson(stderr, { error: { code, message } });
    return;
  }
  // Mirror cli.ts's outer safety net + renderApi: human mode keeps
  // the `error: ` prefix so operators see the visual signal in
  // --no-color and CI environments. Coloring (when ui is present)
  // wraps the full prefixed line.
  const colorize =
    ui && typeof ui.err === "function" ? ui.err : (s: string) => s;
  writeLine(stderr, colorize(`error: ${message}`));
}

export async function runCommand(opts: RunCommandOptions): Promise<number> {
  const {
    name,
    handler,
    retry,
    instrument = true,
    errorMap = {},
    ctx,
    deps = {},
  } = opts;
  if (typeof handler !== "function") {
    throw new TypeError("runCommand: handler must be a function");
  }
  if (typeof name !== "string" || name.length === 0) {
    throw new TypeError("runCommand: name must be a non-empty string");
  }

  // Mutate the caller's ctx in place rather than copying. Handlers
  // already share this object via closure (see cli.ts's registry
  // lambdas) — copying would mean retryConfig propagation and
  // setCliAnalyticsContext mutations during the handler don't round-
  // trip into the wrapper's post-handler events.
  const handlerCtx: HandlerCtx = ctx ?? {};
  handlerCtx.retryConfig = resolveRetryConfig(retry);

  const analyticsClient: AnalyticsClient | null =
    deps.analyticsClient ?? handlerCtx.analyticsClient ?? null;
  const distinctId =
    deps.distinctId ?? handlerCtx.distinctId ?? "anonymous";
  const trackEvent = deps.trackCliEvent ?? defaultTrackCliEvent;
  const printJson = deps.printJson;
  const writeLine = deps.writeLine;
  const ui = deps.ui ?? null;

  // Re-evaluated for every event so handlers that call
  // setCliAnalyticsContext during execution have their additions
  // visible on cli_command_finished and cli_error (matching the
  // pre-migration cli.ts behavior of spreading analyticsContext
  // post-handler). session_id/device_id flow through every emit so
  // PostHog dashboards can correlate events to a CLI session.
  const buildProps = (): Record<string, unknown> => {
    const base: Record<string, unknown> = {
      command: name,
      json_mode: String(handlerCtx.jsonMode ?? false),
    };
    // cli_session_id / cli_device_id are intentionally NAMESPACED so
    // they never collide with handler-set analyticsContext keys (e.g.,
    // login sets `session_id` to mean the auth-session row id — same
    // string, different concept). Two separate dimensions in PostHog.
    if (handlerCtx.cliSessionId) base["cli_session_id"] = handlerCtx.cliSessionId;
    if (handlerCtx.cliDeviceId) base["cli_device_id"] = handlerCtx.cliDeviceId;
    return { ...base, ...getCliAnalyticsContext(handlerCtx) };
  };

  const renderOpts: RenderOpts = {
    handlerCtx,
    printJson,
    writeLine,
    ui,
    instrument,
    trackEvent,
    analyticsClient,
    distinctId,
    buildProps,
  };

  if (instrument) {
    trackEvent(analyticsClient, distinctId, "cli_command_started", buildProps());
  }

  // performance.now() is monotonic — Date.now() can jump backward on
  // NTP slew / VM clock corrections, which would produce negative or
  // inflated duration_ms in the trace record.
  const startMs = performance.now();
  let finalExit = 1;
  try {
    const exitCode = await handler(handlerCtx);
    finalExit = typeof exitCode === "number" ? exitCode : 0;
    if (instrument) {
      trackEvent(analyticsClient, distinctId, "cli_command_finished", {
        ...buildProps(),
        exit_code: String(finalExit),
      });
    }
    return finalExit;
  } catch (err) {
    if (err instanceof ApiError) {
      const remap = errorMap[err.code ?? ""];
      // Server's UZ-* code stays in stderr/JSON output so support and
      // grep workflows still match. The friendly remap.code is the
      // analytics dimension (cli_error.error_code) — that lets us
      // bucket events without leaking churn from server-side code
      // renames into the operator-facing surface.
      const displayCode = err.code ?? "API_ERROR";
      const analyticsCode = remap?.code ?? err.code ?? "API_ERROR";
      const finalMessage = remap?.message ?? err.message;
      emitCliError(renderOpts, analyticsCode);
      renderApi(renderOpts, displayCode, finalMessage, err);
      finalExit = 1;
      return 1;
    }

    if (isFetchFailed(err)) {
      const message = `cannot reach usezombie API at ${handlerCtx.apiUrl}`;
      emitCliError(renderOpts, API_UNREACHABLE_CODE);
      renderPlain(renderOpts, API_UNREACHABLE_CODE, message);
      finalExit = 1;
      return 1;
    }

    emitCliError(renderOpts, UNEXPECTED_CODE);
    const fallback =
      err instanceof Error && typeof err.message === "string"
        ? err.message
        : String(err);
    renderPlain(renderOpts, UNEXPECTED_CODE, fallback);
    finalExit = 1;
    return 1;
  } finally {
    if (instrument) {
      // performance.now() returns fractional ms; round to integer for
      // a stable JSON record shape.
      const durationMs = Math.round(performance.now() - startMs);
      await appendTrace({
        ts: new Date().toISOString(),
        command: name,
        cli_session_id: handlerCtx.cliSessionId ?? null,
        cli_device_id: handlerCtx.cliDeviceId ?? null,
        exit_code: finalExit,
        duration_ms: durationMs,
      });
    }
  }
}

export const runCommandInternals = {
  resolveRetryConfig,
  isFetchFailed,
};
