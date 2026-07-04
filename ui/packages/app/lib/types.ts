/** Domain types — mirrors agentsfleetd API rules */

export type CommandClass = "safe" | "sensitive" | "critical";

export type ApiError = {
  error: string;
  code: string;
  status: number;
};

// ── Fleets ──

// Server projects `config_json->'x-agentsfleet'->'triggers'` into the list-row
// response (`src/http/handlers/fleets/list.zig` FleetListItem). One entry
// per declared trigger from `TRIGGER.md`. Tagged union by `type` — webhook
// carries source + events; cron carries the raw schedule expression.
export type FleetTrigger =
  | { type: "webhook"; source: string; events?: string[] }
  | { type: "cron"; schedule: string }
  | { type: "api" };

// `status` is typed as the loose `string` because the wire format may carry
// values the front-end doesn't recognise (forward-compat). Consumers should
// narrow with `AGENTSFLEET_STATUS` from `lib/api/fleets` before branching.
export type Fleet = {
  id: string;
  name: string;
  status: string;
  created_at: number;
  updated_at: number;
  triggers?: FleetTrigger[];
};

// Install a fleet from exactly one onboarded library tier (M103 §4): a platform
// entry (slug id) or this workspace's tenant entry (UUIDv7). The `?: never`
// arms make the two mutually exclusive at compile time; raw-`SKILL.md` paste, the
// legacy per-workspace `bundle_id`, and github-import-at-create are no longer
// accepted. An optional `name` overrides the SKILL.md-derived fleet name so one
// library entry can back multiple fleets in a workspace.
export type InstallFleetRequest =
  | { platform_library_id: string; name?: string; tenant_library_id?: never }
  | { tenant_library_id: string; name?: string; platform_library_id?: never };

export type InstallFleetResponse = {
  fleet_id: string;
  status: string;
};

// ── Fleet library catalog (two-tier) ──
// The platform catalog and per-workspace tenant entries, unioned by the
// workspace gallery (M103 §5). R2 holds the canonical tar; these rows are
// metadata only — never support-file bytes or an object-store key. Mirrors
// agentsfleetd `http/handlers/library/gallery.zig` (GalleryEntry).

// The catalog tier of a library entry. The install flow keys the create body off it:
// platform → `platform_library_id`, tenant → `tenant_library_id`.
export type FleetLibraryVisibility = "platform" | "tenant";

// A non-authoritative support file shipped alongside SKILL.md/TRIGGER.md, shown
// as a {path, size_bytes} summary — the bytes live in R2, never in the response.
export type FleetLibrarySupportFileSummary = { path: string; size_bytes: number };

// A template's declared requirements — drives the install gate's credential
// preview and the skill-only fallback when no TRIGGER.md shipped.
export type FleetLibraryRequirements = {
  credentials: string[];
  tools: string[];
  network_hosts: string[];
  trigger_present: boolean;
};

// One gallery row from GET /v1/workspaces/{ws}/fleet-libraries — a platform or
// tenant template. Metadata only; `visibility` is the tier the install flow keys
// the create body off.
export type FleetLibraryGalleryEntry = {
  id: string;
  name: string;
  description: string;
  visibility: FleetLibraryVisibility;
  source_ref: string;
  requirements: FleetLibraryRequirements;
  // Display-only "why this fleet needs it" copy, keyed by credential name (e.g.
  // { github: "review your pull requests" }). Platform rows carry curated copy;
  // tenant rows are an empty object (the importer derives no per-credential
  // reason), so the gate falls back to its generic connect prompt. Optional on
  // the client: the server always sends it (OpenAPI-required), but the gallery
  // response is only cast here, so a stale cache or an old backend may omit it —
  // callers default to {} so the gate degrades to generic copy, never crashes.
  required_credentials_reasons?: Record<string, string>;
  support_files: FleetLibrarySupportFileSummary[];
};

export type FleetLibraryGalleryResponse = { items: FleetLibraryGalleryEntry[] };

export const SOURCE_KIND_GITHUB = "github" as const;
export const SOURCE_KIND_UPLOAD = "upload" as const;

export type OnboardTemplateRequest =
  | {
      source_kind: typeof SOURCE_KIND_GITHUB;
      source_ref: string;
    }
  | {
      source_kind: typeof SOURCE_KIND_UPLOAD;
      skill_markdown: string;
      trigger_markdown?: string;
    };

export type OnboardedTemplate = {
  id: string;
  name: string;
  visibility: "tenant";
  content_hash: string;
  requirements: FleetLibraryRequirements;
  support_files: FleetLibrarySupportFileSummary[];
};

export type FleetListResponse = {
  items: Fleet[];
  total: number;
  cursor: string | null;
};

// ── Tenant billing ──

// Canonical billing unit: 1 USD = 1_000_000_000 nanos. JS Number holds the
// full range (≤ 2^53 ≈ 9e15 nanos / ~$9M tenant balance) without precision
// loss. Mirrors `NANOS_PER_USD` in src/state/tenant_billing.zig and
// cli/src/constants/billing.js — keep all three in lockstep.
export const NANOS_PER_USD = 1_000_000_000;

// Rate constants — mirror src/state/tenant_billing.zig identifier-for-identifier
// (cross-tier parity rule). The dashboard reads tenant balances and ledger
// rows in nanos; surfaces that quote an absolute rate import from here so a
// bump shows up everywhere on the same commit. Paired pin tests live in
// agentsfleet tests + tenant_billing_test.zig.
export const STARTER_CREDIT_NANOS = 5 * NANOS_PER_USD;
export const EVENT_NANOS = 0;
// Per-second run rate ($0.0001/sec ≈ $0.36/hr), charged identically under both
// postures while a Fleet is actively running. Replaces the former flat
// per-stage fees.
export const RUN_NANOS_PER_SEC = 100_000;

// Promotional free-trial window. While `now_ms < FREE_TRIAL_END_MS`, the
// server's `compute_stage_charge` returns FREE_TRIAL_STAGE_NANOS regardless
// of posture / model / tokens. The dashboard billing panel surfaces the
// active state from `GET /v1/tenants/me/billing.free_trial`. Customer-
// facing live state lives on agentsfleet.net/#pricing.
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
  // Promotional free-trial window state, from src/state/tenant_billing.zig.
  // `ends_at_ms` is epoch milliseconds (pass straight to `new Date(n)`).
  free_trial: { active: boolean; ends_at_ms: number };
};

// ── Tenant LLM provider ──

export type ProviderMode = "platform" | "self_managed";

export const PROVIDER_MODE = {
  platform: "platform" as ProviderMode,
  self_managed: "self_managed" as ProviderMode,
} as const;

// Provider id that opts a self-managed credential into a custom OpenAI-compatible
// endpoint. Mirrors the backend resolver
// (`src/agentsfleetd/state/tenant_provider_resolver.zig` →
// `OPENAI_COMPATIBLE_PROVIDER`) and the CLI's `constants/custom-endpoint.ts`: a
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

// Mirrors `ChargeType` enum in src/state/fleet_telemetry_store.zig — every
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
  secret_ref: string | null;
};

export type TenantBillingChargesResponse = {
  items: Array<{
    id: string;
    tenant_id: string;
    workspace_id: string;
    fleet_id: string;
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
