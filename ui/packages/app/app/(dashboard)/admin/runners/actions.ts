"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { requireScope } from "@/lib/actions/require-scope";
import { SCOPE } from "@/lib/auth/scopes";
import {
  listRunners,
  listRunnerLeases,
  createRunner,
  updateRunnerAdminState,
  deleteRunner,
  type RunnerListResponse,
  type RunnerLeaseResponse,
  type CreatedRunner,
  type RunnerAdminAction,
  type RunnerAdminStateUpdate,
  type ListParams,
  type LeaseListParams,
  type SandboxTier,
} from "@/lib/api/runners";

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

// runner:write, the same scope as revoke — deleting an already-revoked record is
// strictly less consequential than taking a live runner out of service, so
// gating it higher would be backwards.
export async function deleteRunnerAction(runnerId: string): Promise<ActionResult<void>> {
  return requireScope(SCOPE.RUNNER_WRITE, () => withToken((t) => deleteRunner(t, runnerId)));
}
