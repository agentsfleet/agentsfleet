"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { readPlatformAdminClaim } from "@/lib/auth/platform";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { ERROR_CODE } from "@/lib/errors";
import { createCredential as apiCreateCredential } from "@/lib/api/credentials";
import {
  listAdminModels,
  createAdminModel,
  updateAdminModel,
  deleteAdminModel,
  setPlatformDefault,
  type AdminModel,
  type AdminModelList,
  type ModelCapInput,
  type ModelRatesInput,
} from "@/lib/api/admin_models";

// Defence-in-depth: gate every admin action on the platform_admin claim before
// the round-trip. The backend independently 403s a non-platform-admin principal
// — this just fails fast and keeps the surface platform-only (mirrors the
// runners admin actions).
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

export async function listAdminModelsAction(): Promise<ActionResult<AdminModelList>> {
  return asPlatformAdmin(() => withToken((t) => listAdminModels(t)));
}

export async function createAdminModelAction(body: ModelCapInput): Promise<ActionResult<AdminModel>> {
  return asPlatformAdmin(() => withToken((t) => createAdminModel(t, body)));
}

export async function updateAdminModelAction(
  uid: string,
  body: ModelRatesInput,
): Promise<ActionResult<{ uid: string; updated: boolean }>> {
  return asPlatformAdmin(() => withToken((t) => updateAdminModel(t, uid, body)));
}

export async function deleteAdminModelAction(uid: string): Promise<ActionResult<void>> {
  return asPlatformAdmin(() => withToken((t) => deleteAdminModel(t, uid)));
}

/**
 * Set the active platform default. Two server-side steps under one token:
 *   1. write the api_key into the acting admin's workspace vault as the
 *      credential named for the provider (decision F — the key lives in the
 *      admin's own workspace; the resolver follows source_workspace_id into it),
 *   2. PUT the platform-keys row (provider + catalogued model + base_url), which
 *      validates the model against the catalogue and stands every other row down.
 * The api_key never leaves the server — it is written straight to the vault and
 * never echoed back in any response.
 */
export async function setPlatformDefaultAction(body: {
  provider: string;
  model: string;
  api_key: string;
  base_url?: string;
}): Promise<ActionResult<{ provider: string; model: string; active: boolean }>> {
  return asPlatformAdmin(() =>
    withToken(async (t) => {
      const ws = await resolveActiveWorkspace(t);
      if (!ws) throw new Error("No active workspace to store the platform key in");

      const data: Record<string, unknown> = { provider: body.provider, api_key: body.api_key, model: body.model };
      if (body.base_url) data.base_url = body.base_url;
      await apiCreateCredential(ws.id, { name: body.provider, data }, t);

      return setPlatformDefault(t, {
        provider: body.provider,
        source_workspace_id: ws.id,
        model: body.model,
        base_url: body.base_url,
      });
    }),
  );
}
