"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  resetTenantProvider as apiResetTenantProvider,
  setTenantProviderSelfManaged as apiSetTenantProviderSelfManaged,
} from "@/lib/api/tenant_provider";
import { rotateCredential as apiRotateCredential } from "@/lib/api/credentials";
import type { TenantProvider } from "@/lib/types";

export async function setProviderSelfManagedAction(
  body: { credential_ref: string; model?: string },
): Promise<ActionResult<TenantProvider>> {
  return withToken((t) => apiSetTenantProviderSelfManaged(body, t));
}

export async function resetProviderAction(): Promise<ActionResult<TenantProvider>> {
  return withToken((t) => apiResetTenantProvider(t));
}

// Rotate only the secret of a stored credential (PATCH …/credentials/{name}).
// The server preserves provider/model/base_url, so this is the Replace-key
// action for the active-model hero — safe for every kind.
export async function rotateCredentialAction(
  workspaceId: string,
  name: string,
  apiKey: string,
): Promise<ActionResult<{ name: string }>> {
  return withToken((t) => apiRotateCredential(workspaceId, name, apiKey, t));
}
