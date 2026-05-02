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

// ── Tenant LLM provider ──

export type ProviderMode = "platform" | "byok";

export const PROVIDER_MODE = {
  platform: "platform" as ProviderMode,
  byok: "byok" as ProviderMode,
} as const;

export type TenantProvider = {
  mode: ProviderMode;
  provider: string;
  model: string;
  context_cap_tokens: number;
  credential_ref: string | null;
  // Set when no tenant_providers row exists and the resolver returned the
  // platform default. UI uses this to surface "this is the default" copy.
  synthesised_default?: boolean;
  // Set when the resolver tried to load a BYOK credential and failed
  // (credential row missing from the vault). The UI surfaces this as a
  // warning banner.
  error?: string;
};

export type TenantBillingChargesResponse = {
  items: Array<{
    id: string;
    tenant_id: string;
    workspace_id: string;
    zombie_id: string;
    event_id: string;
    charge_type: "receive" | "stage";
    posture: ProviderMode;
    model: string;
    credit_deducted_cents: number;
    token_count_input: number | null;
    token_count_output: number | null;
    wall_ms: number | null;
    recorded_at: number;
  }>;
  next_cursor: string | null;
};
