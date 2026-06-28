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

// Create accepts EITHER direct Markdown (paste/CLI) OR a `bundle_id` pointing
// at a stored snapshot — never both. The `?: never` arms make the two
// mutually exclusive at compile time, mirroring the immutable-bundle rule:
// a bundle install does not also carry override Markdown (see the
// Discovery 2026-06-20 codex review of create_fleet_bundle.zig). A bundle
// install may carry an optional `name` that overrides the SKILL.md-derived
// fleet name, so one bundle can back multiple fleets in a workspace.
export type InstallFleetRequest =
  | { source_markdown: string; trigger_markdown?: string; bundle_id?: never }
  | { bundle_id: string; name?: string; source_markdown?: never; trigger_markdown?: never };

export type InstallFleetResponse = {
  fleet_id: string;
  status: string;
};

// ── Fleet Bundles ──
// Source package layer above the runtime Fleet. Mirrors agentsfleetd
// `fleet_bundle/importer.zig` (ImportBody / Requirements) and the
// `fleet_bundles/{imports,get,list}.zig` response shapes.

// Backend accepts template | upload | github. The dashboard sends `template`
// (curated agentsfleet/skills source) or `github` (public owner/repo URL);
// `upload` is deferred (Discovery 2026-06-20). Paste does NOT use a
// bundle — it posts source_markdown directly to create.
export type BundleSourceKind = "template" | "upload" | "github";

// A non-authoritative support file shipped alongside SKILL.md/TRIGGER.md.
// Content is inert: capabilities come only from TRIGGER.md metadata ∩ grants.
export type BundleSupportFile = { path: string; content: string };
export type BundleSupportFileSummary = { path: string; size_bytes: number };

// Posted to POST /v1/workspaces/{ws}/fleets/bundles/snapshots. For `github`
// and `template` the server fetches and validates the source itself (SSRF +
// extraction guards) — the dashboard sends only `{ source_kind, source_ref }`.
// `skill_markdown`/`trigger_markdown` are inline content for the `upload`
// kind only (support files ride fetched sources, not uploads). The dashboard's
// paste path does NOT import — it posts source_markdown straight to create.
export type ImportBundleRequest = {
  source_kind: BundleSourceKind;
  source_ref: string;
  skill_markdown?: string;
  trigger_markdown?: string;
  support_files?: BundleSupportFile[];
};

// Parsed, declared requirements of a bundle — drives the install preview.
export type BundleRequirements = {
  credentials: string[];
  tools: string[];
  network_hosts: string[];
  support_files: string[];
  trigger_present: boolean;
};

// Snapshot import / detail response (immutable, content-addressed).
export type BundleSnapshot = {
  bundle_id: string;
  name: string;
  source_kind: BundleSourceKind;
  source_ref: string;
  validation_status: string;
  content_hash: string;
  snapshot_key: string;
  requirements: BundleRequirements;
  support_files: BundleSupportFileSummary[];
};

// First-party template catalog row from GET /v1/fleets/bundles (metadata only —
// the actual SKILL.md/TRIGGER.md content is fetched from agentsfleet/skills).
export type FleetTemplate = {
  id: string;
  name: string;
  description: string;
  required_credentials: string[];
  // Display-only "why this fleet needs it" copy, keyed by credential name (e.g.
  // { github: "review your pull requests" }). Drives the install gate's
  // purpose-driven prompt; absent keys fall back to the generic connect copy.
  // Optional: the catalog response is only cast on the client, so a cached
  // response, an old backend, or a mock template may omit it entirely — callers
  // default to {} so the gate shows generic copy instead of crashing.
  required_credentials_reasons?: Record<string, string>;
  required_tools: string[];
  network_hosts: string[];
};

export type FleetTemplateListResponse = { items: FleetTemplate[] };

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

// Credential JSON field names (verbatim with the server-side resolver's
// `S_API_KEY` / `S_BASE_URL` extraction).
export const CREDENTIAL_FIELD = {
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
  credential_ref: string | null;
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
