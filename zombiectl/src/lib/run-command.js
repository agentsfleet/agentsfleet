// run-command.js — generic per-command boundary for zombiectl handlers.
// Owns the catch block currently inlined in cli.js's top-level: ApiError
// formatting, fetch-failed → API_UNREACHABLE, unknown → UNEXPECTED, and
// the cli_command_started / cli_command_finished / cli_error analytics
// triplet. Per spec M63_004 §3 (docs/v2/active/M63_004_P1_CLI_OBS_RESILIENCE.md).
//
// Handlers opt in by wrapping their body in runCommand({ ... }). The
// existing cli.js top-level catch stays in place as the safety net for
// any command that hasn't migrated yet — the wrapper is additive, not a
// replacement.

import { ApiError } from "./http.js";
import {
  trackCliEvent,
  getCliAnalyticsContext,
} from "./analytics.js";

const API_UNREACHABLE_CODE = "API_UNREACHABLE";
const UNEXPECTED_CODE = "UNEXPECTED";

function isFetchFailed(err) {
  return (
    err instanceof TypeError &&
    typeof err.message === "string" &&
    err.message.toLowerCase().includes("fetch failed")
  );
}

function resolveRetryConfig(retry) {
  // retry: undefined → caller (apiRequestWithRetry) picks the default.
  // retry: true → same as undefined (defer to apiRequestWithRetry default).
  // retry: false → collapse to 1 attempt for the handler's scope.
  // retry: { maxAttempts: N } → propagate verbatim.
  if (retry === undefined || retry === true) return null;
  if (retry === false) return { maxAttempts: 1 };
  return retry;
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
  if (typeof handler !== "function") {
    throw new TypeError("runCommand: handler must be a function");
  }
  if (typeof name !== "string" || name.length === 0) {
    throw new TypeError("runCommand: name must be a non-empty string");
  }

  // Build a per-invocation context with the retry config carried as a
  // single field. request() reads ctx.retryConfig and forwards it to
  // apiRequestWithRetry; signatures of apiRequest/apiRequestWithRetry
  // stay unchanged. ZOMBIE_NO_RETRY=1 still wins inside the HTTP layer.
  const handlerCtx = {
    ...(ctx || {}),
    retryConfig: resolveRetryConfig(retry),
  };

  const analyticsClient = deps.analyticsClient ?? handlerCtx.analyticsClient ?? null;
  const distinctId = deps.distinctId ?? handlerCtx.distinctId ?? "anonymous";
  const trackEvent = deps.trackCliEvent ?? trackCliEvent;
  const writeError = deps.writeError;
  const printJson = deps.printJson;
  const writeLine = deps.writeLine;
  const ui = deps.ui;

  const baseProps = {
    command: name,
    json_mode: String(handlerCtx.jsonMode ?? false),
    ...getCliAnalyticsContext(handlerCtx),
  };

  if (instrument) {
    trackEvent(analyticsClient, distinctId, "cli_command_started", baseProps);
  }

  try {
    const exitCode = await handler(handlerCtx);
    if (instrument) {
      trackEvent(analyticsClient, distinctId, "cli_command_finished", {
        ...baseProps,
        exit_code: String(exitCode ?? 0),
      });
    }
    return typeof exitCode === "number" ? exitCode : 0;
  } catch (err) {
    if (err instanceof ApiError) {
      const remap = errorMap[err.code];
      const finalCode = remap?.code ?? err.code ?? "API_ERROR";
      const finalMessage = remap?.message ?? err.message;
      if (instrument) {
        trackEvent(analyticsClient, distinctId, "cli_error", {
          ...baseProps,
          error_code: finalCode,
          exit_code: "1",
        });
      }
      if (typeof writeError === "function") {
        writeError({
          ctx: handlerCtx,
          code: finalCode,
          message: finalMessage,
          requestId: err.requestId,
          status: err.status,
          deps: { printJson, writeLine, ui },
        });
      }
      return 1;
    }

    if (isFetchFailed(err)) {
      if (instrument) {
        trackEvent(analyticsClient, distinctId, "cli_error", {
          ...baseProps,
          error_code: API_UNREACHABLE_CODE,
          exit_code: "1",
        });
      }
      if (typeof writeError === "function") {
        writeError({
          ctx: handlerCtx,
          code: API_UNREACHABLE_CODE,
          message: `cannot reach usezombie API at ${handlerCtx.apiUrl}`,
          deps: { printJson, writeLine, ui },
        });
      }
      return 1;
    }

    // Unknown — propagate UNEXPECTED so cli.js's outer safety net does
    // not double-report. We still emit the analytics event before
    // re-throwing so the outer catch can log without duplicating.
    if (instrument) {
      trackEvent(analyticsClient, distinctId, "cli_error", {
        ...baseProps,
        error_code: UNEXPECTED_CODE,
        exit_code: "1",
      });
    }
    if (typeof writeError === "function") {
      writeError({
        ctx: handlerCtx,
        code: UNEXPECTED_CODE,
        message: String(err?.message ?? err),
        deps: { printJson, writeLine, ui },
      });
      return 1;
    }
    throw err;
  }
}

export const runCommandInternals = {
  resolveRetryConfig,
  isFetchFailed,
};
