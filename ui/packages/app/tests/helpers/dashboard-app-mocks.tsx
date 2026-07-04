import React from "react";
import { vi } from "vitest";
import { NANOS_PER_USD } from "@/lib/types";
import { authMock, getTokenFn, resolveActiveWorkspace, usePathname } from "./dashboard-mocks";

// App-specific mock harness for the dashboard-coverage shards. Mirrors
// tests/helpers/dashboard-mocks.tsx: the shared mock-fn instances + the
// module factory bodies live here once; each shard still declares its own
// hoisted `vi.mock(...)` delegating via
// `vi.mock("mod", async () => (await import("./helpers/dashboard-app-mocks")).fooMock())`.
// The dynamic import resolves to the same instance as the shard's static
// import, so a fn a test asserts on is the same instance the factory installs.
// Mocking a module a given shard never imports is inert — over-declaring the
// vi.mock set is safe.

export type ActionResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: string; status?: number };

// ── Shared mock fns ─────────────────────────────────────────────────────────
export const setActiveWorkspaceMock = vi.fn().mockResolvedValue(undefined);
export const createWorkspaceActionMock = vi.fn().mockResolvedValue({ ok: true, data: { workspace_id: "ws_new", name: "fresh-name" } });
export const stopFleetMock = vi.fn();
export const listFleetsMock = vi.fn();
export const getTenantBillingMock = vi.fn();
export const listWorkspaceEventsMock = vi.fn();
export const listFleetEventsMock = vi.fn();
export const listTenantBillingChargesMock = vi.fn();
export const getTenantProviderMock = vi.fn();
export const setTenantProviderSelfManagedMock = vi.fn();
export const resetTenantProviderMock = vi.fn();
export const listSecretsMock = vi.fn();
export const createSecretMock = vi.fn();
export const deleteSecretMock = vi.fn();
export const getModelCapsMock = vi.fn();
export const listWorkspaceFleetTemplatesMock = vi.fn();
export const onboardWorkspaceFleetTemplateMock = vi.fn();

export const setFleetStatusActionMock = vi.fn<
  (ws: string, zid: string, status: string) => Promise<ActionResult<unknown>>
>(async (ws, zid, status) => {
  try {
    return { ok: true, data: await stopFleetMock(ws, zid, status, "tok") };
  } catch (e) {
    const err = e as Error & { status?: number };
    return { ok: false, error: err.message ?? String(e), status: err.status };
  }
});
export const listFleetsActionMock = vi.fn<
  (ws: string, opts?: unknown) => Promise<ActionResult<unknown>>
>(async (ws, opts) => {
  try {
    return { ok: true, data: await listFleetsMock(ws, "tok", opts) };
  } catch (e) {
    return { ok: false, error: (e as Error).message ?? String(e) };
  }
});
export const deleteFleetActionMock = vi.fn<() => Promise<ActionResult<void>>>(
  async () => ({ ok: true, data: undefined }),
);
export const installFleetActionMock = vi.fn<
  () => Promise<ActionResult<{ fleet_id: string }>>
>(async () => ({ ok: true, data: { fleet_id: "z_test" } }));

// ── Module factories (delegated to from each shard's vi.mock call) ───────────
export function fleetsApiMock() {
  return {
    listFleets: listFleetsMock,
    setFleetStatus: stopFleetMock,
    stopFleet: (ws: string, id: string, tok: string) => stopFleetMock(ws, id, "stopped", tok),
    resumeFleet: (ws: string, id: string, tok: string) => stopFleetMock(ws, id, "active", tok),
    killFleet: (ws: string, id: string, tok: string) => stopFleetMock(ws, id, "killed", tok),
    getFleet: vi.fn(),
    installFleet: vi.fn(),
    deleteFleet: vi.fn(),
    AGENTSFLEET_STATUS: { ACTIVE: "active", PAUSED: "paused", STOPPED: "stopped", KILLED: "killed", INSTALLING: "installing" },
  };
}

export function fleetActionsMock() {
  return {
    setFleetStatusAction: setFleetStatusActionMock,
    listFleetsAction: listFleetsActionMock,
    deleteFleetAction: deleteFleetActionMock,
    installFleetAction: installFleetActionMock,
  };
}

export function tenantBillingMock() {
  return {
    getTenantBilling: getTenantBillingMock,
    // M101: routes read billing through the cache()-wrapped reader; both names
    // resolve to the same mock fn so existing setups on `getTenantBillingMock` apply.
    getTenantBillingCached: getTenantBillingMock,
    listTenantBillingCharges: listTenantBillingChargesMock,
  };
}

export function tenantProviderMock() {
  return {
    getTenantProvider: getTenantProviderMock,
    setTenantProviderSelfManaged: setTenantProviderSelfManagedMock,
    resetTenantProvider: resetTenantProviderMock,
  };
}

export function billingBalanceCardMock() {
  return { default: () => React.createElement("div", { "data-balance-card": "1" }) };
}

export function billingUsageTabMock() {
  return {
    default: ({ initialCharges, initialCursor }: { initialCharges: { id: string }[]; initialCursor: string | null }) =>
      React.createElement("div", { "data-usage-tab": "1", "data-charge-count": initialCharges.length, "data-cursor": initialCursor ?? "" }),
  };
}

export function eventsMock() {
  return { listWorkspaceEvents: listWorkspaceEventsMock, listFleetEvents: listFleetEventsMock };
}

const SECRET_KIND = {
  provider_key: "provider_key",
  custom_endpoint: "custom_endpoint",
  custom_secret: "custom_secret",
} as const;

export function secretsApiMock() {
  // The vault API calls are mocked fns; the kind discriminator + narrowing
  // helpers (read by the Models page + its client children) keep their
  // real behaviour so the full module mock doesn't strip them to undefined.
  type C = { kind?: string };
  return {
    listSecrets: listSecretsMock,
    createSecret: createSecretMock,
    deleteSecret: deleteSecretMock,
    rotateSecret: vi.fn(),
    SECRET_KIND,
    providerKeysOf: (secrets: C[]) => secrets.filter((c) => c.kind === SECRET_KIND.provider_key),
    customEndpointsOf: (secrets: C[]) => secrets.filter((c) => c.kind === SECRET_KIND.custom_endpoint),
    customSecretsOf: (secrets: C[]) => secrets.filter((c) => c.kind === SECRET_KIND.custom_secret),
  };
}

// Mocks the fleet-template gallery client (`@/lib/api/fleet-templates`). Both the
// raw reader and its React cache() wrapper resolve to the same vi.fn so a test
// can drive either entry point through one mock. M103 replaced the legacy
// bundle client (github-import / paste) with this template-only gallery read.
export function fleetTemplatesMock() {
  return {
    listWorkspaceFleetTemplates: listWorkspaceFleetTemplatesMock,
    listWorkspaceFleetTemplatesCached: listWorkspaceFleetTemplatesMock,
    onboardWorkspaceFleetTemplate: onboardWorkspaceFleetTemplateMock,
  };
}

export function modelCapsMock() {
  // getModelCaps is mocked; the pure catalogue helpers (read synchronously by
  // ProviderSwitchList / ActiveModelHero / ProviderModelSelect) keep their real
  // behaviour so a full module mock doesn't strip them to undefined.
  return {
    getModelCaps: getModelCapsMock,
    uniqueModelIds: (models: { id: string }[]) =>
      Array.from(new Map(models.map((m) => [m.id, m])).values()),
    modelsForProvider: (models: { provider: string }[], provider: string) =>
      models.filter((m) => m.provider === provider),
    uniqueProviders: (models: { provider: string }[]) =>
      Array.from(new Set(models.map((m) => m.provider))),
    providerLabel: (provider: string) =>
      ({ anthropic: "Anthropic", openai: "OpenAI", "openai-compatible": "Custom — OpenAI-compatible" })[
        provider
      ] ?? provider,
  };
}

export function addSecretFormMock() {
  return { default: ({ workspaceId }: { workspaceId: string }) => React.createElement("div", { "data-add-secret-form": workspaceId }) };
}

export function secretsListMock() {
  return {
    default: ({ workspaceId, secrets }: { workspaceId: string; secrets: { name: string; created_at: number }[] }) =>
      secrets.length === 0
        ? React.createElement("p", { "data-secrets-empty": workspaceId }, "No secrets stored yet")
        : React.createElement(
            "div",
            { "data-secrets-list": workspaceId },
            ...secrets.map((c) => React.createElement("div", { key: c.name, "data-secret-name": c.name }, c.name)),
          ),
  };
}

export function dashboardActionsMock() {
  return { setActiveWorkspace: setActiveWorkspaceMock, createWorkspaceAction: createWorkspaceActionMock };
}

// Re-apply default return values after `vi.clearAllMocks()` in beforeEach.
// Owns the dashboard auth default (carries userId + sessionClaims, which the
// common resetCommonMocks omits).
export function resetDashboardMocks() {
  usePathname.mockReturnValue("/");
  getTokenFn.mockResolvedValue("token_abc");
  authMock.mockReset();
  authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_abc"), userId: "usr_1", sessionClaims: null });
  resolveActiveWorkspace.mockResolvedValue({ id: "ws_1", name: "Alpha" });
  listFleetsMock.mockResolvedValue({
    items: [
      { id: "zom_1", name: "alpha-bot", status: "active", created_at: "2026-04-22T00:00:00Z" },
      { id: "zom_2", name: "beta-bot", status: "paused", created_at: "2026-04-22T00:00:01Z" },
      { id: "zom_3", name: "gamma-bot", status: "stopped", created_at: "2026-04-22T00:00:02Z" },
    ],
    total: 3,
    cursor: null,
  });
  getTenantBillingMock.mockResolvedValue({ balance_nanos: 5 * NANOS_PER_USD, is_exhausted: false, exhausted_at: null });
  listWorkspaceEventsMock.mockResolvedValue({ items: [], next_cursor: null });
  listFleetEventsMock.mockResolvedValue({ items: [], next_cursor: null });
  getModelCapsMock.mockResolvedValue({
    version: "2026-04-29",
    models: [],
    rates: { run_nanos_per_sec: 0, event_nanos: 0 },
    billing: { starter_credit_nanos: 0, free_trial_end_ms: 0, free_trial_stage_nanos: 0 },
  });
  stopFleetMock.mockResolvedValue(undefined);
  listWorkspaceFleetTemplatesMock.mockResolvedValue({ items: [] });
  onboardWorkspaceFleetTemplateMock.mockResolvedValue({
    id: "tmpl_1",
    name: "Template",
    visibility: "tenant",
    content_hash: "sha256:abc",
    requirements: { credentials: [], tools: [], network_hosts: [], trigger_present: true },
    support_files: [],
  });
}
