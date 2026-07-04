import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// Server-component page test for the standalone /secrets page — Secrets &
// ENVs is its own page again, not a Models section. The data layer
// (reads.ts → listSecretsCached / getTenantProviderCached) and the two client
// children (SecretsList, AddSecretDialog) are mocked at module boundaries, so
// this asserts the page's own composition only: title/description, the
// resolved secrets + delete-protection guard passed through to the list, and
// the empty-workspace/unauthenticated branches.

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const auth = vi.fn();
const listSecretsCached = vi.fn();
const getTenantProviderCached = vi.fn();

vi.mock("next/navigation", () => ({
  redirect,
  useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));
vi.mock("@clerk/nextjs/server", () => ({ auth }));

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

vi.mock("@/app/(dashboard)/secrets/lib/reads", () => ({ listSecretsCached, getTenantProviderCached }));

// The page's own contract is just: pass workspaceId/secrets/protectedSecretName
// through to SecretsList, and workspaceId through to AddSecretDialog.
vi.mock("@/app/(dashboard)/secrets/components/SecretsList", () => ({
  default: ({
    workspaceId,
    secrets,
    protectedSecretName,
  }: {
    workspaceId: string;
    secrets: Array<{ name: string }>;
    protectedSecretName?: string | null;
  }) =>
    React.createElement("div", {
      "data-testid": "secrets-list",
      "data-workspace": workspaceId,
      "data-secret-count": secrets.length,
      "data-protected-secret": protectedSecretName ?? "",
    }),
}));
vi.mock("@/app/(dashboard)/secrets/components/AddSecretDialog", () => ({
  default: ({ workspaceId }: { workspaceId: string }) =>
    React.createElement("div", { "data-testid": "add-secret-dialog", "data-workspace": workspaceId }),
}));

vi.mock("lucide-react", () => {
  const make = (name: string) => (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": name });
  return { ZapIcon: make("ZapIcon") };
});

import { withWorkspaceScope } from "@/lib/workspace";
import { PROVIDER_MODE } from "@/lib/types";

function selfManagedProvider(secretRef: string | null) {
  return {
    mode: PROVIDER_MODE.self_managed,
    provider: "anthropic",
    model: "claude-sonnet-4-6",
    context_cap_tokens: 256000,
    secret_ref: secretRef,
  };
}

function platformProvider() {
  return {
    mode: PROVIDER_MODE.platform,
    provider: "fireworks",
    model: "accounts/fireworks/models/kimi-k2.6",
    context_cap_tokens: 128000,
    secret_ref: null,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_123") });
  vi.mocked(withWorkspaceScope).mockImplementation(
    async (_token: string, fn: (workspaceId: string) => Promise<unknown>) => fn("ws_1"),
  );
  // Default: platform mode, no protected secret — most tests below override
  // listSecretsCached only and don't care about the provider branch.
  getTenantProviderCached.mockResolvedValue(platformProvider());
});
afterEach(() => vi.clearAllMocks());

describe("Secrets page", () => {
  it("composes the list and the add-dialog with the resolved secrets", async () => {
    listSecretsCached.mockResolvedValue({
      secrets: [
        { kind: "custom_secret", name: "stripe", created_at: 1_777_507_200_000 },
        { kind: "custom_secret", name: "github", created_at: 1_777_507_300_000 },
      ],
    });

    const { default: Page } = await import("../app/(dashboard)/secrets/page");
    const markup = renderToStaticMarkup(await Page());

    expect(markup).toContain("Secrets &amp; ENVs");
    expect(markup).toContain("Encrypted secrets your fleets can use");
    expect(markup).toContain('data-testid="add-secret-dialog"');
    expect(markup).toContain('data-workspace="ws_1"');
    expect(markup).toContain('data-testid="secrets-list"');
    expect(markup).toContain('data-secret-count="2"');
  });

  it("falls back to an empty secrets list when listSecretsCached rejects", async () => {
    listSecretsCached.mockRejectedValue(new Error("503"));

    const { default: Page } = await import("../app/(dashboard)/secrets/page");
    const markup = renderToStaticMarkup(await Page());

    expect(markup).toContain('data-testid="secrets-list"');
    expect(markup).toContain('data-secret-count="0"');
  });

  it("passes the self-managed provider's secret_ref as protectedSecretName", async () => {
    listSecretsCached.mockResolvedValue({
      secrets: [{ kind: "provider_key", name: "anthropic-prod", created_at: 1 }],
    });
    getTenantProviderCached.mockResolvedValue(selfManagedProvider("anthropic-prod"));

    const { default: Page } = await import("../app/(dashboard)/secrets/page");
    const markup = renderToStaticMarkup(await Page());

    // The secret backing the active model can't be deleted from this page —
    // regression pin for the delete-protection guard being wired at all.
    expect(markup).toContain('data-protected-secret="anthropic-prod"');
  });

  it("passes no protectedSecretName when the provider is platform-managed", async () => {
    listSecretsCached.mockResolvedValue({ secrets: [] });
    getTenantProviderCached.mockResolvedValue(platformProvider());

    const { default: Page } = await import("../app/(dashboard)/secrets/page");
    const markup = renderToStaticMarkup(await Page());

    expect(markup).toContain('data-protected-secret=""');
  });

  it("degrades to no protectedSecretName when the provider fetch fails", async () => {
    listSecretsCached.mockResolvedValue({
      secrets: [{ kind: "custom_secret", name: "stripe", created_at: 1 }],
    });
    getTenantProviderCached.mockRejectedValue(new Error("503"));

    const { default: Page } = await import("../app/(dashboard)/secrets/page");
    const markup = renderToStaticMarkup(await Page());

    expect(markup).toContain('data-testid="secrets-list"');
    expect(markup).toContain('data-protected-secret=""');
  });

  it("renders the no-workspace empty state under the Secrets & ENVs title", async () => {
    vi.mocked(withWorkspaceScope).mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/secrets/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("Secrets &amp; ENVs");
    expect(markup).toContain("No workspace yet");
  });

  it("redirects to /sign-in when unauthenticated", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/secrets/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });
});
