// run-command.js — generic per-command boundary for zombiectl handlers.
// Owns ApiError formatting (matching printApiError's
// `error: <code> <message>` + `request_id:` shape), fetch-failed →
// API_UNREACHABLE, unknown → UNEXPECTED, and the
// cli_command_started / cli_command_finished / cli_error analytics
// triplet.
//
// Rendering is self-contained: the wrapper writes directly to
// ctx.stderr via deps.printJson / deps.writeLine / deps.ui, so callers
// don't need to pre-wire io.js's positional writeError.

import { ApiError } from "./http.js";
import {
  trackCliEvent,
  getCliAnalyticsContext,
} from "./analytics.js";

const K_FUNCTION = "function";
const K_STRING = "string";
const K_API_ERROR = "API_ERROR";

const API_UNREACHABLE_CODE = "API_UNREACHABLE";
const UNEXPECTED_CODE = "UNEXPECTED";

function isFetchFailed(err) {
  return (
    err instanceof TypeError &&
    typeof err.message === K_STRING &&
    err.message.toLowerCase().includes("fetch failed")
  );
}

function resolveRetryConfig(retry) {
  // retry: undefined → caller (apiRequestWithRetry) picks the default.
  // retry: true → same as undefined.
  // retry: false → collapse to 1 attempt for the handler's scope.
  // retry: { maxAttempts: N } → propagate verbatim.
  if (retry === undefined || retry === true) return null;
  if (retry === false) return { maxAttempts: 1 };
  return retry;
}

function emitCliError(opts, errorCode) {
  if (!opts.instrument) return;
  opts.trackEvent(opts.analyticsClient, opts.distinctId, "cli_error", {
    ...opts.buildProps(),
    error_code: errorCode,
    exit_code: "1",
  });
}

function renderApi(opts, code, message, err) {
  const { handlerCtx, printJson, writeLine } = opts;
  const stderr = handlerCtx.stderr;
  if (!stderr || typeof writeLine !== K_FUNCTION) return;
  if (handlerCtx.jsonMode) {
    if (typeof printJson !== K_FUNCTION) return;
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

function renderPlain(opts, code, message) {
  const { handlerCtx, printJson, writeLine, ui } = opts;
  const stderr = handlerCtx.stderr;
  if (!stderr || typeof writeLine !== K_FUNCTION) return;
  if (handlerCtx.jsonMode) {
    if (typeof printJson !== K_FUNCTION) return;
    printJson(stderr, { error: { code, message } });
    return;
  }
  // Mirror cli.js's outer safety net + renderApi: human mode keeps
  // the `error: ` prefix so operators see the visual signal in
  // --no-color and CI environments. Coloring (when ui is present)
  // wraps the full prefixed line.
  const colorize = ui && typeof ui.err === K_FUNCTION ? ui.err : (s) => s;
  writeLine(stderr, colorize(`error: ${message}`));
}

export async function runCommand(opts) {
  const {
    name,
    handler,
    retry,
    instrument = true,
    errorMap = {},
    ctx,
    deps = {},
  } = opts;
  if (typeof handler !== K_FUNCTION) {
    throw new TypeError("runCommand: handler must be a function");
  }
  if (typeof name !== K_STRING || name.length === 0) {
    throw new TypeError("runCommand: name must be a non-empty string");
  }

  // Mutate the caller's ctx in place rather than copying. Handlers
  // already share this object via closure (see cli.js's registry
  // lambdas) — copying would mean retryConfig propagation and
  // setCliAnalyticsContext mutations during the handler don't round-
  // trip into the wrapper's post-handler events.
  const handlerCtx = ctx ?? {};
  handlerCtx.retryConfig = resolveRetryConfig(retry);

  const analyticsClient = deps.analyticsClient ?? handlerCtx.analyticsClient ?? null;
  const distinctId = deps.distinctId ?? handlerCtx.distinctId ?? "anonymous";
  const trackEvent = deps.trackCliEvent ?? trackCliEvent;
  const printJson = deps.printJson;
  const writeLine = deps.writeLine;
  const ui = deps.ui;

  // Re-evaluated for every event so handlers that call
  // setCliAnalyticsContext during execution have their additions
  // visible on cli_command_finished and cli_error (matching the
  // pre-migration cli.js behavior of spreading analyticsContext
  // post-handler).
  const buildProps = () => ({
    command: name,
    json_mode: String(handlerCtx.jsonMode ?? false),
    ...getCliAnalyticsContext(handlerCtx),
  });

  const renderOpts = {
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

  try {
    const exitCode = await handler(handlerCtx);
    if (instrument) {
      trackEvent(analyticsClient, distinctId, "cli_command_finished", {
        ...buildProps(),
        exit_code: String(exitCode ?? 0),
      });
    }
    return typeof exitCode === "number" ? exitCode : 0;
  } catch (err) {
    if (err instanceof ApiError) {
      const remap = errorMap[err.code];
      // Server's UZ-* code stays in stderr/JSON output so support and
      // grep workflows still match. The friendly remap.code is the
      // analytics dimension (cli_error.error_code) — that lets us
      // bucket events without leaking churn from server-side code
      // renames into the operator-facing surface.
      const displayCode = err.code ?? K_API_ERROR;
      const analyticsCode = remap?.code ?? err.code ?? K_API_ERROR;
      const finalMessage = remap?.message ?? err.message;
      emitCliError(renderOpts, analyticsCode);
      renderApi(renderOpts, displayCode, finalMessage, err);
      return 1;
    }

    if (isFetchFailed(err)) {
      const message = `cannot reach usezombie API at ${handlerCtx.apiUrl}`;
      emitCliError(renderOpts, API_UNREACHABLE_CODE);
      renderPlain(renderOpts, API_UNREACHABLE_CODE, message);
      return 1;
    }

    emitCliError(renderOpts, UNEXPECTED_CODE);
    renderPlain(renderOpts, UNEXPECTED_CODE, String(err?.message ?? err));
    return 1;
  }
}

export const runCommandInternals = {
  resolveRetryConfig,
  isFetchFailed,
};
