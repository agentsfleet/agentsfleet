import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MODELS_PAGE_DESCRIPTION } from "../app/(dashboard)/w/[workspaceId]/settings/models/copy";

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

vi.mock("next/navigation", () => ({
  redirect,
  useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));
vi.mock("@clerk/nextjs/server", () => ({ auth }));

// The page's data reads come from the cache()-wrapped helpers; mock those rather
// than the underlying API so the React `cache()` primitive isn't exercised here
// (it has its own direct test in tests/reads-cache.test.ts).
// No `listSecretsCached`: the page no longer reads the secret
// list on an ordinary visit, so there is nothing here to mock.
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/lib/reads", () => ({
  listTenantModelEntriesCached,
}));

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider", () => ({
  ModelCatalogueProvider: ({ children }: React.PropsWithChildren) =>
    React.createElement("div", { "data-catalogue-provider": "1" }, children),
}));
// The page's own job is just: pass workspaceId + the first page (or the typed
// read error) through to ModelsRegistryTable, which owns the table + dialogs.
// `data-error-kind` is what lets these tests tell a FAILED read from an EMPTY
// one — the distinction this change restores.
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable", () => ({
  default: ({
    workspaceId,
    initialPage,
    initialError,
  }: {
    workspaceId: string;
    initialPage: { models: unknown[] } | null;
    initialError: { kind: string } | null;
  }) =>
    React.createElement("div", {
      "data-testid": "models-registry-table",
      "data-workspace": workspaceId,
      "data-entry-count": initialPage ? initialPage.models.length : -1,
      "data-error-kind": initialError ? initialError.kind : "",
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

    const { default: Page, ModelsRegistryData } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/page"
    );
    // The shell paints the header immediately; the registry is an async child
    // so renderToStaticMarkup renders the skeleton in its place.
    const shell = renderToStaticMarkup(await renderPage(Page));
    expect(shell).toContain("Models");
    expect(shell).toContain(MODELS_PAGE_DESCRIPTION);
    expect(shell).not.toContain('data-testid="models-registry-table"');

    const markup = renderToStaticMarkup(
      React.createElement(React.Fragment, null, await ModelsRegistryData({ workspaceId: WORKSPACE_ID })),
    );
    expect(markup).toContain('data-catalogue-provider="1"');
    expect(markup).toContain('data-testid="models-registry-table"');
    expect(markup).toContain('data-entry-count="2"');
    // A successful read carries no error — the two are mutually exclusive.
    expect(markup).toContain('data-error-kind=""');
  });

  it("surfaces a typed read error rather than an empty registry when the fetch fails", async () => {
    // This replaces a test that asserted the opposite. The page used to
    // `.catch(() => EMPTY_REGISTRY)`, so a tenant whose models were merely
    // unreachable was told they had none — no distinction, no next step, no
    // retry. A failed read is now a failure, not a fact about the registry.
    listTenantModelEntriesCached.mockRejectedValue(new Error("503"));

    const { ModelsRegistryData } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    const markup = renderToStaticMarkup(
      React.createElement(React.Fragment, null, await ModelsRegistryData({ workspaceId: WORKSPACE_ID })),
    );

    expect(markup).toContain('data-testid="models-registry-table"');
    // -1 is the mock's "no page at all", NOT a zero-length page.
    expect(markup).toContain('data-entry-count="-1"');
    expect(markup).toContain('data-error-kind="unknown"');
  });

  it("does not read the secret list on an ordinary visit", async () => {
    // The secret list used to load in parallel on every visit to
    // seed a picker most visits never open. The read module no longer exports
    // a secrets wrapper at all, so this asserts the page renders without one.
    listTenantModelEntriesCached.mockResolvedValue(registryList(1));

    const { ModelsRegistryData } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    const markup = renderToStaticMarkup(
      React.createElement(React.Fragment, null, await ModelsRegistryData({ workspaceId: WORKSPACE_ID })),
    );

    expect(markup).toContain('data-testid="models-registry-table"');
    expect(markup).toContain('data-entry-count="1"');
  });

  it("redirects to /sign-in when unauthenticated", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    await expect(renderPage(Page)).rejects.toThrow("redirect:/sign-in");
  });

  it("the streamed registry renders nothing when the session lapsed after the shell flushed", async () => {
    // The shell redirects on a missing token, but the registry is a SEPARATE
    // async child that mints its own. By the time it runs, the header has
    // already gone to the browser — so a lapsed session yields no table rather
    // than an unauthenticated read, and it cannot redirect from here.
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });

    const { ModelsRegistryData } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/page");
    const markup = renderToStaticMarkup(
      React.createElement(React.Fragment, null, await ModelsRegistryData({ workspaceId: WORKSPACE_ID })),
    );

    expect(markup).toBe("");
    expect(listTenantModelEntriesCached).not.toHaveBeenCalled();
  });
});
