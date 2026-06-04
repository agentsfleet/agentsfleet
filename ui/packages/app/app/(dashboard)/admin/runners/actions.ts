"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { readPlatformAdminClaim } from "@/lib/auth/platform";
import { ERROR_CODE } from "@/lib/errors";
import {
  listRunners,
  createRunner,
  type RunnerListResponse,
  type CreatedRunner,
  type ListParams,
  type SandboxTier,
} from "@/lib/api/runners";

// Defence-in-depth: gate every runner action on the platform_admin claim before
// the round-trip. The backend independently 403s a non-admin principal
// (UZ-AUTH-021) — this just fails fast and keeps the surface platform-only.
async function asPlatformAdmin<T>(fn: () => Promise<ActionResult<T>>): Promise<ActionResult<T>> {
  if (!(await readPlatformAdminClaim())) {
    return {
      ok: false,
      error: "Platform-admin access required",
      status: 403,
      errorCode: ERROR_CODE.PLATFORM_ADMIN_REQUIRED,
    };
  }
  return fn();
}

export async function listRunnersAction(params: ListParams): Promise<ActionResult<RunnerListResponse>> {
  return asPlatformAdmin(() => withToken((t) => listRunners(t, params)));
}

export async function createRunnerAction(body: {
  host_id: string;
  sandbox_tier: SandboxTier;
  labels: string[];
}): Promise<ActionResult<CreatedRunner>> {
  return asPlatformAdmin(() => withToken((t) => createRunner(t, body)));
}
