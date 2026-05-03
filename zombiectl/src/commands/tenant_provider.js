// Tenant provider configuration: show / add / delete the active LLM
// posture (platform-managed default vs BYOK with a named credential).
//
// Backed by /v1/tenants/me/provider — see src/http/handlers/tenant_provider.zig
// for the wire contract. The api_key is never returned in responses; this
// CLI only ever displays the resolved metadata (mode, provider, model,
// credential_ref, context_cap_tokens).

const TENANT_PROVIDER_PATH = "/v1/tenants/me/provider";
const LOW_BALANCE_THRESHOLD_CENTS = 100; // <$1 left → warn on reset

// ── tenant provider show ─────────────────────────────────────────────────────

export async function commandTenantProviderShow(ctx, _parsed, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const res = await request(ctx, TENANT_PROVIDER_PATH, { method: "GET", headers: apiHeaders(ctx) });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  // The handler surfaces resolver failures via an `error` field — surface
  // it before the table so the operator sees the broken state immediately.
  if (res && typeof res.error === "string" && res.error.length > 0) {
    const ref = res.credential_ref ?? "(unknown)";
    if (res.error === "credential_missing") {
      writeLine(ctx.stderr, ui.err(`⚠ Credential ${ref} is missing from vault — re-add under the same name OR run 'zombiectl tenant provider delete'.`));
    } else {
      writeLine(ctx.stderr, ui.err(`⚠ Provider resolver error: ${res.error} (credential_ref=${ref})`));
    }
  }

  printTable(ctx.stdout, [
    { key: "field", label: "FIELD" },
    { key: "value", label: "VALUE" },
  ], [
    { field: "mode",                value: res.mode ?? "—" },
    { field: "provider",            value: res.provider ?? "—" },
    { field: "model",               value: res.model ?? "—" },
    { field: "context_cap_tokens",  value: typeof res.context_cap_tokens === "number" ? String(res.context_cap_tokens) : "—" },
    { field: "credential_ref",      value: res.credential_ref ?? "—" },
  ]);

  if (res.synthesised_default === true) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim("(this is the platform default — no tenant_providers row)"));
  }
  return 0;
}

// ── tenant provider add ──────────────────────────────────────────────────────

export async function commandTenantProviderAdd(ctx, parsed, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const credentialRef = parsed.options["credential"];
  const modelOverride = parsed.options["model"];

  if (!credentialRef) {
    writeLine(ctx.stderr, ui.err("tenant provider add requires --credential <name>"));
    writeLine(ctx.stderr, ui.dim("(no default — pick the credential explicitly so the link to your vault entry is clear)"));
    return 2;
  }

  const body = { mode: "byok", credential_ref: credentialRef };
  if (modelOverride) body.model = modelOverride;

  const res = await request(ctx, TENANT_PROVIDER_PATH, {
    method: "PUT",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  writeLine(ctx.stdout, ui.ok(`Tenant provider added: mode=byok credential=${credentialRef}`));
  writeLine(ctx.stdout);
  printTable(ctx.stdout, [
    { key: "field", label: "FIELD" },
    { key: "value", label: "VALUE" },
  ], [
    { field: "mode",                value: res.mode ?? "—" },
    { field: "provider",            value: res.provider ?? "—" },
    { field: "model",               value: res.model ?? "—" },
    { field: "context_cap_tokens",  value: typeof res.context_cap_tokens === "number" ? String(res.context_cap_tokens) : "—" },
    { field: "credential_ref",      value: res.credential_ref ?? "—" },
  ]);
  writeLine(ctx.stdout);
  writeLine(ctx.stdout, ui.dim(`Tip: run a test event to verify the key works against ${res.provider ?? credentialRef}.`));
  return 0;
}

// ── tenant provider delete ───────────────────────────────────────────────────

export async function commandTenantProviderDelete(ctx, _parsed, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const res = await request(ctx, TENANT_PROVIDER_PATH, {
    method: "DELETE",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  writeLine(ctx.stdout, ui.ok("Tenant provider deleted; using platform default."));
  writeLine(ctx.stdout);
  printTable(ctx.stdout, [
    { key: "field", label: "FIELD" },
    { key: "value", label: "VALUE" },
  ], [
    { field: "mode",                value: res.mode ?? "—" },
    { field: "provider",            value: res.provider ?? "—" },
    { field: "model",               value: res.model ?? "—" },
    { field: "context_cap_tokens",  value: typeof res.context_cap_tokens === "number" ? String(res.context_cap_tokens) : "—" },
  ]);

  // Best-effort low-balance warning. Skip silently if the snapshot endpoint
  // isn't reachable — the reset itself succeeded and that's the headline.
  try {
    const billing = await request(ctx, "/v1/tenants/me/billing", { method: "GET", headers: apiHeaders(ctx) });
    const balance = typeof billing?.balance_cents === "number" ? billing.balance_cents : null;
    if (balance !== null && balance < LOW_BALANCE_THRESHOLD_CENTS) {
      writeLine(ctx.stdout);
      writeLine(ctx.stdout, ui.err(`⚠ Tenant balance is low: ${balance}¢. Top up via the dashboard before the next event.`));
    }
  } catch {
    // ignore — informational warning only.
  }
  return 0;
}
