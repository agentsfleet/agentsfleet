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
export const stopZombieMock = vi.fn();
export const listZombiesMock = vi.fn();
export const getTenantBillingMock = vi.fn();
export const listWorkspaceEventsMock = vi.fn();
export const listZombieEventsMock = vi.fn();
export const listTenantBillingChargesMock = vi.fn();
export const getTenantProviderMock = vi.fn();
export const setTenantProviderSelfManagedMock = vi.fn();
export const resetTenantProviderMock = vi.fn();
export const listCredentialsMock = vi.fn();
export const createCredentialMock = vi.fn();
export const deleteCredentialMock = vi.fn();
export const getModelCapsMock = vi.fn();

export const setZombieStatusActionMock = vi.fn<
  (ws: string, zid: string, status: string) => Promise<ActionResult<unknown>>
>(async (ws, zid, status) => {
  try {
    return { ok: true, data: await stopZombieMock(ws, zid, status, "tok") };
  } catch (e) {
    const err = e as Error & { status?: number };
    return { ok: false, error: err.message ?? String(e), status: err.status };
  }
});
export const listZombiesActionMock = vi.fn<
  (ws: string, opts?: unknown) => Promise<ActionResult<unknown>>
>(async (ws, opts) => {
  try {
    return { ok: true, data: await listZombiesMock(ws, "tok", opts) };
  } catch (e) {
    return { ok: false, error: (e as Error).message ?? String(e) };
  }
});
export const deleteZombieActionMock = vi.fn<() => Promise<ActionResult<void>>>(
  async () => ({ ok: true, data: undefined }),
);
export const installZombieActionMock = vi.fn<
  () => Promise<ActionResult<{ zombie_id: string }>>
>(async () => ({ ok: true, data: { zombie_id: "z_test" } }));

// ── Module factories (delegated to from each shard's vi.mock call) ───────────
export function zombiesApiMock() {
  return {
    listZombies: listZombiesMock,
    setZombieStatus: stopZombieMock,
    stopZombie: (ws: string, id: string, tok: string) => stopZombieMock(ws, id, "stopped", tok),
    resumeZombie: (ws: string, id: string, tok: string) => stopZombieMock(ws, id, "active", tok),
    killZombie: (ws: string, id: string, tok: string) => stopZombieMock(ws, id, "killed", tok),
    getZombie: vi.fn(),
    installZombie: vi.fn(),
    deleteZombie: vi.fn(),
    AGENTSFLEET_STATUS: { ACTIVE: "active", PAUSED: "paused", STOPPED: "stopped", KILLED: "killed" },
  };
}

export function zombieActionsMock() {
  return {
    setZombieStatusAction: setZombieStatusActionMock,
    listZombiesAction: listZombiesActionMock,
    deleteZombieAction: deleteZombieActionMock,
    installZombieAction: installZombieActionMock,
  };
}

export function tenantBillingMock() {
  return { getTenantBilling: getTenantBillingMock, listTenantBillingCharges: listTenantBillingChargesMock };
}

export function tenantProviderMock() {
  return {
    getTenantProvider: getTenantProviderMock,
    setTenantProviderSelfManaged: setTenantProviderSelfManagedMock,
    resetTenantProvider: resetTenantProviderMock,
  };
}

export function providerSelectorMock() {
  return { default: ({ workspaceId }: { workspaceId: string }) => React.createElement("div", { "data-provider-selector": workspaceId }) };
}

export function billingBalanceCardMock() {
  return { default: () => React.createElement("div", { "data-balance-card": "1" }) };
}

export function billingUsageTabMock() {
  return {
    default: ({ initialEvents, initialCursor }: { initialEvents: { event_id: string }[]; initialCursor: string | null }) =>
      React.createElement("div", { "data-usage-tab": "1", "data-event-count": initialEvents.length, "data-cursor": initialCursor ?? "" }),
  };
}

export function eventsMock() {
  return { listWorkspaceEvents: listWorkspaceEventsMock, listZombieEvents: listZombieEventsMock };
}

export function credentialsApiMock() {
  return { listCredentials: listCredentialsMock, createCredential: createCredentialMock, deleteCredential: deleteCredentialMock };
}

export function modelCapsMock() {
  return { getModelCaps: getModelCapsMock };
}

export function addCredentialFormMock() {
  return { default: ({ workspaceId }: { workspaceId: string }) => React.createElement("div", { "data-add-credential-form": workspaceId }) };
}

export function credentialsListMock() {
  return {
    default: ({ workspaceId, credentials }: { workspaceId: string; credentials: { name: string; created_at: string }[] }) =>
      credentials.length === 0
        ? React.createElement("p", { "data-credentials-empty": workspaceId }, "No credentials stored yet")
        : React.createElement(
            "div",
            { "data-credentials-list": workspaceId },
            ...credentials.map((c) => React.createElement("div", { key: c.name, "data-credential-name": c.name }, c.name)),
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
  listZombiesMock.mockResolvedValue({
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
  listZombieEventsMock.mockResolvedValue({ items: [], next_cursor: null });
  getModelCapsMock.mockResolvedValue({
    version: "2026-04-29",
    models: [],
    rates: { run_nanos_per_sec: 0, event_nanos: 0 },
    billing: { starter_credit_nanos: 0, free_trial_end_ms: 0, free_trial_stage_nanos: 0 },
  });
  stopZombieMock.mockResolvedValue(undefined);
}
