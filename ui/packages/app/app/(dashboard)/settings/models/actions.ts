"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  resetTenantProvider as apiResetTenantProvider,
  setTenantProviderSelfManaged as apiSetTenantProviderSelfManaged,
} from "@/lib/api/tenant_provider";
import type { TenantProvider } from "@/lib/types";

export async function setProviderSelfManagedAction(
  body: { credential_ref: string; model?: string },
): Promise<ActionResult<TenantProvider>> {
  return withToken((t) => apiSetTenantProviderSelfManaged(body, t));
}

export async function resetProviderAction(): Promise<ActionResult<TenantProvider>> {
  return withToken((t) => apiResetTenantProvider(t));
}
