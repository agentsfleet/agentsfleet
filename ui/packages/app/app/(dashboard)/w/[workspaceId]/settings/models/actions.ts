"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  resetTenantProvider as apiResetTenantProvider,
  setTenantProviderSelfManaged as apiSetTenantProviderSelfManaged,
} from "@/lib/api/tenant_provider";
import { listSecrets as apiListSecrets, rotateSecret as apiRotateSecret } from "@/lib/api/secrets";
import { getModelLibrary as apiGetModelLibrary, type ModelLibrary } from "@/lib/api/model_library";
import {
  listTenantModelEntries as apiListTenantModelEntries,
  createTenantModelEntry as apiCreateTenantModelEntry,
  updateTenantModelEntry as apiUpdateTenantModelEntry,
  deleteTenantModelEntry as apiDeleteTenantModelEntry,
} from "@/lib/api/tenant_model_entries";
import type { SecretListResponse } from "@/lib/api/secrets";
import type { TenantModelEntryList, TenantModelEntryWriteResult, TenantProvider } from "@/lib/types";

// The model library read (GET /v1/models) is bearer-authed; the client-side
// catalogue provider fetches through this action so the token never reaches
// the browser.
export async function getModelLibraryAction(): Promise<ActionResult<ModelLibrary>> {
  return withToken((t) => apiGetModelLibrary(t));
}

export async function setProviderSelfManagedAction(
  body: { secret_ref: string; model?: string },
): Promise<ActionResult<TenantProvider>> {
  return withToken((t) => apiSetTenantProviderSelfManaged(body, t));
}

export async function resetProviderAction(): Promise<ActionResult<TenantProvider>> {
  return withToken((t) => apiResetTenantProvider(t));
}

// ── Model registry (M121) ── one row per configured model; two entries can
// share a `secret_ref`. Activation stays on setProviderSelfManagedAction /
// resetProviderAction above — these four only list/register/rename/remove
// the registry rows themselves.

export async function listModelEntriesAction(): Promise<ActionResult<TenantModelEntryList>> {
  return withToken((t) => apiListTenantModelEntries(t));
}

export async function createModelEntryAction(
  body: { model_id: string; secret_ref: string },
): Promise<ActionResult<TenantModelEntryWriteResult>> {
  return withToken((t) => apiCreateTenantModelEntry(body, t));
}

export async function updateModelEntryAction(
  id: string,
  body: { model_id: string },
): Promise<ActionResult<TenantModelEntryWriteResult>> {
  return withToken((t) => apiUpdateTenantModelEntry(id, body, t));
}

export async function deleteModelEntryAction(id: string): Promise<ActionResult<void>> {
  return withToken((t) => apiDeleteTenantModelEntry(id, t));
}

// Client-side refetch of the page's SSR `secrets` prop — used after a model
// entry create commits a new secret, so ModelsRegistryTable can pick it up
// without re-running the whole Server Component tree (no router.refresh()).
export async function listSecretsAction(workspaceId: string): Promise<ActionResult<SecretListResponse>> {
  return withToken((t) => apiListSecrets(workspaceId, t));
}

// Rotate only the api_key of a stored secret (PATCH …/secrets/{name}).
// The server preserves provider/model/base_url, so this is the Replace-key
// action for the active-model hero — safe for every kind.
export async function rotateSecretAction(
  workspaceId: string,
  name: string,
  apiKey: string,
): Promise<ActionResult<{ name: string }>> {
  return withToken((t) => apiRotateSecret(workspaceId, name, apiKey, t));
}
