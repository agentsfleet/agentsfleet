/** Domain types — mirrors agentsfleetd API rules */

// Type-only — erased at compile time, so this does not create a runtime
// cycle with secrets.ts's own `import { SECRET_FIELD } from "@/lib/types"`.
import type { SecretKind } from "./api/secrets";

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
// values the front-end doesn't recognise yet. Consumers should narrow with
// `AGENTSFLEET_STATUS` from `lib/api/fleets` before branching.
export type Fleet = {
  id: string;
  name: string;
  status: string;
  created_at: number;
  updated_at: number;
  triggers?: FleetTrigger[];
};

// The single-fleet detail read (`GET …/fleets/{id}`, M131 §1) — the list row
// plus the editable source and the bundle pin. `trigger_markdown` /
// `bundle_content_hash` are nullable columns that serialize as JSON null.
// `budget_used_nanos` / `events_processed` are server-truth lifetime counters
// (denormalized on the fleet row); the console never derives cost from tokens.
export type FleetDetail = {
  id: string;
  name: string;
  status: string;
  source_markdown: string;
  trigger_markdown: string | null;
  bundle_content_hash: string | null;
  triggers: FleetTrigger[] | null;
  events_processed: number;
  budget_used_nanos: number;
  created_at: number;
  updated_at: number;
};

// One durable memory entry as the tenant read returns it (`GET …/memories`).
// The field is **`content`**, not `text` — the memory store's column name.
export type MemoryEntry = {
  key: string;
  content: string;
  category: string;
  /** epoch milliseconds */
  updated_at: number;
};

// Install a fleet from exactly one onboarded library tier: a platform entry
// (slug id) or this workspace's tenant entry. The `?: never` arms make the two
// mutually exclusive at compile time; raw-`SKILL.md` paste, per-workspace
// `bundle_id`, and github-import-at-create are not accepted. An optional `name`
// overrides the SKILL.md-derived fleet name so one library entry can back
// multiple fleets in a workspace.
export type InstallFleetRequest =
  | { platform_library_id: string; name?: string; tenant_library_id?: never }
  | { tenant_library_id: string; name?: string; platform_library_id?: never };

export type InstallFleetResponse = {
  fleet_id: string;
  status: string;
};

// ── Fleet library catalog (two-tier) ──
// The platform catalog and per-workspace tenant entries, unioned by the
// workspace gallery. Object storage holds the canonical tar; these rows are
// metadata only, never support-file bytes or an object-store key. Mirrors
// agentsfleetd `http/handlers/library/gallery.zig` (GalleryEntry).

// The catalog tier of a library entry. The install flow keys the create body off it:
// platform → `platform_library_id`, tenant → `tenant_library_id`.
export type FleetLibraryVisibility = "platform" | "tenant";

// A non-authoritative support file shipped alongside SKILL.md/TRIGGER.md, shown
// as a {path, size_bytes} summary — the bytes live in R2, never in the response.
export type FleetLibrarySupportFileSummary = { path: string; size_bytes: number };

// A library entry's declared requirements — drives the install gate's
// credential preview and the skill-only fallback when no TRIGGER.md shipped.
export type FleetLibraryRequirements = {
  credentials: string[];
  tools: string[];
  network_hosts: string[];
  trigger_present: boolean;
};

// One gallery row from GET /v1/workspaces/{ws}/fleet-libraries — a platform or
// tenant library entry. Metadata only; `visibility` is the tier the install
// flow keys the create body off.
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

export type OnboardLibraryEntryRequest =
  | {
      source_kind: typeof SOURCE_KIND_GITHUB;
      source_ref: string;
      // Branch or tag to fetch at; absent fetches the default branch. The
      // catalog's Fetch-update sends the row's stored ref, so a PATCH-pinned
      // ref is honored by the next fetch rather than silently reset (M130).
      ref?: string;
      // Platform tier only: overwrite a catalog id already owned by a DIFFERENT
      // repository. Absent means a collision is a 409 the operator must confirm.
      replace?: boolean;
    }
  | {
      source_kind: typeof SOURCE_KIND_UPLOAD;
      skill_markdown: string;
      trigger_markdown?: string;
    };

// The onboard response — identical on both tiers except which catalogue the
// entry landed in. The two endpoints (workspace-scoped vs `/v1/admin/…`) differ
// only in the scope they demand, so the tier is carried as the discriminant
// rather than duplicated as two independent shapes.
type OnboardedLibraryEntryBase = {
  id: string;
  name: string;
  content_hash: string;
  requirements: FleetLibraryRequirements;
  support_files: FleetLibrarySupportFileSummary[];
};

export type OnboardedLibraryEntry = OnboardedLibraryEntryBase & { visibility: "tenant" };

// Onboarded by a `platform-library:write` operator. Stored `visibility='draft'`
// server-side (M128): adding a fleet never publishes it, so it reaches no tenant
// until an operator says so. The tier is still reported as "platform".
export type OnboardedPlatformLibraryEntry = OnboardedLibraryEntryBase & { visibility: "platform" };

// ── The platform catalog (M128) — GET /v1/admin/fleet-libraries ──────────────
//
// The publish lifecycle as the server stores it. Spelled verbatim in
// src/agentsfleetd/fleet_library/library_store.zig and in
// schema/023_fleet_library.sql; SQL cannot import a Zig constant and TypeScript
// cannot import either, so the three must agree by hand. A drift silently hides
// or exposes fleets, which is why an integration test pins it.
export const CATALOG_DRAFT = "draft" as const;
export const CATALOG_PUBLIC = "public" as const;
export type CatalogVisibility = typeof CATALOG_DRAFT | typeof CATALOG_PUBLIC;

// One row of the platform catalog, as the operator sees it. Unlike a gallery
// entry this hides nothing — a draft, and a row whose bundle was never fetched,
// are exactly what the operator needs to see.
export type PlatformCatalogEntry = {
  id: string;
  name: string;
  description: string;
  source_repo: string;
  source_ref: string;
  visibility: CatalogVisibility;
  // null ⇒ no bundle has ever been fetched. Such a row can never be published.
  content_hash: string | null;
  requirements: FleetLibraryRequirements;
  required_credentials_reasons?: Record<string, string>;
  support_files: FleetLibrarySupportFileSummary[];
  updated_at: number;
};

export type PlatformCatalogResponse = { entries: PlatformCatalogEntry[] };

// The four states a row can be in, derived — never a wire field. Two sources for
// one fact is how a table starts lying.
export const CATALOG_STATUS_NO_BUNDLE = "no_bundle" as const;
export const CATALOG_STATUS_DRAFT = "draft" as const;
export const CATALOG_STATUS_PUBLISHED = "published" as const;
/// Public, but holding no bundle. The gallery and install queries both require
/// `content_hash IS NOT NULL`, so such a row is invisible and uninstallable —
/// calling it "published" asserts the opposite of what the system does.
export const CATALOG_STATUS_BROKEN = "broken" as const;
export type CatalogStatus =
  | typeof CATALOG_STATUS_NO_BUNDLE
  | typeof CATALOG_STATUS_DRAFT
  | typeof CATALOG_STATUS_PUBLISHED
  | typeof CATALOG_STATUS_BROKEN;

/// Total over both axes — visibility AND the bundle, never visibility alone.
/// The API refuses to create the public-without-bundle row (publishing checks
/// the hash in SQL), but a hand-inserted one exists in the wild, and a surface
/// that assumes it away lies about it.
///
/// "Has a bundle" means exactly what the server means: `content_hash IS NOT
/// NULL`. An empty-string hash counts as a bundle here BECAUSE it counts as one
/// in every server guard — diverging (treating "" as bundle-less) would hide a
/// Publish the API accepts, which is the exact class of lie this function exists
/// to end.
export function catalogStatus(entry: PlatformCatalogEntry): CatalogStatus {
  const hasBundle = entry.content_hash !== null;
  if (entry.visibility === CATALOG_PUBLIC) {
    return hasBundle ? CATALOG_STATUS_PUBLISHED : CATALOG_STATUS_BROKEN;
  }
  return hasBundle ? CATALOG_STATUS_DRAFT : CATALOG_STATUS_NO_BUNDLE;
}

// A partial update. An absent field is left untouched, so editing the description
// never blanks the credential copy.
//
// `id` is deliberately absent and stays absent: it is the primary key, and a
// workspace install references it as `platform_library_id`. Moving it would
// orphan every install.
//
// Changing `source_repo` or `source_ref` invalidates the stored bundle — it was
// built from the OLD repository — so the server nulls `content_hash` and stages
// the row back to draft. Re-sending an unchanged value does neither.
export type PlatformCatalogPatch = {
  name?: string;
  description?: string;
  source_repo?: string;
  source_ref?: string;
  required_credentials_reasons?: Record<string, string>;
  published?: boolean;
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
  /** Whether an active platform default exists, independent of `mode` — lets
   * the Models page gate "Switch to Default" before the click. */
  platform_default_available: boolean;
};

// ── Tenant model registry ──
// One row per configured `(model_id, secret_ref)` pair — see
// src/agentsfleetd/http/handlers/tenant_model_entries.zig for the wire
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
