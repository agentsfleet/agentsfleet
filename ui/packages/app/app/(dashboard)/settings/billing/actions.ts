"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { listTenantBillingCharges as apiListTenantBillingCharges } from "@/lib/api/tenant_billing";
import type { TenantBillingChargesResponse } from "@/lib/types";

export async function listTenantBillingChargesAction(
  opts: { limit?: number; cursor?: string | null } = {},
): Promise<ActionResult<TenantBillingChargesResponse>> {
  return withToken((t) => apiListTenantBillingCharges(t, opts));
}
