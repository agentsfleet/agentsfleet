"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { hasScope } from "@/lib/auth/platform";
import { SCOPE } from "@/lib/auth/scopes";
import { ERROR_CODE } from "@/lib/errors";
import {
  listRunners,
  createRunner,
  updateRunnerAdminState,
  listRunnerEvents,
  type RunnerListResponse,
  type CreatedRunner,
  type RunnerAdminAction,
  type RunnerAdminStateUpdate,
  type RunnerEventsResponse,
  type ListParams,
  type EventListParams,
  type SandboxTier,
} from "@/lib/api/runners";

// Defence-in-depth: gate each runner action on the specific operator scope its
// backend route enforces (route_scopes.zig) before the round-trip. The backend
// independently 403s a token missing the scope (UZ-AUTH-022) — this just fails
// fast so the UI never round-trips a request the token can't satisfy.
async function requireScope<T>(scope: string, fn: () => Promise<ActionResult<T>>): Promise<ActionResult<T>> {
  if (!(await hasScope(scope))) {
    return {
      ok: false,
      error: `Operator scope required: ${scope}`,
      status: 403,
      errorCode: ERROR_CODE.INSUFFICIENT_SCOPE,
    };
  }
  return fn();
}

export async function listRunnersAction(params: ListParams): Promise<ActionResult<RunnerListResponse>> {
  return requireScope(SCOPE.RUNNER_READ, () => withToken((t) => listRunners(t, params)));
}

export async function createRunnerAction(body: {
  host_id: string;
  sandbox_tier: SandboxTier;
  labels: string[];
}): Promise<ActionResult<CreatedRunner>> {
  return requireScope(SCOPE.RUNNER_ENROLL, () => withToken((t) => createRunner(t, body)));
}

export async function updateRunnerAdminStateAction(
  runnerId: string,
  action: RunnerAdminAction,
): Promise<ActionResult<RunnerAdminStateUpdate>> {
  return requireScope(SCOPE.RUNNER_WRITE, () => withToken((t) => updateRunnerAdminState(t, runnerId, action)));
}

export async function listRunnerEventsAction(
  runnerId: string,
  params: EventListParams,
): Promise<ActionResult<RunnerEventsResponse>> {
  return requireScope(SCOPE.RUNNER_READ, () => withToken((t) => listRunnerEvents(t, runnerId, params)));
}
