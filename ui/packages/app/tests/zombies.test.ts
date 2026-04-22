import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderToStaticMarkup } from "react-dom/server";

// ── Shared mocks ───────────────────────────────────────────────────────────

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const notFound = vi.fn(() => {
  throw new Error("notFound");
});
const usePathname = vi.fn(() => "/zombies");
const routerPush = vi.fn();
const routerRefresh = vi.fn();
const useRouter = vi.fn(() => ({ push: routerPush, refresh: routerRefresh }));
const auth = vi.fn();
const getTokenFn = vi.fn().mockResolvedValue("token_xyz");
const useAuth = vi.fn(() => ({ getToken: getTokenFn }));

const resolveActiveWorkspace = vi.fn();
const fetchMock = vi.fn();
const clipboardWriteText = vi.fn().mockResolvedValue(undefined);

vi.stubGlobal("fetch", fetchMock);

vi.mock("next/navigation", () => ({ redirect, notFound, usePathname, useRouter }));
vi.mock("@clerk/nextjs/server", () => ({ auth }));
vi.mock("@clerk/nextjs", () => ({
  useAuth,
  useUser: vi.fn(() => ({ isLoaded: true, isSignedIn: true, user: null })),
  UserButton: () => React.createElement("div", { "data-user-button": "1" }),
  ClerkProvider: ({ children }: { children: React.ReactNode }) => React.createElement(React.Fragment, null, children),
  SignIn: () => React.createElement("div", { "data-sign-in": "1" }),
  SignUp: () => React.createElement("div", { "data-sign-up": "1" }),
}));
vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href, ...rest }, children),
}));
vi.mock("@/lib/workspace", () => ({
  resolveActiveWorkspace,
  listTenantWorkspacesCached: vi.fn().mockResolvedValue({ items: [], total: 0 }),
}));

vi.mock("lucide-react", () => {
  function Icon(name: string) {
    const Component = (props: Record<string, unknown>) =>
      React.createElement("svg", { ...props, "data-icon": name });
    Component.displayName = name;
    return Component;
  }
  return {
    AlertTriangleIcon: Icon("AlertTriangleIcon"),
    CheckIcon: Icon("CheckIcon"),
    CopyIcon: Icon("CopyIcon"),
    Loader2Icon: Icon("Loader2Icon"),
    PlusIcon: Icon("PlusIcon"),
    ShieldIcon: Icon("ShieldIcon"),
    KeyRoundIcon: Icon("KeyRoundIcon"),
    Trash2Icon: Icon("Trash2Icon"),
  };
});

const TabsCtx = React.createContext<{ active: string; setActive: (v: string) => void }>({
  active: "",
  setActive: () => {},
});

vi.mock("@usezombie/design-system", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@usezombie/design-system")>();
  return {
  ...actual,
  buttonClassName: (variant: string, size: string) => `btn-${variant}-${size}`,
  Button: ({ children, variant, size, ...rest }: { children: React.ReactNode; variant?: string; size?: string } & React.ButtonHTMLAttributes<HTMLButtonElement>) =>
    React.createElement("button", { "data-variant": variant, "data-size": size, ...rest }, children),
  ConfirmDialog: ({ open, onOpenChange, title, description, confirmLabel = "Confirm", cancelLabel = "Cancel", onConfirm, errorMessage, onError }: { open: boolean; onOpenChange: (v: boolean) => void; title: string; description?: React.ReactNode; confirmLabel?: string; cancelLabel?: string; onConfirm: () => void | Promise<void>; errorMessage?: string | null; onError?: (e: unknown) => void }) =>
    open
      ? React.createElement(
          "div",
          { role: "alertdialog", "data-confirm-dialog": title },
          React.createElement("div", { key: "title" }, title),
          description ? React.createElement("div", { key: "desc" }, description) : null,
          errorMessage ? React.createElement("div", { key: "err", role: "alert" }, errorMessage) : null,
          React.createElement(
            "button",
            { key: "cancel", type: "button", onClick: () => onOpenChange(false) },
            cancelLabel,
          ),
          React.createElement(
            "button",
            {
              key: "confirm",
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
  EmptyState: ({ title, description }: { title: string; description?: string }) =>
    React.createElement("div", { "data-empty-state": title }, description),
  Input: (props: React.InputHTMLAttributes<HTMLInputElement>) =>
    React.createElement("input", props),
  Textarea: (props: React.TextareaHTMLAttributes<HTMLTextAreaElement>) =>
    React.createElement("textarea", props),
  Tabs: ({ children, defaultValue, value: controlled, onValueChange }: { children: React.ReactNode; defaultValue?: string; value?: string; onValueChange?: (v: string) => void }) => {
    const [active, setActive] = React.useState<string>(controlled ?? defaultValue ?? "");
    const ctxValue = controlled ?? active;
    return React.createElement(
      TabsCtx.Provider,
      { value: { active: ctxValue, setActive: (v: string) => { setActive(v); onValueChange?.(v); } } },
      React.createElement("div", { "data-tabs-root": ctxValue }, children),
    );
  },
  TabsList: ({ children, ...rest }: { children: React.ReactNode } & React.HTMLAttributes<HTMLDivElement>) =>
    React.createElement("div", { role: "tablist", ...rest }, children),
  TabsTrigger: ({ children, value, ...rest }: { children: React.ReactNode; value: string } & React.ButtonHTMLAttributes<HTMLButtonElement>) => {
    const ctx = React.useContext(TabsCtx);
    const isActive = ctx.active === value;
    return React.createElement(
      "button",
      {
        role: "tab",
        "data-tab-value": value,
        "data-state": isActive ? "active" : "inactive",
        type: "button",
        onClick: () => ctx.setActive(value),
        ...rest,
      },
      children,
    );
  },
  TabsContent: ({ children, value, ...rest }: { children: React.ReactNode; value: string } & React.HTMLAttributes<HTMLDivElement>) => {
    const ctx = React.useContext(TabsCtx);
    if (ctx.active !== value) return null;
    return React.createElement("div", { role: "tabpanel", "data-tab-value": value, ...rest }, children);
  },
  PageHeader: ({ children }: { children: React.ReactNode }) =>
    React.createElement("header", { "data-page-header": "1" }, children),
  PageTitle: ({ children }: { children: React.ReactNode }) =>
    React.createElement("h1", { "data-page-title": "1" }, children),
  SectionLabel: ({ children }: { children: React.ReactNode }) =>
    React.createElement("h2", { "data-section-label": "1" }, children),
  };
});

beforeEach(() => {
  vi.clearAllMocks();
  getTokenFn.mockResolvedValue("token_xyz");
  usePathname.mockReturnValue("/zombies");
  useRouter.mockReturnValue({ push: routerPush, refresh: routerRefresh });
  useAuth.mockReturnValue({ getToken: getTokenFn });
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_xyz") });
  Object.defineProperty(window.navigator, "clipboard", {
    value: { writeText: clipboardWriteText },
    configurable: true,
  });
});

afterEach(() => {
  cleanup();
  fetchMock.mockReset();
});

// ── lib/api/zombies.ts ─────────────────────────────────────────────────────

describe("lib/api/zombies", () => {
  it("listZombies sends GET with bearer and parses the envelope", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [{ id: "zom_1" }], total: 1, next_cursor: null }),
    });
    const mod = await import("../lib/api/zombies");
    const res = await mod.listZombies("ws_1", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/zombies"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tkn" }),
      }),
    );
    expect(res.items[0]?.id).toBe("zom_1");
  });

  it("installZombie sends POST body and returns the created zombie", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => ({ zombie_id: "zom_2", status: "active" }),
    });
    const mod = await import("../lib/api/zombies");
    const body = {
      name: "lead-collector",
      source_markdown: "---\nname: lead-collector\n---\nhi",
      config_json: '{"name":"lead-collector"}',
    };
    const res = await mod.installZombie("ws_1", body, "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/zombies"),
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify(body),
      }),
    );
    expect(res.zombie_id).toBe("zom_2");
  });

  it("installZombie surfaces API error status + code", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 409,
      statusText: "Conflict",
      json: async () => ({ error: "name taken", code: "UZ-ZOM-002" }),
    });
    const mod = await import("../lib/api/zombies");
    await expect(
      mod.installZombie(
        "ws_1",
        { name: "dup", source_markdown: "s", config_json: "{}" },
        "tkn",
      ),
    ).rejects.toMatchObject({ status: 409, code: "UZ-ZOM-002", message: "name taken" });
  });

  it("installZombie falls back to statusText when body is unparseable", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 500,
      statusText: "Server Error",
      json: async () => {
        throw new Error("bad json");
      },
    });
    const mod = await import("../lib/api/zombies");
    await expect(
      mod.installZombie(
        "ws_1",
        { name: "x", source_markdown: "y", config_json: "{}" },
        "tkn",
      ),
    ).rejects.toMatchObject({ status: 500, message: "Server Error" });
  });

  it("deleteZombie sends DELETE and returns void on 204", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204 });
    const mod = await import("../lib/api/zombies");
    const res = await mod.deleteZombie("ws_1", "zom_2", "tkn");
    expect(res).toBeUndefined();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/zombies/zom_2"),
      expect.objectContaining({ method: "DELETE" }),
    );
  });

  it("webhookUrlFor composes the deterministic webhook URL", async () => {
    const mod = await import("../lib/api/zombies");
    expect(mod.webhookUrlFor("zom_abc")).toBe(
      "https://api-dev.usezombie.com/v1/webhooks/zom_abc",
    );
  });
});

// ── lib/api/tenant_billing.ts ──────────────────────────────────────────────

describe("lib/api/tenant_billing", () => {
  it("getTenantBilling sends GET with bearer and returns the snapshot", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        plan_tier: "free",
        plan_sku: "free_default",
        balance_cents: 1000,
        updated_at: 1713700000000,
        is_exhausted: false,
        exhausted_at: null,
      }),
    });
    const mod = await import("../lib/api/tenant_billing");
    const res = await mod.getTenantBilling("tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/tenants/me/billing"),
      expect.objectContaining({
        headers: expect.objectContaining({ Authorization: "Bearer tok" }),
      }),
    );
    expect(res.is_exhausted).toBe(false);
  });

  it("getTenantBilling throws with status + code on error", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 401,
      statusText: "Unauthorized",
      json: async () => ({ error: "bad token", code: "UZ-AUTH-001" }),
    });
    const mod = await import("../lib/api/tenant_billing");
    await expect(mod.getTenantBilling("bad")).rejects.toMatchObject({
      status: 401,
      code: "UZ-AUTH-001",
      message: "bad token",
    });
  });

  it("getTenantBilling falls back to statusText when body parse fails", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 500,
      statusText: "Server Error",
      json: async () => {
        throw new Error("bad json");
      },
    });
    const mod = await import("../lib/api/tenant_billing");
    await expect(mod.getTenantBilling("tok")).rejects.toMatchObject({
      status: 500,
      message: "Server Error",
    });
  });
});

// ── ExhaustionBadge + ExhaustionBanner ─────────────────────────────────────

describe("exhaustion components", () => {
  it("ExhaustionBadge renders with a timestamped tooltip when exhaustedAt set", async () => {
    const { default: ExhaustionBadge } = await import(
      "../components/domain/ExhaustionBadge"
    );
    render(React.createElement(ExhaustionBadge, { exhaustedAt: 1713700000000 }));
    const el = screen.getByRole("status", { name: /balance exhausted/i });
    expect(el).toBeTruthy();
    expect(el.getAttribute("title")).toMatch(/^Exhausted since /);
  });

  it("ExhaustionBadge renders with generic title when exhaustedAt is null", async () => {
    const { default: ExhaustionBadge } = await import(
      "../components/domain/ExhaustionBadge"
    );
    render(React.createElement(ExhaustionBadge, { exhaustedAt: null }));
    expect(screen.getByRole("status").getAttribute("title")).toBe("Balance exhausted");
  });

  it("ExhaustionBanner renders nothing when billing is null", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    const { container } = render(
      React.createElement(ExhaustionBanner, { billing: null }),
    );
    expect(container.innerHTML).toBe("");
  });

  it("ExhaustionBanner renders nothing when not exhausted", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    const { container } = render(
      React.createElement(ExhaustionBanner, {
        billing: {
          plan_tier: "free",
          plan_sku: "free_default",
          balance_cents: 500,
          updated_at: 0,
          is_exhausted: false,
          exhausted_at: null,
        },
      }),
    );
    expect(container.innerHTML).toBe("");
  });

  it("ExhaustionBanner renders destructive alert with timestamp when exhausted", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    render(
      React.createElement(ExhaustionBanner, {
        billing: {
          plan_tier: "free",
          plan_sku: "free_default",
          balance_cents: 0,
          updated_at: 1713700000000,
          is_exhausted: true,
          exhausted_at: 1713700400000,
        },
      }),
    );
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("credit balance is exhausted");
    expect(alert.textContent).toContain("BALANCE_EXHAUSTED_POLICY");
    expect(alert.querySelector("a[href^='mailto:']")).toBeTruthy();
    expect(alert.textContent).toMatch(/Exhausted since /);
  });

  it("ExhaustionBanner omits timestamp text when exhausted_at is null", async () => {
    const { default: ExhaustionBanner } = await import(
      "../components/domain/ExhaustionBanner"
    );
    render(
      React.createElement(ExhaustionBanner, {
        billing: {
          plan_tier: "free",
          plan_sku: "free_default",
          balance_cents: 0,
          updated_at: 0,
          is_exhausted: true,
          exhausted_at: null,
        },
      }),
    );
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("credit balance is exhausted");
    expect(alert.textContent).not.toMatch(/Exhausted since /);
  });
});

// ── Zombies route — page, loading, detail, new ─────────────────────────────

type BillingSnapshot = {
  plan_tier: string;
  plan_sku: string;
  balance_cents: number;
  updated_at: number;
  is_exhausted: boolean;
  exhausted_at: number | null;
};

describe("zombies routes", () => {
  const happyBilling: BillingSnapshot = {
    plan_tier: "free",
    plan_sku: "free_default",
    balance_cents: 1000,
    updated_at: 0,
    is_exhausted: false,
    exhausted_at: null,
  };
  const exhaustedBilling: BillingSnapshot = {
    ...happyBilling,
    is_exhausted: true,
    exhausted_at: 1,
  };

  function mockFetchBilling(billing: BillingSnapshot) {
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => billing };
      }
      if (url.includes("/activity")) {
        return { ok: true, status: 200, json: async () => ({ events: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          items: [
            {
              id: "zom_1",
              name: "lead-collector",
              status: "active",
              created_at: 1713700000000,
              updated_at: 1713700000000,
            },
          ],
          total: 1,
        }),
      };
    });
  }

  it("loading.tsx renders a spinner with status role", async () => {
    const { default: Loading } = await import("../app/(dashboard)/zombies/loading");
    render(React.createElement(Loading));
    const el = screen.getByRole("status");
    expect(el.textContent).toContain("Loading zombies");
    expect(el.querySelector("[data-icon=Loader2Icon]")).toBeTruthy();
  });

  it("zombies list page notFound when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/zombies/page");
    await expect(Page()).rejects.toThrow("notFound");
  });

  it("zombies list page renders empty-workspace state", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce(null);
    const { default: Page } = await import("../app/(dashboard)/zombies/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("No workspace yet");
  });

  it("zombies list page renders empty-zombies state with banner suppressed", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: true, status: 200, json: async () => happyBilling };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({ items: [], total: 0, next_cursor: null }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/zombies/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("No zombies yet");
    expect(markup).toContain("Install Zombie");
    expect(markup).not.toContain("credit balance is exhausted");
  });

  it("zombies list page renders populated list + exhaustion banner", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(exhaustedBilling);
    const { default: Page } = await import("../app/(dashboard)/zombies/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("href=\"/zombies/zom_1\"");
    expect(markup).toContain("lead-collector");
    expect(markup).toContain("credit balance is exhausted");
  });

  it("zombies list page swallows a failed billing fetch and still renders", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        return { ok: false, status: 500, statusText: "err", json: async () => ({}) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({ items: [], total: 0, next_cursor: null }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/zombies/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("No zombies yet");
  });

  it("zombies new page notFound when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/zombies/new/page");
    await expect(Page()).rejects.toThrow("notFound");
  });

  it("zombies new page renders empty-workspace guard", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce(null);
    const { default: Page } = await import("../app/(dashboard)/zombies/new/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("Create a workspace before installing zombies");
  });

  it("zombies new page renders the install form when a workspace exists", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    const { default: Page } = await import("../app/(dashboard)/zombies/new/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("Install Zombie");
    expect(markup).toContain("name=\"name\"");
    expect(markup).toContain("name=\"source_markdown\"");
    expect(markup).toContain("name=\"config_json\"");
  });

  it("zombies detail page notFound when no token", async () => {
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/zombies/[id]/page");
    await expect(Page({ params: Promise.resolve({ id: "zom_1" }) })).rejects.toThrow(
      "notFound",
    );
  });

  it("zombies detail page notFound when no workspace", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce(null);
    const { default: Page } = await import("../app/(dashboard)/zombies/[id]/page");
    await expect(Page({ params: Promise.resolve({ id: "zom_1" }) })).rejects.toThrow(
      "notFound",
    );
  });

  it("zombies detail page notFound when zombie id is not in the list", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/zombies/[id]/page");
    await expect(
      Page({ params: Promise.resolve({ id: "missing" }) }),
    ).rejects.toThrow("notFound");
  });

  it("zombies detail page renders panels + exhaustion badge when tenant is exhausted", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(exhaustedBilling);
    const { default: Page } = await import("../app/(dashboard)/zombies/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("lead-collector");
    expect(markup).toContain("Trigger");
    expect(markup).toContain("Firewall rules");
    expect(markup).toContain("Configuration");
    expect(markup).toContain("Balance exhausted");
  });

  it("zombies detail page renders without badge when not exhausted", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    mockFetchBilling(happyBilling);
    const { default: Page } = await import("../app/(dashboard)/zombies/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).not.toContain("Balance exhausted");
  });

  it("zombies detail page handles billing fetch failure gracefully (catch branch)", async () => {
    resolveActiveWorkspace.mockResolvedValueOnce({ id: "ws_1" });
    fetchMock.mockImplementation(async (url: string) => {
      if (url.endsWith("/v1/tenants/me/billing")) {
        throw new Error("network down");
      }
      if (url.includes("/activity")) {
        return { ok: true, status: 200, json: async () => ({ events: [], next_cursor: null }) };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({
          items: [
            {
              id: "zom_1",
              name: "lead-collector",
              status: "active",
              created_at: 1713700000000,
              updated_at: 1713700000000,
            },
          ],
          total: 1,
        }),
      };
    });
    const { default: Page } = await import("../app/(dashboard)/zombies/[id]/page");
    const markup = renderToStaticMarkup(
      await Page({ params: Promise.resolve({ id: "zom_1" }) }),
    );
    expect(markup).toContain("lead-collector");
    expect(markup).not.toContain("Balance exhausted");
  });
});

// ── Panel interactions (real DOM) ──────────────────────────────────────────

describe("TriggerPanel interactions", () => {
  it("shows webhook URL by default, copy button calls clipboard and toggles state", async () => {
    const { default: TriggerPanel } = await import(
      "../app/(dashboard)/zombies/[id]/components/TriggerPanel"
    );
    render(React.createElement(TriggerPanel, { zombieId: "zom_abc" }));

    expect(screen.getByTestId("webhook-url").textContent).toBe(
      "https://api-dev.usezombie.com/v1/webhooks/zom_abc",
    );
    // fireEvent.click instead of userEvent.click — user-event v14 installs its
    // own clipboard shim that hides the navigator.clipboard spy this test sets.
    fireEvent.click(screen.getByRole("button", { name: /copy webhook url/i }));
    expect(clipboardWriteText).toHaveBeenCalledWith(
      "https://api-dev.usezombie.com/v1/webhooks/zom_abc",
    );
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /copy webhook url/i }).textContent,
      ).toMatch(/Copied/),
    );
  });

  it("copy handler swallows clipboard API rejection without throwing", async () => {
    clipboardWriteText.mockRejectedValueOnce(new Error("denied"));
    const { default: TriggerPanel } = await import(
      "../app/(dashboard)/zombies/[id]/components/TriggerPanel"
    );
    render(React.createElement(TriggerPanel, { zombieId: "zom_abc" }));
    // Should not throw even though clipboard rejects.
    fireEvent.click(screen.getByRole("button", { name: /copy webhook url/i }));
    expect(clipboardWriteText).toHaveBeenCalled();
    // Let the rejected promise settle so the try/catch branch completes.
    await Promise.resolve();
    // State stays un-toggled — still the Copy icon/label.
    expect(screen.getByRole("button", { name: /copy webhook url/i }).textContent).toMatch(
      /Copy/,
    );
  });

  it("switching to schedule tab reveals the CLI-only placeholder", async () => {
    const { default: TriggerPanel } = await import(
      "../app/(dashboard)/zombies/[id]/components/TriggerPanel"
    );
    const user = userEvent.setup();
    render(React.createElement(TriggerPanel, { zombieId: "zom_abc" }));

    await user.click(screen.getByRole("tab", { name: /schedule \(cron\)/i }));
    expect(screen.getByText(/Cron scheduling is CLI-only/i)).toBeTruthy();
    expect(screen.queryByTestId("webhook-url")).toBeNull();
  });
});

describe("ZombieConfig interactions", () => {
  it("delete flow: confirm dialog → DELETE call → push to /zombies", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204 });
    const { default: ZombieConfig } = await import(
      "../app/(dashboard)/zombies/[id]/components/ZombieConfig"
    );
    const user = userEvent.setup();
    render(
      React.createElement(ZombieConfig, {
        workspaceId: "ws_1",
        zombieId: "zom_1",
        zombieName: "lead-collector",
      }),
    );

    await user.click(screen.getByRole("button", { name: /delete zombie/i }));
    // Confirm dialog is now visible.
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /yes, delete/i }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        expect.stringContaining("/v1/workspaces/ws_1/zombies/zom_1"),
        expect.objectContaining({ method: "DELETE" }),
      ),
    );
    expect(routerPush).toHaveBeenCalledWith("/zombies");
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("delete flow: missing token blocks the call and surfaces an error", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    const { default: ZombieConfig } = await import(
      "../app/(dashboard)/zombies/[id]/components/ZombieConfig"
    );
    const user = userEvent.setup();
    render(
      React.createElement(ZombieConfig, {
        workspaceId: "ws_1",
        zombieId: "zom_1",
        zombieName: "lead-collector",
      }),
    );
    await user.click(screen.getByRole("button", { name: /delete zombie/i }));
    await user.click(screen.getByRole("button", { name: /yes, delete/i }));
    await waitFor(() =>
      expect(screen.getByText(/Not authenticated/i)).toBeTruthy(),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("delete flow: server error is rendered inline without navigating", async () => {
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 409,
      statusText: "Conflict",
      json: async () => ({ error: "run in progress", code: "UZ-ZOM-004" }),
    });
    const { default: ZombieConfig } = await import(
      "../app/(dashboard)/zombies/[id]/components/ZombieConfig"
    );
    const user = userEvent.setup();
    render(
      React.createElement(ZombieConfig, {
        workspaceId: "ws_1",
        zombieId: "zom_1",
        zombieName: "lead-collector",
      }),
    );
    await user.click(screen.getByRole("button", { name: /delete zombie/i }));
    await user.click(screen.getByRole("button", { name: /yes, delete/i }));
    await waitFor(() =>
      expect(screen.getByText(/run in progress/i)).toBeTruthy(),
    );
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("delete flow: Cancel dismisses the confirm dialog", async () => {
    const { default: ZombieConfig } = await import(
      "../app/(dashboard)/zombies/[id]/components/ZombieConfig"
    );
    const user = userEvent.setup();
    render(
      React.createElement(ZombieConfig, {
        workspaceId: "ws_1",
        zombieId: "zom_1",
        zombieName: "lead-collector",
      }),
    );
    await user.click(screen.getByRole("button", { name: /delete zombie/i }));
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("FirewallRulesEditor", () => {
  it("renders the CLI-only placeholder", async () => {
    const { default: FirewallRulesEditor } = await import(
      "../app/(dashboard)/zombies/[id]/components/FirewallRulesEditor"
    );
    render(React.createElement(FirewallRulesEditor));
    expect(screen.getByText(/Firewall editing is CLI-only/i)).toBeTruthy();
    expect(screen.getByText(/zombiectl zombie firewall set/i)).toBeTruthy();
  });
});

describe("InstallZombieForm interactions", () => {
  async function renderForm() {
    const { default: Form } = await import(
      "../app/(dashboard)/zombies/new/InstallZombieForm"
    );
    return render(React.createElement(Form, { workspaceId: "ws_1" }));
  }

  it("empty name blocks submit and shows the required-field error", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.click(screen.getByRole("button", { name: /install zombie/i }));
    await waitFor(() =>
      expect(screen.getByText(/Zombie name is required/i)).toBeTruthy(),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("empty SKILL.md blocks submit and shows the required-field error", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^Name$/), "lead-collector");
    await user.click(screen.getByRole("button", { name: /install zombie/i }));
    await waitFor(() =>
      expect(screen.getByText(/SKILL\.md body is required/i)).toBeTruthy(),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("empty config JSON blocks submit and shows the required-field error", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^Name$/), "lead-collector");
    await user.type(screen.getByLabelText(/SKILL\.md body/i), "hello");
    await user.click(screen.getByRole("button", { name: /install zombie/i }));
    await waitFor(() =>
      expect(screen.getByText(/Config JSON is required/i)).toBeTruthy(),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("invalid config JSON blocks submit with a parse error", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^Name$/), "lead-collector");
    await user.type(screen.getByLabelText(/SKILL\.md body/i), "hello");
    await user.type(screen.getByLabelText(/Config JSON/i), "not-json");
    await user.click(screen.getByRole("button", { name: /install zombie/i }));
    await waitFor(() =>
      expect(screen.getByText(/Config JSON must be valid JSON/i)).toBeTruthy(),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("happy path: fills form, POSTs, redirects to detail page", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => ({ zombie_id: "zom_new", status: "active" }),
    });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^Name$/), "lead-collector");
    await user.type(screen.getByLabelText(/SKILL\.md body/i), "# skill body");
    await user.type(screen.getByLabelText(/Config JSON/i), '{{"name":"x"}');
    await user.click(screen.getByRole("button", { name: /install zombie/i }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        expect.stringContaining("/v1/workspaces/ws_1/zombies"),
        expect.objectContaining({ method: "POST" }),
      ),
    );
    const callBody = JSON.parse(
      (fetchMock.mock.calls[0]![1] as RequestInit).body as string,
    );
    expect(callBody).toEqual({
      name: "lead-collector",
      source_markdown: "# skill body",
      config_json: '{"name":"x"}',
    });
    expect(routerPush).toHaveBeenCalledWith("/zombies/zom_new");
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("409 conflict renders the 'already exists' error", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 409,
      statusText: "Conflict",
      json: async () => ({ error: "dup", code: "UZ-ZOM-002" }),
    });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^Name$/), "lead-collector");
    await user.type(screen.getByLabelText(/SKILL\.md body/i), "# skill");
    await user.type(screen.getByLabelText(/Config JSON/i), "{{}");
    await user.click(screen.getByRole("button", { name: /install zombie/i }));
    await waitFor(() =>
      expect(screen.getByText(/already exists in this workspace/i)).toBeTruthy(),
    );
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("non-409 errors render the raw error message", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 500,
      statusText: "Server Error",
      json: async () => ({ error: "boom", code: "UZ-SRV" }),
    });
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^Name$/), "lead-collector");
    await user.type(screen.getByLabelText(/SKILL\.md body/i), "# skill");
    await user.type(screen.getByLabelText(/Config JSON/i), "{{}");
    await user.click(screen.getByRole("button", { name: /install zombie/i }));
    await waitFor(() =>
      expect(screen.getByText(/boom/)).toBeTruthy(),
    );
  });

  it("missing token surfaces Not authenticated", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    const user = userEvent.setup();
    await renderForm();
    await user.type(screen.getByLabelText(/^Name$/), "lead-collector");
    await user.type(screen.getByLabelText(/SKILL\.md body/i), "# skill");
    await user.type(screen.getByLabelText(/Config JSON/i), "{{}");
    await user.click(screen.getByRole("button", { name: /install zombie/i }));
    await waitFor(() =>
      expect(screen.getByText(/Not authenticated/i)).toBeTruthy(),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("Cancel button navigates back to /zombies", async () => {
    const user = userEvent.setup();
    await renderForm();
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(routerPush).toHaveBeenCalledWith("/zombies");
  });
});
