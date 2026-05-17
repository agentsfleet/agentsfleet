// Tenant provider configuration: show / add / delete the active LLM
// posture (platform-managed default vs self-managed key with a named
// credential).
//
// Backed by /v1/tenants/me/provider — see src/http/handlers/tenant_provider.zig
// for the wire contract. The api_key is never returned in responses; this
// CLI only ever displays the resolved metadata (mode, provider, model,
// credential_ref, context_cap_tokens).

import {
  PROVIDER_MODE,
  formatDollars,
  NANOS_PER_USD,
} from "../constants/billing.ts";
import { AUTH_PRESET, compose } from "../lib/error-map-presets.ts";
import { TENANT_PROVIDER_PATH, TENANT_BILLING_PATH } from "../lib/api-paths.ts";
import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
} from "./types.ts";

// <$1 left → warn on reset.
const LOW_BALANCE_THRESHOLD_NANOS = NANOS_PER_USD;

interface ProviderResponse {
  mode?: string;
  provider?: string;
  model?: string;
  context_cap_tokens?: number;
  credential_ref?: string | null;
  synthesised_default?: boolean;
  error?: string;
  [key: string]: unknown;
}

interface BillingResponse {
  balance_nanos?: number;
  [key: string]: unknown;
}

interface ProviderAddBody {
  mode: string;
  credential_ref: string;
  model?: string;
}

// Tenant provider posture and billing snapshot. Auth-only at the CLI
// surface; provider-side validation codes are not yet keyed here and
// fall through to bare server messages.
export const errorMap = compose(AUTH_PRESET);

// ── tenant provider show ─────────────────────────────────────────────────────

export async function commandTenantProviderShow(
  ctx: CommandCtx,
  _parsed: ParsedArgs,
  _workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const res = (await request(ctx, TENANT_PROVIDER_PATH, {
    method: "GET",
    headers: apiHeaders(ctx),
  })) as ProviderResponse | null;

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, res);
    return 0;
  }

  // The handler surfaces resolver failures via an `error` field — surface
  // it before the table so the operator sees the broken state immediately.
  if (res && typeof res.error === "string" && res.error.length > 0 && ctx.stderr) {
    const ref = res.credential_ref ?? "(unknown)";
    if (res.error === "credential_missing") {
      writeLine(ctx.stderr, ui.err(`⚠ Credential ${ref} is missing from vault — re-add under the same name OR run 'zombiectl tenant provider delete'.`));
    } else {
      writeLine(ctx.stderr, ui.err(`⚠ Provider resolver error: ${res.error} (credential_ref=${ref})`));
    }
  }

  if (!ctx.stdout) return 0;
  printTable(ctx.stdout, [
    { key: "field", label: "FIELD" },
    { key: "value", label: "VALUE" },
  ], [
    { field: "mode",                value: res?.mode ?? "—" },
    { field: "provider",            value: res?.provider ?? "—" },
    { field: "model",               value: res?.model ?? "—" },
    { field: "context_cap_tokens",  value: typeof res?.context_cap_tokens === "number" ? String(res.context_cap_tokens) : "—" },
    { field: "credential_ref",      value: res?.credential_ref ?? "—" },
  ]);

  if (res?.synthesised_default === true) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim("(this is the platform default — no tenant_providers row)"));
  }
  return 0;
}

// ── tenant provider add ──────────────────────────────────────────────────────

export async function commandTenantProviderAdd(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  _workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const credOpt = parsed.options["credential"];
  const credentialRef = typeof credOpt === "string" ? credOpt : null;
  const modelOpt = parsed.options["model"];
  const modelOverride = typeof modelOpt === "string" ? modelOpt : null;

  if (!credentialRef) {
    if (ctx.stderr) {
      writeLine(ctx.stderr, ui.err("tenant provider add requires --credential <name>"));
      writeLine(ctx.stderr, ui.dim("(no default — pick the credential explicitly so the link to your vault entry is clear)"));
    }
    return 2;
  }

  const body: ProviderAddBody = {
    mode: PROVIDER_MODE.self_managed,
    credential_ref: credentialRef,
  };
  if (modelOverride) body.model = modelOverride;

  const res = (await request(ctx, TENANT_PROVIDER_PATH, {
    method: "PUT",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify(body),
  })) as ProviderResponse | null;

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, res);
    return 0;
  }

  if (!ctx.stdout) return 0;
  writeLine(ctx.stdout, ui.ok(`Tenant provider added: mode=${PROVIDER_MODE.self_managed} credential=${credentialRef}`));
  writeLine(ctx.stdout);
  printTable(ctx.stdout, [
    { key: "field", label: "FIELD" },
    { key: "value", label: "VALUE" },
  ], [
    { field: "mode",                value: res?.mode ?? "—" },
    { field: "provider",            value: res?.provider ?? "—" },
    { field: "model",               value: res?.model ?? "—" },
    { field: "context_cap_tokens",  value: typeof res?.context_cap_tokens === "number" ? String(res.context_cap_tokens) : "—" },
    { field: "credential_ref",      value: res?.credential_ref ?? "—" },
  ]);
  writeLine(ctx.stdout);
  writeLine(ctx.stdout, ui.dim(`Tip: run a test event to verify the key works against ${res?.provider ?? credentialRef}.`));
  return 0;
}

// ── tenant provider delete ───────────────────────────────────────────────────

export async function commandTenantProviderDelete(
  ctx: CommandCtx,
  _parsed: ParsedArgs,
  _workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const res = (await request(ctx, TENANT_PROVIDER_PATH, {
    method: "DELETE",
    headers: apiHeaders(ctx),
  })) as ProviderResponse | null;

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, res);
    return 0;
  }

  if (!ctx.stdout) return 0;
  writeLine(ctx.stdout, ui.ok("Tenant provider deleted; using platform default."));
  writeLine(ctx.stdout);
  printTable(ctx.stdout, [
    { key: "field", label: "FIELD" },
    { key: "value", label: "VALUE" },
  ], [
    { field: "mode",                value: res?.mode ?? "—" },
    { field: "provider",            value: res?.provider ?? "—" },
    { field: "model",               value: res?.model ?? "—" },
    { field: "context_cap_tokens",  value: typeof res?.context_cap_tokens === "number" ? String(res.context_cap_tokens) : "—" },
  ]);

  // Best-effort low-balance warning. Skip silently if the snapshot endpoint
  // isn't reachable — the reset itself succeeded and that's the headline.
  try {
    const billing = (await request(ctx, TENANT_BILLING_PATH, {
      method: "GET",
      headers: apiHeaders(ctx),
    })) as BillingResponse | null;
    const balance = typeof billing?.balance_nanos === "number" ? billing.balance_nanos : null;
    if (balance !== null && balance < LOW_BALANCE_THRESHOLD_NANOS) {
      writeLine(ctx.stdout);
      writeLine(ctx.stdout, ui.err(`⚠ Tenant balance is low: ${formatDollars(balance)}. Top up via the dashboard before the next event.`));
    }
  } catch {
    // ignore — informational warning only.
  }
  return 0;
}
