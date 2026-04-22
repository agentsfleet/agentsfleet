import type { TenantBilling } from "../types";

const BASE = process.env.NEXT_PUBLIC_API_URL ?? "https://api.usezombie.com";

export async function getTenantBilling(token: string): Promise<TenantBilling> {
  const res = await fetch(`${BASE}/v1/tenants/me/billing`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw Object.assign(new Error(body.error ?? res.statusText), {
      status: res.status,
      code: body.code,
    });
  }
  return res.json() as Promise<TenantBilling>;
}
