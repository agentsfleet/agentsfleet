import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { CHARGE_TYPE, PROVIDER_MODE } from "@/lib/types";
import { mockAuthOnce as mockAuth, resolveActiveWorkspace as resolveActiveWorkspaceMock } from "./helpers/dashboard-mocks";
import {
  resetDashboardMocks,
  listCredentialsMock,
  listWorkspaceEventsMock,
  getTenantProviderMock,
  getTenantBillingMock,
  listTenantBillingChargesMock,
} from "./helpers/dashboard-app-mocks";

// Common dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@clerk/nextjs", async () => (await import("./helpers/dashboard-mocks")).clerkMock());
vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("@/lib/workspace", async () => (await import("./helpers/dashboard-mocks")).workspaceMock());
vi.mock("lucide-react", async () => (await import("./helpers/dashboard-mocks")).lucideMock());
vi.mock("@usezombie/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return { ...h.designSystemCore(await orig<Record<string, unknown>>()), ...h.designSystemDropdown() };
});

// App-specific dashboard mocks — see tests/helpers/dashboard-app-mocks.tsx.
vi.mock("@/lib/api/zombies", async () => (await import("./helpers/dashboard-app-mocks")).zombiesApiMock());
vi.mock("@/app/(dashboard)/zombies/actions", async () => (await import("./helpers/dashboard-app-mocks")).zombieActionsMock());
vi.mock("@/lib/api/tenant_billing", async () => (await import("./helpers/dashboard-app-mocks")).tenantBillingMock());
vi.mock("@/lib/api/tenant_provider", async () => (await import("./helpers/dashboard-app-mocks")).tenantProviderMock());
vi.mock("@/app/(dashboard)/settings/models/components/ProviderSelector", async () => (await import("./helpers/dashboard-app-mocks")).providerSelectorMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingBalanceCard", async () => (await import("./helpers/dashboard-app-mocks")).billingBalanceCardMock());
vi.mock("@/app/(dashboard)/settings/billing/components/BillingUsageTab", async () => (await import("./helpers/dashboard-app-mocks")).billingUsageTabMock());
vi.mock("@/lib/api/events", async () => (await import("./helpers/dashboard-app-mocks")).eventsMock());
vi.mock("@/lib/api/credentials", async () => (await import("./helpers/dashboard-app-mocks")).credentialsApiMock());
vi.mock("@/app/(dashboard)/credentials/components/AddCredentialForm", async () => (await import("./helpers/dashboard-app-mocks")).addCredentialFormMock());
vi.mock("@/app/(dashboard)/credentials/components/CredentialsList", async () => (await import("./helpers/dashboard-app-mocks")).credentialsListMock());
vi.mock("@/app/(dashboard)/actions", async () => (await import("./helpers/dashboard-app-mocks")).dashboardActionsMock());

beforeEach(() => {
  vi.clearAllMocks();
  resetDashboardMocks();
});
afterEach(() => {
  cleanup();
});

describe("placeholder pages", () => {
  it("credentials page renders list + add form when workspace + credentials are present", async () => {
    mockAuth({ token: "token_abc" });
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_1", name: "Default" });
    listCredentialsMock.mockResolvedValue({
      credentials: [
        { name: "fly", created_at: "2026-04-26T00:00:00Z" },
        { name: "slack", created_at: "2026-04-26T00:00:01Z" },
      ],
    });
    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Credentials");
    expect(m).toContain("Stored credentials");
    expect(m).toContain("Add a credential");
    expect(m).toContain("fly");
    expect(m).toContain("slack");
  });

  it("credentials page falls back to empty list when API errors", async () => {
    mockAuth({ token: "token_abc" });
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_1", name: "Default" });
    listCredentialsMock.mockRejectedValue(new Error("boom"));
    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("No credentials stored yet");
  });

  it("settings page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null, userId: null });
    const { default: Page } = await import("../app/(dashboard)/settings/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("events page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/events/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("events page calls notFound when no active workspace", async () => {
    mockAuth({ token: "token_abc" });
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/events/page");
    await expect(Page()).rejects.toThrow("notFound");
  });

  it("events page renders Workspace events section with EventsList", async () => {
    mockAuth({ token: "token_abc" });
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_1", name: "Default" });
    listWorkspaceEventsMock.mockResolvedValue({ items: [], next_cursor: null });
    const { default: Page } = await import("../app/(dashboard)/events/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Events");
    expect(m).toContain("Workspace events");
  });

  it("events page falls back to empty page when listWorkspaceEvents errors", async () => {
    mockAuth({ token: "token_abc" });
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_1", name: "Default" });
    listWorkspaceEventsMock.mockRejectedValue(new Error("boom"));
    const { default: Page } = await import("../app/(dashboard)/events/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Workspace events");
  });

  it("settings page renders workspace info and userId when authenticated", async () => {
    mockAuth({ token: "tkn", userId: "usr_42" });
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_xyz", name: "Production" });
    const { default: Page } = await import("../app/(dashboard)/settings/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Settings");
    expect(m).toContain("Production");
    expect(m).toContain("ws_xyz");
    expect(m).toContain("usr_42");
  });

  it("settings page tolerates missing active workspace", async () => {
    mockAuth({ token: "tkn", userId: "usr_42" });
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/settings/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Settings");
    expect(m).toContain("—");
  });

  it("provider settings page renders selector with current config and empty credentials", async () => {
    mockAuth({ token: "token_provider" });
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_p", name: "P" });
    getTenantProviderMock.mockResolvedValue({
      mode: PROVIDER_MODE.platform,
      provider: "fireworks",
      model: "kimi-k2.6",
      context_cap_tokens: 256000,
      credential_ref: null,
    });
    listCredentialsMock.mockResolvedValue({ credentials: [] });
    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Model");
    expect(m).toContain(PROVIDER_MODE.platform);
    expect(m).toContain("data-provider-selector=\"ws_p\"");
  });

  it("provider settings page renders empty-workspace empty-state when no workspace", async () => {
    mockAuth({ token: "token_provider" });
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("No workspace yet");
  });

  it("provider settings page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("provider settings page tolerates a getTenantProvider 5xx (catch fallback)", async () => {
    mockAuth({ token: "token_provider" });
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_p", name: "P" });
    getTenantProviderMock.mockRejectedValue(new Error("503"));
    listCredentialsMock.mockResolvedValue({ credentials: [] });
    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    // The page swallows the error to keep rendering.
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Model");
  });

  it("billing settings page renders balance card + usage tab + invoice/payment empty states", async () => {
    mockAuth({ token: "token_billing" });
    getTenantBillingMock.mockResolvedValue({
      balance_nanos: 4_710_000_000,
      updated_at: 1, is_exhausted: false, exhausted_at: null,
    });
    listTenantBillingChargesMock.mockResolvedValue({
      items: [
        {
          id: "tel_1", tenant_id: "t", workspace_id: "w", zombie_id: "z",
          event_id: "evt_1", charge_type: CHARGE_TYPE.receive, posture: PROVIDER_MODE.platform,
          model: "kimi-k2.6", credit_deducted_nanos: 1,
          token_count_input: null, token_count_output: null, wall_ms: null, recorded_at: 1,
        },
      ],
    });
    const { default: Page } = await import("../app/(dashboard)/settings/billing/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Billing");
    expect(m).toContain("data-balance-card=\"1\"");
    expect(m).toContain("data-usage-tab=\"1\"");
    // Radix Tabs only renders the active panel; assert the tab triggers
    // are wired so Invoices / Payment Method are reachable on click.
    expect(m).toContain(">Invoices</button>");
    expect(m).toContain(">Payment Method</button>");
  });

  it("billing settings page tolerates a /charges 5xx by falling back to empty events", async () => {
    mockAuth({ token: "token_billing" });
    getTenantBillingMock.mockResolvedValue({
      balance_nanos: 0,
      updated_at: 1, is_exhausted: true, exhausted_at: 2,
    });
    listTenantBillingChargesMock.mockRejectedValue(new Error("503"));
    const { default: Page } = await import("../app/(dashboard)/settings/billing/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("data-event-count=\"0\"");
  });

  it("billing settings page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/settings/billing/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("billing settings page shows the not-ready empty state when billing is null", async () => {
    mockAuth({ token: "token_billing" });
    getTenantBillingMock.mockResolvedValue(null);
    listTenantBillingChargesMock.mockResolvedValue({ items: [], next_cursor: null });
    const { default: Page } = await import("../app/(dashboard)/settings/billing/page");
    const m = renderToStaticMarkup(await Page());
    // renderToStaticMarkup escapes the apostrophe in "isn't"; assert on a
    // stable substring of the not-ready empty state instead.
    expect(m).toContain("ready yet");
    expect(m).toContain("still being set up");
  });

  it("credentials page redirects to /sign-in when no token", async () => {
    mockAuth({ token: null });
    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("credentials page shows the no-workspace empty state", async () => {
    mockAuth({ token: "token_abc" });
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("No workspace yet");
  });

  it("settings page renders an em-dash when the userId is missing", async () => {
    mockAuth({ token: "tkn", userId: null });
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_xyz", name: "Production" });
    const { default: Page } = await import("../app/(dashboard)/settings/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("ws_xyz");
    expect(m).toContain("—"); // userId ?? "—"
  });
});
