"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { requireScope } from "@/lib/actions/require-scope";
import { SCOPE } from "@/lib/auth/scopes";
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
