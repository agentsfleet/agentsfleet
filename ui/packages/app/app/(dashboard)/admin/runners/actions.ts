"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { requireScope } from "@/lib/actions/require-scope";
import { SCOPE } from "@/lib/auth/scopes";
import { listRunners, listRunnerLeases, createRunner, updateRunnerAdminState, updateRunner, deleteRunner, requestRunnerSelftest, type RunnerListResponse, type RunnerLeaseResponse, type CreatedRunner, type RunnerAdminStateUpdate, type RunnerEditResponse, type RunnerEdit, type RunnerSelftestRequest, type ListParams, type LeaseListParams } from "@/lib/api/runners";
import { type AssignedPolicy, type RunnerAdminAction } from "@/lib/api/runners-types";

export async function listRunnersAction(params: ListParams): Promise<ActionResult<RunnerListResponse>> {
  return requireScope(SCOPE.RUNNER_READ, () => withToken((t) => listRunners(t, params)));
}

// The busy tile's work line and the lease table both read this; the detail
// page itself fetches server-side for the cursor the URL names.
export async function listRunnerLeasesAction(
  runnerId: string,
  params: LeaseListParams,
): Promise<ActionResult<RunnerLeaseResponse>> {
  return requireScope(SCOPE.RUNNER_READ, () => withToken((t) => listRunnerLeases(t, runnerId, params)));
}

export async function createRunnerAction(body: {
  host_id: string;
  assigned_policy: AssignedPolicy;
  labels: string[];
}): Promise<ActionResult<CreatedRunner>> {
  return requireScope(SCOPE.RUNNER_ENROLL, () =>
    withToken((t) =>
      createRunner(t, { host_id: body.host_id, assigned_policy: body.assigned_policy, labels: body.labels }),
    ),
  );
}

/** Save policy and label edits through the same runner write permission. */
export async function updateRunnerAction(
  runnerId: string,
  edit: RunnerEdit,
): Promise<ActionResult<RunnerEditResponse>> {
  return requireScope(SCOPE.RUNNER_WRITE, () => withToken((t) => updateRunner(t, runnerId, edit)));
}

export async function updateRunnerAdminStateAction(
  runnerId: string,
  action: RunnerAdminAction,
): Promise<ActionResult<RunnerAdminStateUpdate>> {
  return requireScope(SCOPE.RUNNER_WRITE, () => withToken((t) => updateRunnerAdminState(t, runnerId, action)));
}

// runner:write, the same scope as the transitions — a self-test executes code
// inside the runner's sandbox and its result is an operator-visible statement
// about that host, so it is a write even though it moves no state.
export async function requestRunnerSelftestAction(
  runnerId: string,
): Promise<ActionResult<RunnerSelftestRequest>> {
  return requireScope(SCOPE.RUNNER_WRITE, () => withToken((t) => requestRunnerSelftest(t, runnerId)));
}

// runner:write, the same scope as revoke — deleting an already-revoked record is
// strictly less consequential than taking a live runner out of service, so
// gating it higher would be backwards.
export async function deleteRunnerAction(runnerId: string): Promise<ActionResult<void>> {
  return requireScope(SCOPE.RUNNER_WRITE, () => withToken((t) => deleteRunner(t, runnerId)));
}
