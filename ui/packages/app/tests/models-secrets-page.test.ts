import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// Server-component page test for the Models page. The data layer
// (reads.ts → tenant_provider / secrets) and the heavy client child
// (ProviderSwitchList, which owns the active-model hero row + catalogue) are
// mocked at module boundaries, so this asserts the page's composition only:
// title/description + the switch list mounted under the catalogue provider.
// Secrets is its own page now — this
// page no longer renders any secrets-vault content.

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const auth = vi.fn();
const getTenantProviderCached = vi.fn();
const listSecretsCached = vi.fn();

vi.mock("next/navigation", () => ({
  redirect,
  useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));
vi.mock("@clerk/nextjs/server", () => ({ auth }));

// The page's data reads come from the cache()-wrapped helpers; mock those rather
// than the underlying API so the React `cache()` primitive isn't exercised here
// (it has its own direct test in tests/reads-cache.test.ts).
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/lib/reads", () => ({
  getTenantProviderCached,
  listSecretsCached,
}));

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider", () => ({
  ModelCatalogueProvider: ({ children }: React.PropsWithChildren) =>
    React.createElement("div", { "data-catalogue-provider": "1" }, children),
}));
// The page's own contract is just: pass workspaceId/provider/secrets through
// to ProviderSwitchList, which owns rendering the hero row internally.
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderSwitchList", () => ({
  default: ({ workspaceId, provider }: { workspaceId: string; provider: unknown }) =>
    React.createElement("div", {
      "data-testid": "provider-switch-list",
      "data-workspace": workspaceId,
      "data-provider": provider === null ? "null" : "present",
    }),
}));

vi.mock("lucide-react", () => {
  const make = (name: string) => (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": name });
  return { ZapIcon: make("ZapIcon") };
});

import { PROVIDER_MODE } from "@/lib/types";

// The workspace id now comes from the route param; the page reads it from
// `params` and forwards it to its data reads.
const WORKSPACE_ID = "ws_1";
function renderPage(Page: (args: { params: Promise<{ workspaceId: string }> }) => Promise<React.ReactElement>) {
  return Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID }) });
}

const ANTHROPIC_SECRET_NAME = "anthropic-prod";

function selfManagedProvider() {
  return {
    mode: PROVIDER_MODE.self_managed,
    provider: "anthropic",
    model: "claude-sonnet-4-6",
    context_cap_tokens: 256000,
    secret_ref: ANTHROPIC_SECRET_NAME,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_123") });
});
afterEach(() => vi.clearAllMocks());

describe("Models page", () => {
  it("composes the switch list under the catalogue provider", async () => {
    getTenantProviderCached.mockResolvedValue(selfManagedProvider());
    listSecretsCached.mockResolvedValue({
      secrets: [
        { kind: "provider_key", name: ANTHROPIC_SECRET_NAME, created_at: 1_777_507_200_000, provider: "anthropic", model: "claude-sonnet-4-6" },
      ],
    });

    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    const markup = renderToStaticMarkup(await renderPage(Page));

    // Title + description.
    expect(markup).toContain("Models");
    expect(markup).toContain("The model your fleets run on, and the key behind it.");

    // The switch list (which owns the active-model row internally) mounts
    // inside the catalogue provider.
    expect(markup).toContain('data-catalogue-provider="1"');
    expect(markup).toContain('data-testid="provider-switch-list"');
    expect(markup).toContain('data-provider="present"');
  });

  it("degrades the hero to the platform-default view when the provider fetch fails", async () => {
    getTenantProviderCached.mockRejectedValue(new Error("503"));
    listSecretsCached.mockResolvedValue({ secrets: [] });

    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    const markup = renderToStaticMarkup(await renderPage(Page));

    // Provider error → `provider` is null; the page still renders, passing
    // null through to the switch list (which degrades its hero row to DEFAULT).
    expect(markup).toContain('data-testid="provider-switch-list"');
    expect(markup).toContain('data-provider="null"');
  });

  it("still renders the switch list when listSecrets errors", async () => {
    getTenantProviderCached.mockResolvedValue(selfManagedProvider());
    listSecretsCached.mockRejectedValue(new Error("503"));

    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    const markup = renderToStaticMarkup(await renderPage(Page));

    expect(markup).toContain('data-testid="provider-switch-list"');
    expect(markup).toContain('data-provider="present"');
  });

  it("redirects to /sign-in when unauthenticated", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    await expect(renderPage(Page)).rejects.toThrow("redirect:/sign-in");
  });
});
