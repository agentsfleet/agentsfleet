/** Domain types — mirrors zombied API contracts */

export type CommandClass = "safe" | "sensitive" | "critical";

export type ApiError = {
  error: string;
  code: string;
  status: number;
};

// ── Zombies ──

// Server projects `config_json->'x-usezombie'->'triggers'` into the list-row
// response (`src/http/handlers/zombies/list.zig` ZombieListItem). One entry
// per declared trigger from `TRIGGER.md`. Tagged union by `type` — webhook
// carries source + events; cron carries the raw schedule expression.
export type ZombieTrigger =
  | { type: "webhook"; source: string; events?: string[] }
  | { type: "cron"; schedule: string }
  | { type: "api" };

// `status` is typed as the loose `string` because the wire format may carry
// values the front-end doesn't recognise (forward-compat). Consumers should
// narrow with `ZOMBIE_STATUS` from `lib/api/zombies` before branching.
export type Zombie = {
  id: string;
  name: string;
  status: string;
  created_at: number;
  updated_at: number;
  triggers?: ZombieTrigger[];
};

export type InstallZombieRequest = {
  trigger_markdown: string;
  source_markdown: string;
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

// Canonical billing unit: 1 USD = 1_000_000_000 nanos. JS Number holds the
// full range (≤ 2^53 ≈ 9e15 nanos / ~$9M tenant balance) without precision
// loss. Mirrors `NANOS_PER_USD` in src/state/tenant_billing.zig and
// zombiectl/src/constants/billing.js — keep all three in lockstep.
export const NANOS_PER_USD = 1_000_000_000;

// Rate constants — mirror src/state/tenant_billing.zig identifier-for-identifier
// (cross-tier parity rule). The dashboard reads tenant balances and ledger
// rows in nanos; surfaces that quote an absolute rate import from here so a
// bump shows up everywhere on the same commit. Paired pin tests live in
// zombiectl tests + tenant_billing_test.zig.
export const STARTER_CREDIT_NANOS = 5 * NANOS_PER_USD;
export const EVENT_NANOS = 0;
export const STAGE_PLATFORM_NANOS = 1_000_000;
export const STAGE_SELF_MANAGED_NANOS = 100_000;

// Promotional free-trial window. While `now_ms < FREE_TRIAL_END_MS`, the
// server's `compute_stage_charge` returns FREE_TRIAL_STAGE_NANOS regardless
// of posture / model / tokens. The dashboard billing panel surfaces the
// active state from `GET /v1/tenants/me/billing.free_trial`. Customer-
// facing live state lives on usezombie.com/#pricing.
export const FREE_TRIAL_END_MS = 1_785_542_400_000; // 2026-08-01T00:00:00Z
export const FREE_TRIAL_STAGE_NANOS = 0;

// Unix-epoch timestamps on this type are **milliseconds**, matching the
// server's `*_at_ms` fields (src/state/tenant_billing.zig). Pass them
// straight to `new Date(n)`; never multiply by 1000.
export type TenantBilling = {
  balance_nanos: number;
  updated_at: number;
  is_exhausted: boolean;
  exhausted_at: number | null;
};

// ── Tenant LLM provider ──

export type ProviderMode = "platform" | "self_managed";

export const PROVIDER_MODE = {
  platform: "platform" as ProviderMode,
  self_managed: "self_managed" as ProviderMode,
} as const;

// Mirrors `ChargeType` enum in src/state/zombie_telemetry_store.zig — every
// metered event yields up to two rows, one per charge_type. Use this rather
// than typing "receive" / "stage" inline so a future rename catches every
// callsite via the type.
export type ChargeType = "receive" | "stage";

export const CHARGE_TYPE = {
  receive: "receive" as ChargeType,
  stage: "stage" as ChargeType,
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
  // Set when the resolver tried to load a self-managed credential and
  // failed (credential row missing from the vault). The UI surfaces this
  // as a warning banner.
  error?: string;
};

export type TenantBillingChargesResponse = {
  items: Array<{
    id: string;
    tenant_id: string;
    workspace_id: string;
    zombie_id: string;
    event_id: string;
    charge_type: ChargeType;
    posture: ProviderMode;
    model: string;
    credit_deducted_nanos: number;
    token_count_input: number | null;
    token_count_output: number | null;
    wall_ms: number | null;
    recorded_at: number;
  }>;
  next_cursor: string | null;
};
