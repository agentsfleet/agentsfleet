import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// Server-component page test for the consolidated Models & Keys page.
// The data layer (reads.ts → tenant_provider / credentials) and the heavy client
// children (hero, switch list, catalogue provider, custom-secrets list, add form)
// are mocked at module boundaries, so this asserts the page's composition: the
// hero + switch list under the catalogue provider, and a custom-secrets section
// fed only by `customSecretsOf` — never a provider key. The old /credentials
// vault page is now a bare redirect, covered in dashboard-placeholder.test.ts.

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const auth = vi.fn();
const getTenantProviderCached = vi.fn();
const listCredentialsCached = vi.fn();

vi.mock("next/navigation", () => ({
  redirect,
  useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));
vi.mock("@clerk/nextjs/server", () => ({ auth }));

// Stub the scope wrapper; provide a self-contained `orFallback` that mirrors the
// real one (rethrow a 403/404, degrade everything else). Not importOriginal —
// the real module pulls in clerk/next-headers which collides with hoisting.
vi.mock("@/lib/workspace", () => ({
  withWorkspaceScope: vi.fn(),
  orFallback:
    <T,>(fallback: T) =>
    (err: unknown): T => {
      if (
        err &&
        typeof err === "object" &&
        "status" in err &&
        ((err as { status: number }).status === 403 || (err as { status: number }).status === 404)
      )
        throw err;
      return fallback;
    },
}));

// The page's data reads come from the cache()-wrapped helpers; mock those rather
// than the underlying API so the React `cache()` primitive isn't exercised here
// (it has its own direct test in tests/reads-cache.test.ts).
vi.mock("@/app/(dashboard)/settings/models/lib/reads", () => ({
  getTenantProviderCached,
  listCredentialsCached,
}));

// Keep `customSecretsOf` + CREDENTIAL_KIND real so the page's classification (a
// provider key must never reach the custom-secrets section) is genuinely tested.
vi.mock("@/lib/api/credentials", async (orig) => {
  const actual = await orig<typeof import("@/lib/api/credentials")>();
  return { ...actual };
});

// Mock the interactive children: the page test asserts composition + which
// credentials reach which surface, not the children's internals.
vi.mock("@/app/(dashboard)/settings/models/components/ModelCatalogueProvider", () => ({
  ModelCatalogueProvider: ({ children }: React.PropsWithChildren) =>
    React.createElement("div", { "data-catalogue-provider": "1" }, children),
}));
vi.mock("@/app/(dashboard)/settings/models/components/ActiveModelHero", () => ({
  default: ({ workspaceId, provider }: { workspaceId: string; provider: unknown }) =>
    React.createElement("div", {
      "data-testid": "active-model-hero",
      "data-workspace": workspaceId,
      "data-provider": provider === null ? "null" : "present",
    }),
}));
vi.mock("@/app/(dashboard)/settings/models/components/ProviderSwitchList", () => ({
  default: ({ workspaceId }: { workspaceId: string }) =>
    React.createElement("div", { "data-testid": "provider-switch-list", "data-workspace": workspaceId }),
}));
vi.mock("@/app/(dashboard)/credentials/components/CustomSecretsList", () => ({
  default: ({ workspaceId, secrets }: { workspaceId: string; secrets: { name: string }[] }) =>
    React.createElement(
      "div",
      { "data-custom-secrets-list": workspaceId },
      ...secrets.map((s) => React.createElement("span", { key: s.name }, s.name)),
    ),
}));
vi.mock("@/components/domain/island-dynamic/AddCredentialFormDynamic", () => ({
  default: ({ workspaceId }: { workspaceId: string }) =>
    React.createElement("div", { "data-add-credential-form": workspaceId }),
}));

vi.mock("lucide-react", () => {
  const make = (name: string) => (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": name });
  return { ZapIcon: make("ZapIcon") };
});

import { withWorkspaceScope } from "@/lib/workspace";
import { CREDENTIAL_KIND } from "@/lib/api/credentials";
import { PROVIDER_MODE } from "@/lib/types";

const ANTHROPIC_CREDENTIAL_NAME = "anthropic-prod";
const STRIPE_SECRET_NAME = "STRIPE_API_KEY";
const CREATED_AT_MS = 1_777_507_200_000;

function selfManagedProvider() {
  return {
    mode: PROVIDER_MODE.self_managed,
    provider: "anthropic",
    model: "claude-sonnet-4-6",
    context_cap_tokens: 256000,
    credential_ref: ANTHROPIC_CREDENTIAL_NAME,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_123") });
  // Default: a workspace exists — run the page's data fn against `ws_1`.
  vi.mocked(withWorkspaceScope).mockImplementation(
    async (_token: string, fn: (workspaceId: string) => Promise<unknown>) => fn("ws_1"),
  );
});
afterEach(() => vi.clearAllMocks());

describe("Models & Keys page", () => {
  it("composes the hero + switch list under the catalogue, and a custom-secrets section", async () => {
    getTenantProviderCached.mockResolvedValue(selfManagedProvider());
    listCredentialsCached.mockResolvedValue({
      credentials: [
        { kind: CREDENTIAL_KIND.provider_key, name: ANTHROPIC_CREDENTIAL_NAME, created_at: CREATED_AT_MS, provider: "anthropic", model: "claude-sonnet-4-6" },
        { kind: CREDENTIAL_KIND.custom_secret, name: STRIPE_SECRET_NAME, created_at: CREATED_AT_MS },
      ],
    });

    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());

    // Title + description.
    expect(markup).toContain("Models &amp; Keys");
    expect(markup).toContain("The model your fleets run on, and the keys behind it.");

    // Hero + switch list mount inside the catalogue provider.
    expect(markup).toContain('data-catalogue-provider="1"');
    expect(markup).toContain('data-testid="active-model-hero"');
    expect(markup).toContain('data-testid="provider-switch-list"');
    expect(markup).toContain('data-provider="present"');

    // The custom-secrets section is present and lists only the custom secret …
    expect(markup).toContain('data-testid="custom-secrets-group"');
    expect(markup).toContain('data-custom-secrets-list="ws_1"');
    expect(markup).toContain(STRIPE_SECRET_NAME);
    expect(markup).toContain('data-add-credential-form="ws_1"');
    expect(markup).toContain("Add a custom secret");

    // … and NEVER the provider key: it is classified by `kind`, not by name, so
    // it stays out of the custom-secrets surface entirely.
    expect(markup).not.toContain(ANTHROPIC_CREDENTIAL_NAME);
  });

  it("degrades the hero to the platform-default view when the provider fetch fails", async () => {
    getTenantProviderCached.mockRejectedValue(new Error("503"));
    listCredentialsCached.mockResolvedValue({ credentials: [] });

    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());

    // Provider error → `provider` is null; the page still renders, hero gets null.
    expect(markup).toContain('data-testid="active-model-hero"');
    expect(markup).toContain('data-provider="null"');
  });

  it("falls back to an empty vault when listCredentials errors", async () => {
    getTenantProviderCached.mockResolvedValue(selfManagedProvider());
    listCredentialsCached.mockRejectedValue(new Error("503"));

    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());

    // The custom-secrets list still renders (empty) alongside the add form.
    expect(markup).toContain('data-custom-secrets-list="ws_1"');
    expect(markup).toContain('data-add-credential-form="ws_1"');
  });

  it("renders the no-workspace empty state under the Models & Keys title", async () => {
    vi.mocked(withWorkspaceScope).mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("Models &amp; Keys");
    expect(markup).toContain("No workspace yet");
  });

  it("redirects to /sign-in when unauthenticated", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });
});
