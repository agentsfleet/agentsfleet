import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderToStaticMarkup } from "react-dom/server";
import { CHARGE_TYPE, NANOS_PER_USD, PROVIDER_MODE } from "@/lib/types";

// ── Shared mocks ───────────────────────────────────────────────────────────

const routerRefresh = vi.fn();
const routerPush = vi.fn();
const notFound = vi.fn(() => {
  throw new Error("notFound");
});
const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const setActiveWorkspaceMock = vi.fn().mockResolvedValue(undefined);
const createWorkspaceActionMock = vi.fn().mockResolvedValue({ ok: true, data: { workspace_id: "ws_new", name: "fresh-name" } });
const getTokenFn = vi.fn().mockResolvedValue("token_abc");
const stopZombieMock = vi.fn();
const listZombiesMock = vi.fn();
const getTenantBillingMock = vi.fn();
const listWorkspaceEventsMock = vi.fn();
const listZombieEventsMock = vi.fn();
const resolveActiveWorkspaceMock = vi.fn();
const getServerTokenMock = vi.fn();
const getServerAuthMock = vi.fn();

vi.mock("next/navigation", () => ({
  notFound,
  redirect,
  usePathname: vi.fn(() => "/"),
  useRouter: () => ({ push: routerPush, refresh: routerRefresh }),
}));

vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href, ...rest }, children),
}));

vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ getToken: getTokenFn }),
  useUser: () => ({ isLoaded: true, isSignedIn: true, user: null }),
  ClerkProvider: ({ children }: { children: React.ReactNode }) =>
    React.createElement(React.Fragment, null, children),
  UserButton: () => React.createElement("div", { "data-user-button": "1" }),
  SignIn: () => React.createElement("div", { "data-sign-in": "1" }),
  SignUp: () => React.createElement("div", { "data-sign-up": "1" }),
}));

vi.mock("@clerk/nextjs/server", () => ({
  auth: vi.fn().mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_abc"), userId: "usr_1" }),
}));

vi.mock("@/lib/auth/server", () => ({
  getServerToken: getServerTokenMock,
  getServerAuth: getServerAuthMock,
  getServerSessionMetadata: vi.fn(),
}));

vi.mock("@/lib/workspace", () => ({
  resolveActiveWorkspace: resolveActiveWorkspaceMock,
  listTenantWorkspacesCached: vi.fn().mockResolvedValue({ items: [], total: 0 }),
}));

vi.mock("@/lib/api/zombies", () => ({
  listZombies: listZombiesMock,
  setZombieStatus: stopZombieMock,
  stopZombie: (ws: string, id: string, tok: string) => stopZombieMock(ws, id, "stopped", tok),
  resumeZombie: (ws: string, id: string, tok: string) => stopZombieMock(ws, id, "active", tok),
  killZombie: (ws: string, id: string, tok: string) => stopZombieMock(ws, id, "killed", tok),
  getZombie: vi.fn(),
  installZombie: vi.fn(),
  deleteZombie: vi.fn(),
  ZOMBIE_STATUS: {
    ACTIVE: "active",
    PAUSED: "paused",
    STOPPED: "stopped",
    KILLED: "killed",
    ERRORED: "errored",
  },
}));

// The dashboard's mutation surface now flows through Server Actions instead
// of useClientToken. The actions internally call getServerToken + the api
// helpers above; in unit tests we stub them with thin wrappers that mirror
// the ActionResult contract while still reusing the api-layer mocks above.
type ActionResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: string; status?: number };

const setZombieStatusActionMock = vi.fn<
  (ws: string, zid: string, status: string) => Promise<ActionResult<unknown>>
>(async (ws, zid, status) => {
  try {
    return { ok: true, data: await stopZombieMock(ws, zid, status, "tok") };
  } catch (e) {
    const err = e as Error & { status?: number };
    return { ok: false, error: err.message ?? String(e), status: err.status };
  }
});
const listZombiesActionMock = vi.fn<
  (ws: string, opts?: unknown) => Promise<ActionResult<unknown>>
>(async (ws, opts) => {
  try {
    return { ok: true, data: await listZombiesMock(ws, "tok", opts) };
  } catch (e) {
    return { ok: false, error: (e as Error).message ?? String(e) };
  }
});
const deleteZombieActionMock = vi.fn<() => Promise<ActionResult<void>>>(
  async () => ({ ok: true, data: undefined }),
);
const installZombieActionMock = vi.fn<
  () => Promise<ActionResult<{ zombie_id: string }>>
>(async () => ({ ok: true, data: { zombie_id: "z_test" } }));

vi.mock("@/app/(dashboard)/zombies/actions", () => ({
  setZombieStatusAction: setZombieStatusActionMock,
  listZombiesAction: listZombiesActionMock,
  deleteZombieAction: deleteZombieActionMock,
  installZombieAction: installZombieActionMock,
}));

const listTenantBillingChargesMock = vi.fn();
vi.mock("@/lib/api/tenant_billing", () => ({
  getTenantBilling: getTenantBillingMock,
  listTenantBillingCharges: listTenantBillingChargesMock,
}));

const getTenantProviderMock = vi.fn();
const setTenantProviderSelfManagedMock = vi.fn();
const resetTenantProviderMock = vi.fn();
vi.mock("@/lib/api/tenant_provider", () => ({
  getTenantProvider: getTenantProviderMock,
  setTenantProviderSelfManaged: setTenantProviderSelfManagedMock,
  resetTenantProvider: resetTenantProviderMock,
}));

vi.mock("@/app/(dashboard)/settings/provider/components/ProviderSelector", () => ({
  default: ({ workspaceId }: { workspaceId: string }) =>
    React.createElement("div", { "data-provider-selector": workspaceId }),
}));
vi.mock("@/app/(dashboard)/settings/billing/components/BillingBalanceCard", () => ({
  default: () => React.createElement("div", { "data-balance-card": "1" }),
}));
vi.mock("@/app/(dashboard)/settings/billing/components/BillingUsageTab", () => ({
  default: ({ initialEvents, initialCursor }: { initialEvents: { event_id: string }[]; initialCursor: string | null }) =>
    React.createElement("div", {
      "data-usage-tab": "1",
      "data-event-count": initialEvents.length,
      "data-cursor": initialCursor ?? "",
    }),
}));

vi.mock("@/lib/api/events", () => ({
  listWorkspaceEvents: listWorkspaceEventsMock,
  listZombieEvents: listZombieEventsMock,
}));

const listCredentialsMock = vi.fn();
const createCredentialMock = vi.fn();
const deleteCredentialMock = vi.fn();
vi.mock("@/lib/api/credentials", () => ({
  listCredentials: listCredentialsMock,
  createCredential: createCredentialMock,
  deleteCredential: deleteCredentialMock,
}));

// The credentials page composes two client components. The structural
// dashboard-coverage test only cares that the page wraps them with the
// right header/section labels — each child component carries its own
// tests for behaviour. Stub them here so the static-markup pass does not
// drag in react-hook-form or radix client-only providers.
vi.mock("@/app/(dashboard)/credentials/components/AddCredentialForm", () => ({
  default: ({ workspaceId }: { workspaceId: string }) =>
    React.createElement("div", { "data-add-credential-form": workspaceId }),
}));

vi.mock("@/app/(dashboard)/credentials/components/CredentialsList", () => ({
  default: ({
    workspaceId,
    credentials,
  }: {
    workspaceId: string;
    credentials: { name: string; created_at: string }[];
  }) =>
    credentials.length === 0
      ? React.createElement(
          "p",
          { "data-credentials-empty": workspaceId },
          "No credentials stored yet",
        )
      : React.createElement(
          "div",
          { "data-credentials-list": workspaceId },
          ...credentials.map((c) =>
            React.createElement("div", { key: c.name, "data-credential-name": c.name }, c.name),
          ),
        ),
}));

vi.mock("@/app/(dashboard)/actions", () => ({
  setActiveWorkspace: setActiveWorkspaceMock,
  createWorkspaceAction: createWorkspaceActionMock,
}));

vi.mock("lucide-react", () => {
  const make = (name: string) => {
    const C = (p: Record<string, unknown>) =>
      React.createElement("svg", { ...p, "data-icon": name });
    C.displayName = name;
    return C;
  };
  return {
    ChevronDownIcon: make("ChevronDownIcon"),
    ChevronRightIcon: make("ChevronRightIcon"),
    KeyRoundIcon: make("KeyRoundIcon"),
    ShieldIcon: make("ShieldIcon"),
    SettingsIcon: make("SettingsIcon"),
    PlusIcon: make("PlusIcon"),
    Loader2Icon: make("Loader2Icon"),
    Trash2Icon: make("Trash2Icon"),
    WalletIcon: make("WalletIcon"),
    ZapIcon: make("ZapIcon"),
    ReceiptIcon: make("ReceiptIcon"),
    CreditCardIcon: make("CreditCardIcon"),
    ActivityIcon: make("ActivityIcon"),
    AlertTriangleIcon: make("AlertTriangleIcon"),
  };
});

vi.mock("@usezombie/design-system", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@usezombie/design-system")>();
  return {
    ...actual,
    Button: ({
      children,
      ...rest
    }: React.ButtonHTMLAttributes<HTMLButtonElement> & { children: React.ReactNode }) =>
      React.createElement("button", { ...rest }, children),
    Input: (props: React.InputHTMLAttributes<HTMLInputElement>) =>
      React.createElement("input", props),
    Skeleton: ({ className }: { className?: string }) =>
      React.createElement("div", { "data-skeleton": "1", className }),
    StatusCard: ({ label, count, variant }: { label: string; count: number | string; variant?: string }) =>
      React.createElement(
        "div",
        { "data-status-card": label, "data-variant": variant ?? "default" },
        `${label}: ${count}`,
      ),
    PageHeader: ({ children }: { children: React.ReactNode }) =>
      React.createElement("header", null, children),
    PageTitle: ({ children }: { children: React.ReactNode }) =>
      React.createElement("h1", null, children),
    SectionLabel: ({ children }: { children: React.ReactNode }) =>
      React.createElement("h2", null, children),
    EmptyState: ({ title, description }: { title: string; description?: string }) =>
      React.createElement("div", { "data-empty-state": title }, description),
    ConfirmDialog: ({
      open,
      onOpenChange,
      title,
      onConfirm,
      onError,
      errorMessage,
      confirmLabel = "Confirm",
      cancelLabel = "Cancel",
    }: {
      open: boolean;
      onOpenChange: (v: boolean) => void;
      title: string;
      onConfirm: () => void | Promise<void>;
      onError?: (e: unknown) => void;
      errorMessage?: string | null;
      confirmLabel?: string;
      cancelLabel?: string;
    }) =>
      open
        ? React.createElement(
            "div",
            { role: "alertdialog", "data-confirm": title },
            React.createElement("div", { key: "title" }, title),
            errorMessage
              ? React.createElement("div", { key: "err", role: "alert" }, errorMessage)
              : null,
            React.createElement(
              "button",
              { key: "c", type: "button", onClick: () => onOpenChange(false) },
              cancelLabel,
            ),
            React.createElement(
              "button",
              {
                key: "ok",
                type: "button",
                onClick: async () => {
                  try {
                    await onConfirm();
                  } catch (e) {
                    if (onError) onError(e);
                  }
                },
              },
              confirmLabel,
            ),
          )
        : null,
    DropdownMenu: ({ children }: { children: React.ReactNode }) =>
      React.createElement("div", { "data-dropdown": "1" }, children),
    DropdownMenuTrigger: ({
      children,
      ...rest
    }: React.ButtonHTMLAttributes<HTMLButtonElement> & { children: React.ReactNode }) =>
      React.createElement("button", { ...rest }, children),
    DropdownMenuContent: ({ children }: { children: React.ReactNode }) =>
      React.createElement("div", { "data-dropdown-content": "1" }, children),
    DropdownMenuLabel: ({ children }: { children: React.ReactNode }) =>
      React.createElement("div", { "data-dropdown-label": "1" }, children),
    DropdownMenuSeparator: () =>
      React.createElement("hr", { "data-dropdown-separator": "1" }),
    DropdownMenuItem: ({
      children,
      onSelect,
      ...rest
    }: {
      children: React.ReactNode;
      onSelect?: () => void;
    } & React.HTMLAttributes<HTMLDivElement>) =>
      React.createElement(
        "div",
        {
          role: "menuitem",
          onClick: () => onSelect?.(),
          ...rest,
        },
        children,
      ),
  };
});

beforeEach(() => {
  vi.clearAllMocks();
  getServerTokenMock.mockResolvedValue("token_abc");
  getServerAuthMock.mockResolvedValue({ token: "token_abc", userId: "usr_1" });
  resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_1", name: "Alpha" });
  listZombiesMock.mockResolvedValue({
    items: [
      { id: "zom_1", name: "alpha-bot", status: "active", created_at: "2026-04-22T00:00:00Z" },
      { id: "zom_2", name: "beta-bot", status: "paused", created_at: "2026-04-22T00:00:01Z" },
      { id: "zom_3", name: "gamma-bot", status: "stopped", created_at: "2026-04-22T00:00:02Z" },
    ],
    total: 3,
    cursor: null,
  });
  getTenantBillingMock.mockResolvedValue({
    balance_nanos: 5 * NANOS_PER_USD,
    is_exhausted: false,
    exhausted_at: null,
  });
  listWorkspaceEventsMock.mockResolvedValue({ items: [], next_cursor: null });
  listZombieEventsMock.mockResolvedValue({ items: [], next_cursor: null });
  getTokenFn.mockResolvedValue("token_abc");
  stopZombieMock.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
});

// ── Placeholder pages ──────────────────────────────────────────────────────

describe("placeholder pages", () => {
  it("credentials page renders list + add form when workspace + credentials are present", async () => {
    getServerTokenMock.mockResolvedValue("token_abc");
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
    getServerTokenMock.mockResolvedValue("token_abc");
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_1", name: "Default" });
    listCredentialsMock.mockRejectedValue(new Error("boom"));
    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("No credentials stored yet");
  });

  it("settings page redirects to /sign-in when no token", async () => {
    getServerAuthMock.mockResolvedValue({ token: null, userId: null });
    const { default: Page } = await import("../app/(dashboard)/settings/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("events page redirects to /sign-in when no token", async () => {
    getServerTokenMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/events/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("events page calls notFound when no active workspace", async () => {
    getServerTokenMock.mockResolvedValue("token_abc");
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/events/page");
    await expect(Page()).rejects.toThrow("notFound");
  });

  it("events page renders Workspace events section with EventsList", async () => {
    getServerTokenMock.mockResolvedValue("token_abc");
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_1", name: "Default" });
    listWorkspaceEventsMock.mockResolvedValue({ items: [], next_cursor: null });
    const { default: Page } = await import("../app/(dashboard)/events/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Events");
    expect(m).toContain("Workspace events");
  });

  it("events page falls back to empty page when listWorkspaceEvents errors", async () => {
    getServerTokenMock.mockResolvedValue("token_abc");
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_1", name: "Default" });
    listWorkspaceEventsMock.mockRejectedValue(new Error("boom"));
    const { default: Page } = await import("../app/(dashboard)/events/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Workspace events");
  });

  it("settings page renders workspace info and userId when authenticated", async () => {
    getServerAuthMock.mockResolvedValue({ token: "tkn", userId: "usr_42" });
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_xyz", name: "Production" });
    const { default: Page } = await import("../app/(dashboard)/settings/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Settings");
    expect(m).toContain("Production");
    expect(m).toContain("ws_xyz");
    expect(m).toContain("usr_42");
  });

  it("settings page tolerates missing active workspace", async () => {
    getServerAuthMock.mockResolvedValue({ token: "tkn", userId: "usr_42" });
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/settings/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Settings");
    expect(m).toContain("—");
  });

  it("provider settings page renders selector with current config and empty credentials", async () => {
    getServerTokenMock.mockResolvedValue("token_provider");
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_p", name: "P" });
    getTenantProviderMock.mockResolvedValue({
      mode: PROVIDER_MODE.platform,
      provider: "fireworks",
      model: "kimi-k2.6",
      context_cap_tokens: 256000,
      credential_ref: null,
      synthesised_default: true,
    });
    listCredentialsMock.mockResolvedValue({ credentials: [] });
    const { default: Page } = await import("../app/(dashboard)/settings/provider/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("LLM Provider");
    expect(m).toContain(PROVIDER_MODE.platform);
    expect(m).toContain("data-provider-selector=\"ws_p\"");
    expect(m).toContain("This is the platform default");
  });

  it("provider settings page surfaces resolver error banner from the API", async () => {
    getServerTokenMock.mockResolvedValue("token_provider");
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_p", name: "P" });
    getTenantProviderMock.mockResolvedValue({
      mode: PROVIDER_MODE.self_managed,
      provider: "fireworks",
      model: "kimi-k2.6",
      context_cap_tokens: 256000,
      credential_ref: "fw-key",
      error: "credential_missing",
    });
    listCredentialsMock.mockResolvedValue({ credentials: [{ name: "fw-key", created_at: "2026-04-01T00:00:00Z" }] });
    const { default: Page } = await import("../app/(dashboard)/settings/provider/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Provider resolver error");
    expect(m).toContain("credential_missing");
  });

  it("provider settings page renders empty-workspace empty-state when no workspace", async () => {
    getServerTokenMock.mockResolvedValue("token_provider");
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/settings/provider/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("No workspace yet");
  });

  it("provider settings page redirects to /sign-in when no token", async () => {
    getServerTokenMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/settings/provider/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("provider settings page tolerates a getTenantProvider 5xx (catch fallback)", async () => {
    getServerTokenMock.mockResolvedValue("token_provider");
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_p", name: "P" });
    getTenantProviderMock.mockRejectedValue(new Error("503"));
    listCredentialsMock.mockResolvedValue({ credentials: [] });
    const { default: Page } = await import("../app/(dashboard)/settings/provider/page");
    // The page swallows the error to keep rendering.
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("LLM Provider");
  });

  it("billing settings page renders balance card + usage tab + invoice/payment empty states", async () => {
    getServerTokenMock.mockResolvedValue("token_billing");
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
    getServerTokenMock.mockResolvedValue("token_billing");
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
    getServerTokenMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/settings/billing/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("billing settings page shows the not-ready empty state when billing is null", async () => {
    getServerTokenMock.mockResolvedValue("token_billing");
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
    getServerTokenMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("credentials page shows the no-workspace empty state", async () => {
    getServerTokenMock.mockResolvedValue("token_abc");
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("No workspace yet");
  });

  it("settings page renders an em-dash when the userId is missing", async () => {
    getServerAuthMock.mockResolvedValue({ token: "tkn", userId: null });
    resolveActiveWorkspaceMock.mockResolvedValue({ id: "ws_xyz", name: "Production" });
    const { default: Page } = await import("../app/(dashboard)/settings/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("ws_xyz");
    expect(m).toContain("—"); // userId ?? "—"
  });
});

// ── Dashboard page (StatusTiles + RecentActivity) ──────────────────────────

describe("dashboard overview page", () => {
  it("redirects to /sign-in when no server token", async () => {
    getServerTokenMock.mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("renders page header with Suspense fallbacks when authenticated", async () => {
    const { default: Page } = await import("../app/(dashboard)/page");
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Dashboard");
    expect(m).toContain("data-skeleton");
  });

  it("StatusTiles returns null when no token or no workspace", async () => {
    const mod = await import("../app/(dashboard)/page");
    const Page = mod.default;
    getServerTokenMock.mockResolvedValue(null);
    await expect(Page()).rejects.toThrow("redirect:/sign-in");

    getServerTokenMock.mockResolvedValue("token_abc");
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    const m = renderToStaticMarkup(await Page());
    expect(m).toContain("Dashboard");
  });

  it("StatusTiles renders Live/Paused/Stopped tiles + balance from the zombie list", async () => {
    const { StatusTiles } = await import("../app/(dashboard)/page");
    // beforeEach seeds 1 active / 1 paused / 1 stopped; an exhausted balance
    // exercises the `is_exhausted ? "danger"` truthy arm + `active > 0` sublabel.
    getTenantBillingMock.mockResolvedValue({
      balance_nanos: 5 * NANOS_PER_USD,
      is_exhausted: true,
      exhausted_at: 1,
    });
    const m = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles()));
    expect(m).toContain("Live");
    expect(m).toContain("Paused");
    expect(m).toContain("Stopped");
    expect(m).toContain("$5.00"); // billing present → formatted-balance branch

    // No active zombies → the sublabel ternary takes its undefined arm while
    // the grid still renders.
    listZombiesMock.mockResolvedValue({
      items: [{ id: "z", name: "n", status: "stopped", created_at: "2026-04-22T00:00:00Z" }],
      total: 1,
      cursor: null,
    });
    getTenantBillingMock.mockResolvedValue(null); // billing null + zombies present → Balance "—"
    const m2 = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles()));
    expect(m2).toContain("Stopped");
    expect(m2).toContain("—"); // billing ? ... : "—" false arm
  });

  it("StatusTiles shows the first-install free-credit card when there are no zombies", async () => {
    listZombiesMock.mockResolvedValue({ items: [], total: 0, cursor: null });
    getTenantBillingMock.mockResolvedValue({
      balance_nanos: 5 * NANOS_PER_USD,
      is_exhausted: false,
      exhausted_at: null,
    });
    const { StatusTiles } = await import("../app/(dashboard)/page");
    const m = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles()));
    expect(m).toContain("First wake");
    expect(m).toContain("free credit"); // credits > 0 copy branch
  });

  it("StatusTiles first-install copy degrades to the terminal prompt when balance is unknown", async () => {
    listZombiesMock.mockResolvedValue({ items: [], total: 0, cursor: null });
    getTenantBillingMock.mockResolvedValue(null); // balance null → credits-null branch
    const { StatusTiles } = await import("../app/(dashboard)/page");
    const m = renderToStaticMarkup(React.createElement(React.Fragment, null, await StatusTiles()));
    expect(m).toContain("First wake");
    expect(m).toContain("Install a zombie from your terminal");
  });

  it("StatusTiles returns null without a token or an active workspace", async () => {
    const { StatusTiles } = await import("../app/(dashboard)/page");
    getServerTokenMock.mockResolvedValue(null);
    expect(await StatusTiles()).toBeNull();
    getServerTokenMock.mockResolvedValue("token_abc");
    resolveActiveWorkspaceMock.mockResolvedValue(null);
    expect(await StatusTiles()).toBeNull();
  });
});

// ── KillSwitch ─────────────────────────────────────────────────────────────

describe("KillSwitch component", () => {
  async function renderSwitch(status: string = "active") {
    const { default: KillSwitch } = await import(
      "../app/(dashboard)/zombies/[id]/components/KillSwitch"
    );
    render(
      React.createElement(KillSwitch, {
        workspaceId: "ws_1",
        zombie: { id: "zom_1", name: "alpha", status, created_at: "2026-04-22T00:00:00Z" },
      } as never),
    );
  }

  it("renders Killed label when zombie is terminal (no actions)", async () => {
    await renderSwitch("killed");
    expect(screen.getByText("Killed")).toBeTruthy();
  });

  it("offers Resume + Kill when zombie is stopped", async () => {
    await renderSwitch("stopped");
    expect(screen.getByRole("button", { name: /^resume$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^kill$/i })).toBeTruthy();
  });

  it("offers Resume + Kill when zombie is paused (auto-halt)", async () => {
    await renderSwitch("paused");
    expect(screen.getByRole("button", { name: /^resume$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^kill$/i })).toBeTruthy();
  });

  // After opening the action dialog, both the trigger button and the
  // ConfirmDialog confirm button carry the same accessible name. Scope the
  // confirm-click to the alertdialog subtree to disambiguate.
  async function clickConfirmInDialog(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
    const dialog = await screen.findByRole("alertdialog");
    const { within } = await import("@testing-library/react");
    await user.click(within(dialog).getByRole("button", { name }));
  }

  it("active → Stop happy path: click → confirm → setZombieStatusAction(stopped) → refresh", async () => {
    const user = userEvent.setup();
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() =>
      expect(setZombieStatusActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "stopped"),
    );
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("stopped → Resume sends status='active'", async () => {
    const user = userEvent.setup();
    await renderSwitch("stopped");
    await user.click(screen.getByRole("button", { name: /^resume$/i }));
    await clickConfirmInDialog(user, /^resume$/i);
    await waitFor(() =>
      expect(setZombieStatusActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "active"),
    );
  });

  it("active → Kill sends status='killed'", async () => {
    const user = userEvent.setup();
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^kill$/i }));
    await clickConfirmInDialog(user, /^kill$/i);
    await waitFor(() =>
      expect(setZombieStatusActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "killed"),
    );
  });

  it("409 conflict closes the dialog and refreshes (status changed elsewhere)", async () => {
    const { ApiError } = await import("../lib/api/errors");
    stopZombieMock.mockRejectedValue(new ApiError("transition not allowed", 409, "UZ-ZMB-010", "req_x"));
    const user = userEvent.setup();
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });

  it("non-409 error keeps dialog open (status rolled back)", async () => {
    stopZombieMock.mockRejectedValue(new Error("network down"));
    const user = userEvent.setup();
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() => expect(stopZombieMock).toHaveBeenCalled());
    expect(screen.queryByRole("alertdialog")).toBeTruthy();
  });

  it("server action reporting unauth surfaces the error and rolls back the optimistic flip", async () => {
    setZombieStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    const user = userEvent.setup();
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Not authenticated/),
    );
    expect(stopZombieMock).not.toHaveBeenCalled();
  });

  it("server action returning empty error string falls back to 'Failed to stop zombie' default", async () => {
    setZombieStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup();
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    await clickConfirmInDialog(user, /^stop$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't stop this zombie/i),
    );
  });

  // WS-G — every ActionConfig carries its own `errorVerb` literal so the
  // operator-facing sentence reads naturally per action. The Stop case above
  // exercises the Stop verb; the next two pin Resume and Kill so each branch
  // of the static-literal config is hit by patch coverage.
  it("resume action error path renders 'Couldn't resume this zombie' (WS-G verb literal)", async () => {
    setZombieStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup();
    await renderSwitch("stopped");
    await user.click(screen.getByRole("button", { name: /^resume$/i }));
    await clickConfirmInDialog(user, /^resume$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't resume this zombie/i),
    );
  });

  it("kill action error path renders 'Couldn't kill this zombie' (WS-G verb literal)", async () => {
    setZombieStatusActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup();
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^kill$/i }));
    await clickConfirmInDialog(user, /^kill$/i);
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't kill this zombie/i),
    );
  });

  // Pins the dialog-dismiss path: clicking Cancel drives onOpenChange(false)
  // which clears pendingAction. Without this, the close-handler line stays
  // uncovered by patch coverage even though every other interaction works.
  it("Cancel dismisses the confirm dialog and clears pendingAction", async () => {
    const user = userEvent.setup();
    await renderSwitch("active");
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    const dialog = await screen.findByRole("alertdialog");
    const { within } = await import("@testing-library/react");
    await user.click(within(dialog).getByRole("button", { name: /cancel/i }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(setZombieStatusActionMock).not.toHaveBeenCalled();
  });
});

// ── ZombiesList ────────────────────────────────────────────────────────────

describe("ZombiesList component", () => {
  const baseZombies = [
    { id: "zom_1", name: "alpha-bot", status: "active", created_at: 1745280000000, updated_at: 1745280000000 },
    { id: "zom_2", name: "beta-bot", status: "paused", created_at: 1745280001000, updated_at: 1745280001000 },
  ];

  async function renderList(props: {
    initialZombies?: typeof baseZombies;
    initialCursor?: string | null;
  } = {}) {
    const { default: ZombiesList } = await import(
      "../app/(dashboard)/zombies/components/ZombiesList"
    );
    render(
      React.createElement(ZombiesList, {
        workspaceId: "ws_1",
        initialZombies: props.initialZombies ?? baseZombies,
        initialCursor: props.initialCursor ?? null,
      } as never),
    );
  }

  it("renders a row per zombie with name + status + id", async () => {
    await renderList();
    expect(screen.getByText("alpha-bot")).toBeTruthy();
    expect(screen.getByText("beta-bot")).toBeTruthy();
    expect(screen.getByText("zom_1")).toBeTruthy();
  });

  it("search filters rows down by name (case-insensitive)", async () => {
    const user = userEvent.setup();
    await renderList();
    await user.type(screen.getByLabelText(/search zombies/i), "ALPHA");
    await waitFor(() => expect(screen.queryByText("beta-bot")).toBeNull());
    expect(screen.getByText("alpha-bot")).toBeTruthy();
  });

  it("search shows empty-match message when nothing matches", async () => {
    const user = userEvent.setup();
    await renderList();
    await user.type(screen.getByLabelText(/search zombies/i), "zzz-no-match");
    await waitFor(() =>
      expect(screen.getByText(/No zombies match/i)).toBeTruthy(),
    );
  });

  it("loadMore: hidden when no cursor", async () => {
    await renderList({ initialCursor: null });
    expect(screen.queryByRole("button", { name: /load more/i })).toBeNull();
  });

  it("loadMore: visible when cursor is present and fetches next page", async () => {
    listZombiesMock.mockResolvedValue({
      items: [
        { id: "zom_3", name: "gamma-bot", status: "active", created_at: "2026-04-22T00:00:02Z" },
      ],
      total: 1,
      cursor: null,
    });
    const user = userEvent.setup();
    await renderList({ initialCursor: "cursor_1" });
    const btn = screen.getByRole("button", { name: /load more/i });
    await user.click(btn);
    await waitFor(() =>
      expect(listZombiesActionMock).toHaveBeenCalledWith("ws_1", { cursor: "cursor_1" }),
    );
    await waitFor(() => expect(screen.getByText("gamma-bot")).toBeTruthy());
  });

  it("loadMore: surfaces fetch error as an alert", async () => {
    listZombiesMock.mockRejectedValue(new Error("boom"));
    const user = userEvent.setup();
    await renderList({ initialCursor: "cursor_1" });
    await user.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/boom/),
    );
  });

  it("loadMore: unauthenticated action result surfaces Not authenticated", async () => {
    listZombiesActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    const user = userEvent.setup();
    await renderList({ initialCursor: "cursor_1" });
    await user.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Not authenticated/),
    );
  });

  it("loadMore: empty error string falls back to default message (covers `||` short-circuit)", async () => {
    listZombiesActionMock.mockResolvedValueOnce({
      ok: false,
      error: "",
      status: 500,
    });
    const user = userEvent.setup();
    await renderList({ initialCursor: "cursor_1" });
    await user.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't load more zombies/),
    );
  });

  it("wake-pulse fires only on active rows (data-live attribute)", async () => {
    await renderList({
      initialZombies: [
        { id: "zom_1", name: "alpha-bot", status: "active", created_at: 1745280000000, updated_at: 1745280000000 },
        { id: "zom_2", name: "beta-bot", status: "paused", created_at: 1745280001000, updated_at: 1745280001000 },
        { id: "zom_3", name: "gamma-bot", status: "killed", created_at: 1745280002000, updated_at: 1745280002000 },
      ],
    });
    const liveRow = screen.getByRole("link", { name: /alpha-bot/i });
    const parkedRow = screen.getByRole("link", { name: /beta-bot/i });
    const failedRow = screen.getByRole("link", { name: /gamma-bot/i });
    expect(liveRow.getAttribute("data-state")).toBe("live");
    expect(parkedRow.getAttribute("data-state")).toBe("parked");
    expect(failedRow.getAttribute("data-state")).toBe("failed");
    expect(liveRow.querySelector("[data-live]")).toBeTruthy();
    expect(parkedRow.querySelector("[data-live]")).toBeFalsy();
    expect(failedRow.querySelector("[data-live]")).toBeFalsy();
  });

  it("wake-pulse cap: only first 5 live rows in render order pulse; rest static", async () => {
    const sixLive = Array.from({ length: 6 }, (_, i) => ({
      id: `zom_${i + 1}`,
      name: `live-${i + 1}`,
      status: "active",
      created_at: 1745280000000 + i,
      updated_at: 1745280000000 + i,
    }));
    await renderList({ initialZombies: sixLive });
    const rows = screen.getAllByRole("link", { name: /live-/i });
    expect(rows).toHaveLength(6);
    const livePulses = rows.filter((r) => r.querySelector("[data-live]"));
    expect(livePulses).toHaveLength(5);
    // Header consolidation count is shown.
    expect(screen.getByLabelText(/6 live/i)).toBeTruthy();
  });

  it("status dot palette: live, parked, failed via data-state", async () => {
    await renderList({
      initialZombies: [
        { id: "zom_1", name: "alpha", status: "active", created_at: 1745280000000, updated_at: 1745280000000 },
        { id: "zom_2", name: "beta", status: "paused", created_at: 1745280001000, updated_at: 1745280001000 },
        { id: "zom_3", name: "gamma", status: "killed", created_at: 1745280002000, updated_at: 1745280002000 },
        { id: "zom_4", name: "delta", status: "errored", created_at: 1745280003000, updated_at: 1745280003000 },
      ],
    });
    expect(screen.getByRole("link", { name: /alpha/ }).getAttribute("data-state")).toBe("live");
    expect(screen.getByRole("link", { name: /beta/ }).getAttribute("data-state")).toBe("parked");
    expect(screen.getByRole("link", { name: /gamma/ }).getAttribute("data-state")).toBe("failed");
    expect(screen.getByRole("link", { name: /delta/ }).getAttribute("data-state")).toBe("failed");
  });
});

// ── WorkspaceSwitcher ──────────────────────────────────────────────────────

describe("WorkspaceSwitcher component", () => {
  async function renderSwitcher(props: {
    workspaces?: Array<{ id: string; name: string | null }>;
    activeId?: string | null;
    onSwitch?: (id: string) => void | Promise<void>;
  } = {}) {
    const { default: WorkspaceSwitcher } = await import(
      "../components/layout/WorkspaceSwitcher"
    );
    render(
      React.createElement(WorkspaceSwitcher, {
        workspaces: props.workspaces ?? [
          { id: "ws_1", name: "Alpha" },
          { id: "ws_2", name: "Beta" },
        ],
        activeId: props.activeId ?? "ws_1",
        onSwitch: props.onSwitch ?? setActiveWorkspaceMock,
      } as never),
    );
  }

  it("still renders with a New workspace affordance when workspaces is empty", async () => {
    render(
      React.createElement(
        (await import("../components/layout/WorkspaceSwitcher")).default,
        { workspaces: [], activeId: null, onSwitch: setActiveWorkspaceMock } as never,
      ),
    );
    // The empty case is exactly when create matters most — switcher must show.
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("No workspace");
    expect(screen.getByTestId("workspace-new")).toBeTruthy();
  });

  it("opens the create dialog from the New workspace item", async () => {
    const user = userEvent.setup();
    await renderSwitcher();
    await user.click(screen.getByTestId("workspace-new"));
    await waitFor(() => expect(screen.getByTestId("workspace-name-input")).toBeTruthy());
  });

  it("renders the active workspace label", async () => {
    await renderSwitcher();
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("Alpha");
  });

  it("falls back to id when name is null", async () => {
    await renderSwitcher({
      workspaces: [{ id: "ws_only", name: null }],
      activeId: "ws_only",
    });
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("ws_only");
  });

  it("falls back to the first workspace when activeId is unknown", async () => {
    await renderSwitcher({
      workspaces: [
        { id: "ws_a", name: "Alpha" },
        { id: "ws_b", name: "Beta" },
      ],
      activeId: "ws_unknown",
    });
    expect(screen.getByLabelText(/select workspace/i).textContent).toContain("Alpha");
  });

  it("picking a different workspace invokes the onSwitch callback", async () => {
    const user = userEvent.setup();
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    // Second item = Beta (different from active ws_1)
    await user.click(items[1]!);
    await waitFor(() =>
      expect(setActiveWorkspaceMock).toHaveBeenCalledWith("ws_2"),
    );
  });

  it("picking the active workspace is a no-op", async () => {
    const user = userEvent.setup();
    await renderSwitcher();
    const items = screen.getAllByRole("menuitem");
    // First item = Alpha (same as active)
    await user.click(items[0]!);
    // Give transition a tick
    await new Promise((r) => setTimeout(r, 10));
    expect(setActiveWorkspaceMock).not.toHaveBeenCalled();
  });
});

// ── CreateWorkspaceDialog ───────────────────────────────────────────────────

describe("CreateWorkspaceDialog component", () => {
  async function renderDialog(
    props: { open?: boolean; onOpenChange?: (open: boolean) => void } = {},
  ) {
    const onOpenChange = props.onOpenChange ?? vi.fn();
    const { default: CreateWorkspaceDialog } = await import(
      "../components/layout/CreateWorkspaceDialog"
    );
    render(
      React.createElement(CreateWorkspaceDialog, {
        open: props.open ?? true,
        onOpenChange,
      } as never),
    );
    return { onOpenChange };
  }

  it("submits the trimmed name, then closes and refreshes on success", async () => {
    const user = userEvent.setup();
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_x", name: "acme-prod" },
    });
    const { onOpenChange } = await renderDialog();
    await user.type(screen.getByTestId("workspace-name-input"), "  acme-prod  ");
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(createWorkspaceActionMock).toHaveBeenCalledWith({ name: "acme-prod" }),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("omits a blank name so the server generates a Heroku-style one", async () => {
    const user = userEvent.setup();
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_y", name: "auto-gen" },
    });
    await renderDialog();
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(createWorkspaceActionMock).toHaveBeenCalledWith({ name: undefined }),
    );
  });

  it("shows the mapped error and stays open when the action fails", async () => {
    const user = userEvent.setup();
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-AUTH-401",
      error: "Missing tenant context on session",
    });
    const { onOpenChange } = await renderDialog();
    await user.type(screen.getByTestId("workspace-name-input"), "x");
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(screen.getByTestId("workspace-create-error")).toBeTruthy(),
    );
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("submits when Enter is pressed inside the name field", async () => {
    const user = userEvent.setup();
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: true,
      data: { workspace_id: "ws_z", name: "via-enter" },
    });
    await renderDialog();
    await user.type(screen.getByTestId("workspace-name-input"), "via-enter{Enter}");
    await waitFor(() =>
      expect(createWorkspaceActionMock).toHaveBeenCalledWith({ name: "via-enter" }),
    );
  });

  it("Cancel closes the dialog without calling the action", async () => {
    const user = userEvent.setup();
    const { onOpenChange } = await renderDialog();
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(createWorkspaceActionMock).not.toHaveBeenCalled();
  });

  it("ignores a second Enter submit while the first is still in flight", async () => {
    const user = userEvent.setup();
    let release: (v: unknown) => void = () => {};
    createWorkspaceActionMock.mockImplementationOnce(
      () => new Promise((r) => { release = r; }), // stays pending until released
    );
    await renderDialog();
    const input = screen.getByTestId("workspace-name-input");
    await user.type(input, "ws{Enter}"); // first submit → useTransition pending
    await user.type(input, "{Enter}"); // second Enter hits the `if (pending) return` guard
    expect(createWorkspaceActionMock).toHaveBeenCalledTimes(1);
    release({ ok: true, data: { workspace_id: "ws_p", name: "ws" } });
  });

  it("clears a typed-but-unsaved name when the dialog closes, so a reopen starts fresh", async () => {
    const user = userEvent.setup();
    const { default: CreateWorkspaceDialog } = await import(
      "../components/layout/CreateWorkspaceDialog"
    );
    const onOpenChange = vi.fn();
    const { rerender } = render(
      React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never),
    );
    await user.type(screen.getByTestId("workspace-name-input"), "draft-name");
    expect(
      (screen.getByTestId("workspace-name-input") as HTMLInputElement).value,
    ).toBe("draft-name");
    // Close (open→false): the effect cleanup resets the form while the
    // component stays mounted.
    rerender(
      React.createElement(CreateWorkspaceDialog, { open: false, onOpenChange } as never),
    );
    // Reopen: the name field is blank again, not the abandoned "draft-name".
    rerender(
      React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never),
    );
    expect(
      (screen.getByTestId("workspace-name-input") as HTMLInputElement).value,
    ).toBe("");
  });

  it("drops a stale error when the dialog closes, so a reopen starts clean", async () => {
    const user = userEvent.setup();
    createWorkspaceActionMock.mockResolvedValueOnce({
      ok: false,
      errorCode: "UZ-AUTH-401",
      error: "Missing tenant context on session",
    });
    const { default: CreateWorkspaceDialog } = await import(
      "../components/layout/CreateWorkspaceDialog"
    );
    const onOpenChange = vi.fn();
    const { rerender } = render(
      React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never),
    );
    await user.type(screen.getByTestId("workspace-name-input"), "x");
    await user.click(screen.getByTestId("workspace-create-submit"));
    await waitFor(() =>
      expect(screen.getByTestId("workspace-create-error")).toBeTruthy(),
    );
    // Close then reopen: the cleanup must drop the error banner, not carry the
    // failed-attempt message into a fresh session.
    rerender(
      React.createElement(CreateWorkspaceDialog, { open: false, onOpenChange } as never),
    );
    rerender(
      React.createElement(CreateWorkspaceDialog, { open: true, onOpenChange } as never),
    );
    expect(screen.queryByTestId("workspace-create-error")).toBeNull();
  });
});
