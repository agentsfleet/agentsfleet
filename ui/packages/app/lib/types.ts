/** Domain types — mirrors agentsfleetd API rules */

// Type-only — erased at compile time, so this does not create a runtime
// cycle with secrets.ts's own `import { SECRET_FIELD } from "@/lib/types"`.
import type { SecretKind } from "./api/secrets-types";
import type { NativeFleetDetail, NativeFleetSummary } from "./api/fleet-detail";

export type CommandClass = "safe" | "sensitive" | "critical";

export type ApiError = {
  error: string;
  code: string;
  status: number;
};

// ── Fleets ──

// The server projects each trigger in the native definition into fleet list
// rows, served as `FleetSummary` in rustd/crates/afd_api_wire/src/fleet/collection.rs.
// Tagged union by `type` — webhook
// carries source + events; cron carries the raw schedule expression; mention
// carries the chat provider and the one channel it answers in.
export const AGENT_TRIGGER_TYPE = {webhook: "webhook", cron: "cron", mention: "mention"} as const;

export type FleetTrigger =
  | { type: "webhook"; source: string; events?: string[] }
  | { type: "cron"; schedule: string }
  | { type: "mention"; source: string; channels: string[] };

export type Fleet = NativeFleetSummary;

export type FleetDetail = NativeFleetDetail;

// A fleet's two shared-memory grants (`PATCH …/fleets/{id}/memory-access`).
export type MemoryAccess = {
  read: boolean;
  publish: boolean;
};

// Who reads a stored memory entry: its writer alone, or every fleet in the
// workspace granted to read shared memory.
export type MemoryVisibility = "fleet" | "workspace";

// One durable memory entry as the tenant read returns it (`GET …/memories`).
// The field is **`content`**, not `text` — the memory store's column name.
export type MemoryEntry = {
  key: string;
  content: string;
  category: string;
  /** epoch milliseconds */
  updated_at: number;
  visibility: MemoryVisibility;
  /** The fleet that wrote it; another fleet's id marks an entry this fleet
   * reads and cannot forget. */
  writer_fleet_id: string;
};

export type FleetListResponse = {
  items: Fleet[];
  total: null;
  next_cursor: string | null;
};

// ── Tenant billing ──

// Canonical billing unit: 1 USD = 1_000_000_000 nanos. A JavaScript Number
// holds the full range (≤ 2^53 ≈ 9e15 nanos / ~$9M tenant balance) without
// precision loss. This is the wire format's denominator, not a price: no
// balance or ledger row can be read without it. Rates are the server's — the
// dashboard renders the charge it was served and computes none of its own.
export const NANOS_PER_USD = 1_000_000_000;

// Unix-epoch timestamps on this type are **milliseconds**, matching the
// server's `*_at_ms` fields (rustd/crates/afd_billing/src/nanos.rs). Pass them
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

// Provider id that opts a self-managed credential into a custom OpenAI-compatible
// endpoint. Mirrors the backend resolver
// (`rustd/crates/afd_credential/src/provider/endpoint/mod.rs` →
// `OPENAI_COMPATIBLE`) and the CLI's `constants/custom-endpoint.ts`: a
// credential carrying this provider requires a `base_url` (https, SSRF-validated
// server-side), forbidden for every named provider. The literal lives here once
// for the UI; every reader (the custom credential form, the Models own-key
// option, and the tests) imports it (RULE UFS).
export const OPENAI_COMPATIBLE_PROVIDER = "openai-compatible" as const;

// Secret JSON field names (verbatim with the server-side resolver's
// `S_API_KEY` / `S_BASE_URL` extraction).
export const SECRET_FIELD = {
  provider: "provider",
  apiKey: "api_key",
  baseUrl: "base_url",
  model: "model",
} as const;

// The only scheme a custom endpoint may use — checked client-side for an inline
// flag before submit; the server re-checks and also blocks SSRF-unsafe hosts.
export const HTTPS_SCHEME_PREFIX = "https://" as const;

// Runtime event costs and settled setup attempts share the tenant cost history.
export type ChargeType = "receive" | "stage" | "authoring";

export const CHARGE_TYPE = {
  receive: "receive" as ChargeType,
  stage: "stage" as ChargeType,
  authoring: "authoring" as ChargeType,
} as const;

export type TenantProvider = {
  mode: ProviderMode;
  provider: string;
  model: string;
  context_cap_tokens: number;
  secret_ref: string | null;
  /** Whether an active platform default exists, independent of `mode` — lets
   * the Models page gate "Switch to Default" before the click. */
  platform_default_available: boolean;
};

// ── Tenant model registry ──
// One row per configured `(model_id, secret_ref)` pair — see
// rustd/crates/afd_api_tenant/src/handler/tenant/model_entry/mod.rs for the wire
// shape. `provider`/`base_url`/`context_cap_tokens`/rates ride the
// `emit_null_optional_fields=false` shape (omitted, never null) — same
// convention as `Secret` in lib/api/secrets.ts.
export type TenantModelEntry = {
  id: string;
  model_id: string;
  secret_ref: string;
  provider?: string;
  kind: SecretKind;
  base_url?: string;
  has_key: boolean;
  context_cap_tokens?: number;
  input_nanos_per_mtok?: number;
  cached_input_nanos_per_mtok?: number;
  output_nanos_per_mtok?: number;
  active: boolean;
  created_at: number;
};

/** The active platform default's identity — rides the registry list so the
 * Models page renders the Default row without a second request. */
export type TenantPlatformDefault = {
  provider: string;
  model: string;
  context_cap_tokens: number;
  input_nanos_per_mtok?: number;
  cached_input_nanos_per_mtok?: number;
  output_nanos_per_mtok?: number;
};

export type TenantModelEntryList = {
  models: TenantModelEntry[];
  platform_default_available: boolean;
  /** Present iff `platform_default_available` — both derive from one server-side read. */
  platform_default?: TenantPlatformDefault;
};

export type TenantModelEntryWriteResult = {
  id: string;
  model_id: string;
  secret_ref: string;
  created_at: number;
};

export type TenantBillingChargesResponse = {
  items: Array<{
    id: string;
    tenant_id: string;
    workspace_id: string | null;
    /** Runtime identity survives deletion. Setup uses its retained draft's
     * public fleet link, or null when no saved fleet is available. */
    fleet_id: string | null;
    /** The fleet's name when the charge was written — a snapshot, so a later
     * rename never reaches it. Setup uses the linked fleet's current name. */
    fleet_name: string | null;
    event_id: string | null;
    authoring_attempt_id: string | null;
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
