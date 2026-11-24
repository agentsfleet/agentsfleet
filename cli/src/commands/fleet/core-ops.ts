// agentsfleet doctor — Effect-shaped.
//
// Runs three checks: server reachable (/healthz, no auth), workspace
// selected (read from Workspaces service), workspace binding valid
// (GET /v1/workspaces/{ws}/fleets, authed). Aggregates the results
// and exits 0/1 by the all-checks-ok fold.
//
// CliError variants are mapped down into per-check `ok=false, detail`
// rows by Effect.either + match, so doctor never short-circuits on
// the first transport failure — every check still runs.

import { Effect } from "effect";
import { CliConfig } from "../../services/config.ts";
import { Credentials } from "../../services/credentials.ts";
import { HttpClient } from "../../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../../services/output.ts";
import { Workspaces } from "../../services/workspaces.ts";
import {
  resolveAuthToken,
  WORKSPACE_CREATE_USAGE,
} from "../workspace/guards.ts";
import {
  wsFleetsPath,
  HEALTHZ_PATH,
  HEALTHZ_STATUS_OK,
} from "../../lib/api-paths.ts";
import { DOCTOR_CHECK } from "../../constants/doctor-checks.ts";
import type { CliError } from "../../errors/index.ts";

const SERVER_ERROR_TAG = "ServerError";

// Client-side fallback when an outgoing request fails without a
// server-supplied err.code (network failure, timeout, transport).
// Surfaced on the doctor JSON envelope's error.code field.
const REQUEST_FAILED = "REQUEST_FAILED";
const WORKSPACE_ACCESS_DENIED = "UZ-AUTH-001";
const CHECK_TEXT = {
  [DOCTOR_CHECK.SERVER_REACHABLE]: {passed: "API responding", failed: "API check failed"},
  [DOCTOR_CHECK.WORKSPACE_SELECTED]: {passed: "Workspace selected", failed: "Select a workspace"},
  [DOCTOR_CHECK.WORKSPACE_BINDING_VALID]: {passed: "Can list fleets in this workspace", failed: "Could not list fleets in this workspace"},
} as const;

interface DoctorCheckResult {
  readonly name: keyof typeof CHECK_TEXT;
  readonly ok: boolean;
  readonly detail: string;
}

const renderErrorDetail = (err: CliError): string => {
  if (err._tag === "NetworkError") return err.detail;
  if (err._tag === SERVER_ERROR_TAG) return `${err.code}: ${err.detail}`;
  return err.detail;
};

const runHealthzCheck: Effect.Effect<
  DoctorCheckResult,
  never,
  CliConfig | HttpClient
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const http = yield* HttpClient;
  return yield* http
    .request<{ status?: string }>({
      path: HEALTHZ_PATH,
    })
    .pipe(
      Effect.match({
        onSuccess: (res): DoctorCheckResult => {
          const ok = res?.status === HEALTHZ_STATUS_OK;
          return {
            name: DOCTOR_CHECK.SERVER_REACHABLE,
            ok,
            detail: ok
              ? `${config.apiUrl}${HEALTHZ_PATH}`
              : `unexpected payload: ${JSON.stringify(res)}`,
          };
        },
        onFailure: (err): DoctorCheckResult => ({
          name: DOCTOR_CHECK.SERVER_REACHABLE,
          ok: false,
          detail: `${config.apiUrl}${HEALTHZ_PATH}: ${renderErrorDetail(err)}`,
        }),
      }),
    );
});

const runBindingCheck = (
  wsId: string,
): Effect.Effect<
  DoctorCheckResult,
  never,
  CliConfig | Credentials | HttpClient
> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const tokenResult = yield* resolveAuthToken.pipe(
      Effect.match({
        onSuccess: (t) => ({ ok: true as const, token: t }),
        onFailure: (err) => ({ ok: false as const, err }),
      }),
    );
    if (!tokenResult.ok) {
      return {
        name: DOCTOR_CHECK.WORKSPACE_BINDING_VALID,
        ok: false,
        detail: `${wsId}: ${renderErrorDetail(tokenResult.err)}`,
      };
    }
    return yield* http
      .request<unknown>({
        path: wsFleetsPath(wsId),
        token: tokenResult.token,
      })
      .pipe(
        Effect.match({
          onSuccess: (): DoctorCheckResult => ({
            name: DOCTOR_CHECK.WORKSPACE_BINDING_VALID,
            ok: true,
            detail: `token bound to ${wsId}`,
          }),
          onFailure: (err): DoctorCheckResult => {
            const code =
              err._tag === SERVER_ERROR_TAG ? err.code : REQUEST_FAILED;
            const advice = code === WORKSPACE_ACCESS_DENIED
              ? "Select a workspace this credential can access, or use the intended credential."
              : err.suggestion;
            return {
              name: DOCTOR_CHECK.WORKSPACE_BINDING_VALID,
              ok: false,
              detail: `${wsId}: ${code} — ${advice}`,
            };
          },
        }),
      );
  });

const renderHuman = (
  checks: ReadonlyArray<DoctorCheckResult>,
  ok: boolean,
  workspace: string | null,
): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    yield* output.printSection("agentsfleet doctor");
    for (const check of checks) {
      const label = CHECK_TEXT[check.name];
      if (check.ok) {
        const detail = check.name === DOCTOR_CHECK.SERVER_REACHABLE ? check.detail
          : check.name === DOCTOR_CHECK.WORKSPACE_SELECTED ? workspace : null;
        yield* output.success(detail ? `${label.passed} · ${detail}` : label.passed);
      } else if (check.name === DOCTOR_CHECK.WORKSPACE_BINDING_VALID && workspace === null) {
        yield* output.info("– Fleet access check skipped until a workspace is selected");
      } else {
        yield* output.info(`✕ ${label.failed}`);
        yield* output.info(`  ${check.detail}`);
      }
    }
    yield* output.info("");
    const passed = checks.filter((c) => c.ok).length;
    yield* output.info(
      ok ? "All checks passed." : `${passed}/${checks.length} checks passed`,
    );
  });

export const doctorEffect: Effect.Effect<
  number,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const workspaces = yield* Workspaces;

  const healthz = yield* runHealthzCheck;

  const state = yield* workspaces.load;
  const wsId = state.current_workspace_id;
  const wsSelected = Boolean(wsId);
  const wsCheck: DoctorCheckResult = {
    name: DOCTOR_CHECK.WORKSPACE_SELECTED,
    ok: wsSelected,
    detail: wsSelected
      ? String(wsId)
      : `Run \`agentsfleet workspace list\` to see your workspaces, then \`agentsfleet workspace use <id>\`. To create one, run \`${WORKSPACE_CREATE_USAGE}\`.`,
  };

  const bindingCheck: DoctorCheckResult = wsId
    ? yield* runBindingCheck(wsId)
    : {
        name: DOCTOR_CHECK.WORKSPACE_BINDING_VALID,
        ok: false,
        detail: "skipped: no workspace selected",
      };

  const checks: ReadonlyArray<DoctorCheckResult> = [
    healthz,
    wsCheck,
    bindingCheck,
  ];
  const ok = checks.every((c) => c.ok);
  const report = { ok, api_url: config.apiUrl, checks };

  if (output.format !== OUTPUT_FORMAT.text) {
    yield* output.printJson(report);
  } else {
    yield* renderHuman(checks, ok, state.items.find(item => item.workspace_id === wsId)?.name ?? wsId);
  }
  return ok ? 0 : 1;
});
