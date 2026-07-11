"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { requireScope } from "@/lib/actions/require-scope";
import { SCOPE } from "@/lib/auth/scopes";
import { listTenantWorkspacesCached } from "@/lib/workspace";
import { createSecret } from "@/lib/api/secrets";
import {
  listAdminModels,
  createAdminModel,
  updateAdminModel,
  deleteAdminModel,
  setPlatformDefault,
  type AdminModel,
  type AdminModelList,
  type LibraryModelInput,
  type ModelRatesInput,
} from "@/lib/api/admin_model_library";

export async function listAdminModelsAction(): Promise<ActionResult<AdminModelList>> {
  return requireScope(SCOPE.MODEL_READ, () => withToken((t) => listAdminModels(t)));
}

export async function createAdminModelAction(body: LibraryModelInput): Promise<ActionResult<AdminModel>> {
  return requireScope(SCOPE.MODEL_ADMIN, () => withToken((t) => createAdminModel(t, body)));
}

export async function updateAdminModelAction(
  uid: string,
  body: ModelRatesInput,
): Promise<ActionResult<{ uid: string; updated: boolean }>> {
  return requireScope(SCOPE.MODEL_ADMIN, () => withToken((t) => updateAdminModel(t, uid, body)));
}

export async function deleteAdminModelAction(uid: string): Promise<ActionResult<void>> {
  return requireScope(SCOPE.MODEL_ADMIN, () => withToken((t) => deleteAdminModel(t, uid)));
}

/**
 * Set the active platform default. Two server-side steps under one token:
 *   1. write the api_key into the acting admin's workspace vault as the
 *      secret named for the provider (decision F — the key lives in the
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
  return requireScope(SCOPE.MODEL_ADMIN, () =>
    withToken(async (t) => {
      // admin/models is a platform surface with no workspace URL segment, so the
      // storage workspace is resolved explicitly from the authoritative tenant
      // list rather than a cookie/claim hint — `items[0]`, exactly as the pre-M118
      // `resolveFromList` fallback did (any owned workspace is a valid store for
      // the platform key; the resolver follows `source_workspace_id` into it).
      // Both writes run under the same id; an empty list is the genuine
      // "no workspace" error.
      const { items } = await listTenantWorkspacesCached(t);
      const workspaceId = items[0]?.id;
      if (!workspaceId) throw new Error("No active workspace to store the platform key in");

      const data: Record<string, unknown> = { provider: body.provider, api_key: body.api_key, model: body.model };
      if (body.base_url) data.base_url = body.base_url;
      await createSecret(workspaceId, { name: body.provider, data }, t);

      return setPlatformDefault(t, {
        provider: body.provider,
        source_workspace_id: workspaceId,
        model: body.model,
        base_url: body.base_url,
      });
    }),
  );
}
