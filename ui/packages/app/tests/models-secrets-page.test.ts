import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// Server-component page test for the Models page (M121: the registry
// table replaced the switch list). The data layer (reads.ts → tenant model
// entries / secrets) and the heavy client child (ModelsRegistryTable, which
// owns the DataTable + dialogs) are mocked at module boundaries, so this
// asserts the page's composition only: title/description + the registry
// table mounted under the catalogue provider.

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const auth = vi.fn();
const listTenantModelEntriesCached = vi.fn();
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
  listTenantModelEntriesCached,
  listSecretsCached,
}));

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider", () => ({
  ModelCatalogueProvider: ({ children }: React.PropsWithChildren) =>
    React.createElement("div", { "data-catalogue-provider": "1" }, children),
}));
// The page's own contract is just: pass workspaceId/initial/secrets through
// to ModelsRegistryTable, which owns rendering the table + dialogs internally.
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable", () => ({
  default: ({ workspaceId, initial }: { workspaceId: string; initial: { models: unknown[] } }) =>
    React.createElement("div", {
      "data-testid": "models-registry-table",
      "data-workspace": workspaceId,
      "data-entry-count": initial.models.length,
    }),
}));

const WORKSPACE_ID = "ws_1";
function renderPage(Page: (args: { params: Promise<{ workspaceId: string }> }) => Promise<React.ReactElement>) {
  return Page({ params: Promise.resolve({ workspaceId: WORKSPACE_ID }) });
}

function registryList(count: number) {
  return {
    models: Array.from({ length: count }, (_, i) => ({
      id: `entry_${i}`,
      model_id: `model-${i}`,
      secret_ref: "anthropic-prod",
      kind: "provider_key",
      has_key: true,
      active: i === 0,
      created_at: 1_777_507_200_000,
    })),
    platform_default_available: true,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_123") });
});
afterEach(() => vi.clearAllMocks());

describe("Models page", () => {
  it("composes the registry table under the catalogue provider", async () => {
    listTenantModelEntriesCached.mockResolvedValue(registryList(2));
    listSecretsCached.mockResolvedValue({
      secrets: [{ kind: "provider_key", name: "anthropic-prod", created_at: 1_777_507_200_000, provider: "anthropic" }],
    });

    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    const markup = renderToStaticMarkup(await renderPage(Page));

    expect(markup).toContain("Models");
    expect(markup).toContain("The model your fleets run on, and the key behind it.");
    expect(markup).toContain('data-catalogue-provider="1"');
    expect(markup).toContain('data-testid="models-registry-table"');
    expect(markup).toContain('data-entry-count="2"');
  });

  it("degrades to an empty registry when the entries fetch fails", async () => {
    listTenantModelEntriesCached.mockRejectedValue(new Error("503"));
    listSecretsCached.mockResolvedValue({ secrets: [] });

    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    const markup = renderToStaticMarkup(await renderPage(Page));

    expect(markup).toContain('data-testid="models-registry-table"');
    expect(markup).toContain('data-entry-count="0"');
  });

  it("still renders the registry table when listSecrets errors", async () => {
    listTenantModelEntriesCached.mockResolvedValue(registryList(1));
    listSecretsCached.mockRejectedValue(new Error("503"));

    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    const markup = renderToStaticMarkup(await renderPage(Page));

    expect(markup).toContain('data-testid="models-registry-table"');
    expect(markup).toContain('data-entry-count="1"');
  });

  it("redirects to /sign-in when unauthenticated", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    await expect(renderPage(Page)).rejects.toThrow("redirect:/sign-in");
  });
});
