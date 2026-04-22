/** Domain types — mirrors zombied API contracts */

export type CommandClass = "safe" | "sensitive" | "critical";

export type ApiError = {
  error: string;
  code: string;
  status: number;
};

// ── Zombies ──

export type Zombie = {
  id: string;
  name: string;
  status: string;
  created_at: number;
  updated_at: number;
};

export type InstallZombieRequest = {
  name: string;
  source_markdown: string;
  config_json: string;
};

export type InstallZombieResponse = {
  zombie_id: string;
  status: string;
};

export type ZombieListResponse = {
  items: Zombie[];
  total: number;
  cursor: string | null;
};

// ── Tenant billing ──

// Unix-epoch timestamps on this type are **milliseconds**, matching the
// server's `*_at_ms` fields (src/state/tenant_billing_store.zig). Pass them
// straight to `new Date(n)`; never multiply by 1000.
export type TenantBilling = {
  plan_tier: string;
  plan_sku: string;
  balance_cents: number;
  updated_at: number;
  is_exhausted: boolean;
  exhausted_at: number | null;
};
