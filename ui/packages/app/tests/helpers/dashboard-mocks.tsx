import React from "react";
import { vi } from "vitest";

// Shared test harness for the dashboard's heavy-mock test files. `vi.mock` is
// hoisted per-file, so each shard still declares its own `vi.mock(...)` calls —
// but the factory bodies and the shared mock-fn instances live here once. Shards
// delegate via `vi.mock("mod", async () => (await import("./helpers/dashboard-mocks")).fooMock())`;
// the dynamic import resolves to the same module instance as the shard's static
// import (vitest isolates the registry per test file), so the fn a test asserts
// on is the same instance the factory installs.

// ── Shared mock fns (one fresh set per importing test file) ─────────────────
export const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
export const notFound = vi.fn(() => {
  throw new Error("notFound");
});
export const usePathname = vi.fn(() => "/");
export const routerPush = vi.fn();
export const routerRefresh = vi.fn();
export const authMock = vi.fn();
export const getTokenFn = vi.fn().mockResolvedValue("token_abc");
export const listTenantWorkspacesCached = vi.fn().mockResolvedValue({ items: [], total: 0 });
export const fetchMock = vi.fn();
export const clipboardWriteText = vi.fn().mockResolvedValue(undefined);

// ── Module factories (delegated to from each shard's vi.mock call) ──────────
export function nextNavigationMock() {
  return { redirect, notFound, usePathname, useRouter: () => ({ push: routerPush, refresh: routerRefresh }) };
}

export function nextLinkMock() {
  return {
    default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
      React.createElement("a", { href, ...rest }, children),
  };
}

export function clerkMock() {
  return {
    useAuth: () => ({ getToken: getTokenFn }),
    useUser: () => ({ isLoaded: true, isSignedIn: true, user: null }),
    ClerkProvider: ({ children }: { children: React.ReactNode }) => React.createElement(React.Fragment, null, children),
    UserButton: () => React.createElement("div", { "data-user-button": "1" }),
    SignIn: () => React.createElement("div", { "data-sign-in": "1" }),
    SignUp: () => React.createElement("div", { "data-sign-up": "1" }),
  };
}

export function clerkServerMock() {
  return { auth: authMock };
}

// `@/lib/workspace` mock (M118). The module is slimmed to a single survivor —
// `listTenantWorkspacesCached` — now that the active workspace is an explicit
// URL segment rather than a resolved cookie/claim. Pages read
// `params.workspaceId`; the switcher/nav derive it from the route.
export function workspaceMock() {
  return {
    listTenantWorkspacesCached,
  };
}

// Union of every lucide icon the dashboard test files reference. Each renders a
// stub <svg data-icon="…"> so name-based queries keep working.
const LUCIDE_ICONS = [
  "AlertTriangleIcon", "CheckIcon", "CopyIcon", "Loader2Icon", "PlusIcon", "ShieldIcon",
  "KeyRoundIcon", "KeyIcon", "Trash2Icon", "ChevronDownIcon", "ChevronRightIcon", "SettingsIcon",
  "WalletIcon", "ZapIcon", "ReceiptIcon", "CreditCardIcon", "ActivityIcon", "CpuIcon", "CoinsIcon",
  "SlidersHorizontalIcon", "PencilIcon", "GitPullRequestIcon", "BriefcaseIcon", "HashIcon",
  "BotIcon", "LinkIcon", "CheckCircle2Icon", "CircleHelpIcon", "LayoutTemplateIcon",
] as const;

export function lucideMock() {
  const icon = (name: string) => {
    const C = (p: Record<string, unknown>) => React.createElement("svg", { ...p, "data-icon": name });
    C.displayName = name;
    return C;
  };
  return Object.fromEntries(LUCIDE_ICONS.map((n) => [n, icon(n)]));
}

// Design-system core shared by both files. Superset behavior: Button carries
// `data-variant`/`data-size`; ConfirmDialog exposes both `data-confirm-dialog`
// and `data-confirm` plus the optional description — so whichever attribute a
// given file's assertions read is present.
type ConfirmDialogProps = {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  title: string;
  description?: React.ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  onConfirm: () => void | Promise<void>;
  errorMessage?: string | null;
  onError?: (e: unknown) => void;
};

function ConfirmDialogMock({ open, onOpenChange, title, description, confirmLabel = "Confirm", cancelLabel = "Cancel", onConfirm, errorMessage, onError }: ConfirmDialogProps) {
  if (!open) return null;
  return React.createElement(
    "div",
    { role: "alertdialog", "data-confirm-dialog": title, "data-confirm": title },
    React.createElement("div", { key: "title" }, title),
    description ? React.createElement("div", { key: "desc" }, description) : null,
    errorMessage ? React.createElement("div", { key: "err", role: "alert" }, errorMessage) : null,
    React.createElement("button", { key: "cancel", type: "button", onClick: () => onOpenChange(false) }, cancelLabel),
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
  );
}

export function designSystemCore(actual: Record<string, unknown>) {
  return {
    ...actual,
    buttonClassName: (variant: string, size: string) => `btn-${variant}-${size}`,
    Button: ({ children, variant, size, ...rest }: { children: React.ReactNode; variant?: string; size?: string } & React.ButtonHTMLAttributes<HTMLButtonElement>) =>
      React.createElement("button", { "data-variant": variant, "data-size": size, ...rest }, children),
    ConfirmDialog: ConfirmDialogMock,
    EmptyState: ({ title, description, icon, action }: { title: string; description?: string; icon?: React.ReactNode; action?: React.ReactNode }) =>
      React.createElement("div", { "data-empty-state": title }, icon, description, action),
    Input: (props: React.InputHTMLAttributes<HTMLInputElement>) => React.createElement("input", props),
    PageHeader: ({ children }: { children: React.ReactNode }) => React.createElement("header", { "data-page-header": "1" }, children),
    PageTitle: ({ children }: { children: React.ReactNode }) => React.createElement("h1", { "data-page-title": "1" }, children),
    SectionLabel: ({ children }: { children: React.ReactNode }) => React.createElement("h2", { "data-section-label": "1" }, children),
  };
}

// Tabs family + Textarea — used by the fleets test shards (FleetConfig).
const TabsCtx = React.createContext<{ active: string; setActive: (v: string) => void }>({ active: "", setActive: () => {} });

export function designSystemTabs() {
  return {
    Textarea: (props: React.TextareaHTMLAttributes<HTMLTextAreaElement>) => React.createElement("textarea", props),
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
      return React.createElement(
        "button",
        { role: "tab", "data-tab-value": value, "data-state": ctx.active === value ? "active" : "inactive", type: "button", onClick: () => ctx.setActive(value), ...rest },
        children,
      );
    },
    TabsContent: ({ children, value, ...rest }: { children: React.ReactNode; value: string } & React.HTMLAttributes<HTMLDivElement>) => {
      const ctx = React.useContext(TabsCtx);
      if (ctx.active !== value) return null;
      return React.createElement("div", { role: "tabpanel", "data-tab-value": value, ...rest }, children);
    },
  };
}

// DropdownMenu family + Skeleton + StatusCard — used by the dashboard test shards.
export function designSystemDropdown() {
  return {
    Skeleton: ({ className }: { className?: string }) => React.createElement("div", { "data-skeleton": "1", className }),
    StatusCard: ({ label, count, variant }: { label: string; count: number | string; variant?: string }) =>
      React.createElement("div", { "data-status-card": label, "data-variant": variant ?? "default" }, `${label}: ${count}`),
    DropdownMenu: ({ children }: { children: React.ReactNode }) => React.createElement("div", { "data-dropdown": "1" }, children),
    DropdownMenuTrigger: ({ children, ...rest }: React.ButtonHTMLAttributes<HTMLButtonElement> & { children: React.ReactNode }) =>
      React.createElement("button", { ...rest }, children),
    DropdownMenuContent: ({ children, ...rest }: { children: React.ReactNode } & React.HTMLAttributes<HTMLDivElement>) =>
      React.createElement("div", { "data-dropdown-content": "1", ...rest }, children),
    DropdownMenuLabel: ({ children }: { children: React.ReactNode }) => React.createElement("div", { "data-dropdown-label": "1" }, children),
    DropdownMenuSeparator: () => React.createElement("hr", { "data-dropdown-separator": "1" }),
    DropdownMenuItem: ({ children, onSelect, ...rest }: { children: React.ReactNode; onSelect?: () => void } & React.HTMLAttributes<HTMLDivElement>) =>
      React.createElement("div", { role: "menuitem", onClick: () => onSelect?.(), ...rest }, children),
  };
}

// Configure the clerk-server `auth()` mock return for a single call.
export function mockAuthOnce(opts: { token?: string | null; userId?: string | null } = {}) {
  const token = opts.token === undefined ? "token_abc" : opts.token;
  const userId = opts.userId === undefined ? "usr_1" : opts.userId;
  authMock.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(token), userId, sessionClaims: null });
}

// Re-apply default return values after `vi.clearAllMocks()` in beforeEach.
export function resetCommonMocks(opts: { pathname?: string } = {}) {
  usePathname.mockReturnValue(opts.pathname ?? "/");
  getTokenFn.mockResolvedValue("token_abc");
  authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_abc") });
  listTenantWorkspacesCached.mockResolvedValue({ items: [], total: 0 });
}
