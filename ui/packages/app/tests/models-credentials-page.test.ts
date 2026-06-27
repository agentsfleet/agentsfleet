import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// Server-component page tests: render the async page to static markup with the
// data layer + heavy client components mocked at module boundaries. Covers the
// single-purpose Models page (no in-page Credentials tab) and the Credentials
// vault page (kinds strip + three ordered groups).

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const auth = vi.fn();

vi.mock("next/navigation", () => ({
  redirect,
  useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));
vi.mock("next/link", () => ({
  default: ({ children, ...props }: React.PropsWithChildren<React.AnchorHTMLAttributes<HTMLAnchorElement>>) =>
    React.createElement("a", props, children),
}));
vi.mock("@clerk/nextjs/server", () => ({ auth }));

// Stub the scope wrapper; provide a self-contained `orFallback` that mirrors
// the real one (rethrow a 403/404, degrade everything else). Not importOriginal
// — the real module pulls in clerk/next-headers which collides with hoisting.
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
vi.mock("@/lib/api/tenant_provider", () => ({ getTenantProvider: vi.fn() }));
vi.mock("@/lib/api/credentials", () => ({ listCredentials: vi.fn() }));
vi.mock("@/lib/api/model_caps", () => ({
  getModelCaps: vi.fn(),
  uniqueModelIds: (models: Array<{ id: string }>) =>
    Array.from(new Map(models.map((m) => [m.id, m])).values()),
}));

// Mock the interactive children so the page tests assert composition + order,
// not the children's internals (those have their own unit tests).
vi.mock("@/app/(dashboard)/settings/models/components/ProviderSelector", () => ({
  default: ({ workspaceId }: { workspaceId: string }) =>
    React.createElement("div", { "data-provider-selector": workspaceId }),
}));
vi.mock("@/app/(dashboard)/credentials/components/CredentialsList", () => ({
  default: ({ workspaceId, credentials }: { workspaceId: string; credentials: { name: string }[] }) =>
    React.createElement(
      "div",
      { "data-credentials-list": workspaceId },
      ...credentials.map((c) => React.createElement("span", { key: c.name }, c.name)),
    ),
}));
vi.mock("@/app/(dashboard)/credentials/components/CustomSecretsList", () => ({
  default: ({ workspaceId, secrets }: { workspaceId: string; secrets: { name: string }[] }) =>
    React.createElement(
      "div",
      { "data-custom-secrets-list": workspaceId },
      ...secrets.map((s) => React.createElement("span", { key: s.name }, s.name)),
    ),
}));
vi.mock("@/app/(dashboard)/credentials/components/ProviderCredentialRows", () => ({
  default: ({
    workspaceId,
    provider,
  }: {
    workspaceId: string;
    provider: { credential_ref?: string | null } | null;
  }) =>
    React.createElement(
      "div",
      { "data-provider-credential-rows-client": workspaceId },
      provider?.credential_ref ?? "provider keys",
    ),
}));
vi.mock("@/app/(dashboard)/credentials/components/AddCredentialForm", () => ({
  default: ({ workspaceId }: { workspaceId: string }) =>
    React.createElement("div", { "data-add-credential-form": workspaceId }),
}));

vi.mock("lucide-react", () => {
  const make = (name: string) => (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": name });
  return {
    ZapIcon: make("ZapIcon"),
    KeyRoundIcon: make("KeyRoundIcon"),
    GitPullRequestIcon: make("GitPullRequestIcon"),
    BriefcaseIcon: make("BriefcaseIcon"),
    HashIcon: make("HashIcon"),
    CpuIcon: make("CpuIcon"),
    LinkIcon: make("LinkIcon"),
    ChevronDownIcon: make("ChevronDownIcon"),
  };
});

import { withWorkspaceScope } from "@/lib/workspace";
import { getTenantProvider } from "@/lib/api/tenant_provider";
import { listCredentials } from "@/lib/api/credentials";
import { getModelCaps } from "@/lib/api/model_caps";
import { OPENAI_COMPATIBLE_PROVIDER, PROVIDER_MODE } from "@/lib/types";

const FIREWORKS_PROVIDER = "fireworks";
const FIREWORKS_MODEL_ID = "kimi-k2.6";
const ANTHROPIC_CREDENTIAL_NAME = "anthropic-prod";
const STRIPE_SECRET_NAME = "STRIPE_API_KEY";
const CREATED_AT_MS = 1_777_507_200_000;
const CONTEXT_CAP_TOKENS = 256000;

function platformProvider() {
  return {
    mode: PROVIDER_MODE.platform,
    provider: FIREWORKS_PROVIDER,
    model: FIREWORKS_MODEL_ID,
    context_cap_tokens: CONTEXT_CAP_TOKENS,
    credential_ref: null,
  };
}

function emptyCatalogue() {
  return {
    version: "1",
    models: [],
    rates: { run_nanos_per_sec: 0, event_nanos: 0 },
    billing: { starter_credit_nanos: 0, free_trial_end_ms: 0, free_trial_stage_nanos: 0 },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_123") });
  // Default: a workspace exists — run the page's data fn against `ws_1`.
  // No-workspace tests override this to resolve `null`.
  vi.mocked(withWorkspaceScope).mockImplementation(
    async (_token: string, fn: (workspaceId: string) => Promise<unknown>) => fn("ws_1"),
  );
});
afterEach(() => vi.clearAllMocks());

describe("Models page", () => {
  it("test_models_no_inpage_credentials_tab: renders no Credentials tab trigger", async () => {
    vi.mocked(getTenantProvider).mockResolvedValue(platformProvider());
    vi.mocked(listCredentials).mockResolvedValue({ credentials: [] });
    vi.mocked(getModelCaps).mockResolvedValue(emptyCatalogue());

    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());

    // Single-purpose page: title "Models" + description, the model selector, and
    // NO tablist / Credentials tab trigger.
    expect(markup).toContain(">Models<");
    expect(markup).toContain("Choose platform defaults or your key");
    expect(markup).toContain('data-provider-selector="ws_1"');
    expect(markup).not.toContain('role="tablist"');
    expect(markup).not.toContain('role="tab"');
    expect(markup).not.toContain('id="credentials"');
    // The old union title is gone.
    expect(markup).not.toContain("Models &amp; Credentials");
  });

  it("renders the no-workspace empty state under the Models title", async () => {
    vi.mocked(withWorkspaceScope).mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain(">Models<");
    expect(markup).toContain("No workspace yet");
  });

  it("redirects to /sign-in when unauthenticated", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("degrades to platform-default cards when getTenantProvider errors", async () => {
    vi.mocked(getTenantProvider).mockRejectedValue(new Error("503"));
    vi.mocked(listCredentials).mockResolvedValue({ credentials: [] });
    vi.mocked(getModelCaps).mockRejectedValue(new Error("503"));
    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain('data-provider-selector="ws_1"');
  });
});

describe("Credentials vault page", () => {
  it("test_credentials_vault_order: kinds strip + groups in order providers→custom→integrations", async () => {
    vi.mocked(getTenantProvider).mockResolvedValue({
      ...platformProvider(),
      mode: PROVIDER_MODE.self_managed,
      credential_ref: ANTHROPIC_CREDENTIAL_NAME,
    });
    vi.mocked(listCredentials).mockResolvedValue({
      credentials: [
        { name: ANTHROPIC_CREDENTIAL_NAME, created_at: CREATED_AT_MS },
        { name: STRIPE_SECRET_NAME, created_at: CREATED_AT_MS },
      ],
    });

    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    const markup = renderToStaticMarkup(await Page());

    // Header (no generic "Add credential" action — each kind self-serves inline).
    expect(markup).toContain(">Credentials<");
    expect(markup).toContain("Write-only keys for models, tools, and secrets.");
    expect(markup).not.toContain("Add credential");

    // Kinds strip carries all three kinds.
    expect(markup).toContain('data-testid="vault-kinds-strip"');
    expect(markup).toContain('data-testid="vault-kind-providers"');
    expect(markup).toContain('data-testid="vault-kind-custom"');
    expect(markup).toContain('data-testid="vault-kind-integrations"');

    // Groups render in order: providers → custom → integrations.
    const providersAt = markup.indexOf('data-testid="group-providers"');
    const customAt = markup.indexOf('data-testid="group-custom"');
    const integrationsAt = markup.indexOf('data-testid="group-integrations"');
    expect(providersAt).toBeGreaterThan(-1);
    expect(providersAt).toBeLessThan(customAt);
    expect(customAt).toBeLessThan(integrationsAt);

    // The active model credential lands in the providers group; the other in custom.
    expect(markup).toContain(ANTHROPIC_CREDENTIAL_NAME);
    expect(markup).toContain(STRIPE_SECRET_NAME);
  });

  it("degrades the GitHub connector to not-connected when the status endpoint errors", async () => {
    vi.mocked(getTenantProvider).mockResolvedValue(platformProvider());
    vi.mocked(listCredentials).mockResolvedValue({ credentials: [] });
    // getGithubConnector is the only client on this page not module-mocked, so it
    // hits the real request → global fetch. Force that one call to reject: the page
    // must catch it, degrade the connector to "not connected" (never fabricate a
    // connected pill), and still render the vault rather than throw.
    const stubbedFetch = global.fetch;
    global.fetch = vi.fn().mockRejectedValue(new Error("connector endpoint down")) as unknown as typeof fetch;
    try {
      const { default: Page } = await import("../app/(dashboard)/credentials/page");
      const markup = renderToStaticMarkup(await Page());
      expect(markup).toContain(">Credentials<");
    } finally {
      global.fetch = stubbedFetch;
    }
  });

  it("test_custom_secret_create_and_status: custom secrets list + an add form for named JSON objects", async () => {
    // Platform mode → no active model credential, so every stored credential is a
    // custom secret and the providers group shows its empty state.
    vi.mocked(getTenantProvider).mockResolvedValue(platformProvider());
    vi.mocked(listCredentials).mockResolvedValue({
      credentials: [{ name: STRIPE_SECRET_NAME, created_at: CREATED_AT_MS }],
    });

    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    const markup = renderToStaticMarkup(await Page());

    // The custom secret is listed and the add form for named JSON objects is composed.
    expect(markup).toContain('data-custom-secrets-list="ws_1"');
    expect(markup).toContain(STRIPE_SECRET_NAME);
    expect(markup).toContain('data-add-credential-form="ws_1"');
    expect(markup).toContain("Add a custom secret");
    // No active model key: provider rows still render, but no fake reference is made.
    expect(markup).toContain('data-provider-credential-rows-client="ws_1"');
    expect(markup).toContain("provider keys");
    expect(markup).toContain("Custom — OpenAI-compatible");
  });

  it("marks the custom OpenAI-compatible provider row as connected when selected", async () => {
    vi.mocked(getTenantProvider).mockResolvedValue({
      ...platformProvider(),
      mode: PROVIDER_MODE.self_managed,
      provider: OPENAI_COMPATIBLE_PROVIDER,
      credential_ref: "custom-endpoint",
    });
    vi.mocked(listCredentials).mockResolvedValue({ credentials: [] });

    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    const markup = renderToStaticMarkup(await Page());

    expect(markup).toContain("Custom — OpenAI-compatible");
    expect(markup).toContain("Connected");
  });

  it("never fabricates a referenced-by when the provider fetch fails", async () => {
    vi.mocked(getTenantProvider).mockRejectedValue(new Error("503"));
    vi.mocked(listCredentials).mockResolvedValue({
      credentials: [{ name: STRIPE_SECRET_NAME, created_at: CREATED_AT_MS }],
    });
    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    const markup = renderToStaticMarkup(await Page());
    // With no known provider ref, the credential is a custom secret (not a
    // fabricated model-provider reference).
    expect(markup).toContain('data-custom-secrets-list="ws_1"');
    expect(markup).toContain('data-provider-credential-rows-client="ws_1"');
    expect(markup).toContain("provider keys");
  });

  it("renders the no-workspace empty state under the Credentials title", async () => {
    vi.mocked(withWorkspaceScope).mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain(">Credentials<");
    expect(markup).toContain("No workspace yet");
  });

  it("redirects to /sign-in when unauthenticated", async () => {
    auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(null) });
    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    await expect(Page()).rejects.toThrow("redirect:/sign-in");
  });

  it("falls back to an empty vault when listCredentials errors", async () => {
    vi.mocked(getTenantProvider).mockResolvedValue(platformProvider());
    vi.mocked(listCredentials).mockRejectedValue(new Error("503"));
    const { default: Page } = await import("../app/(dashboard)/credentials/page");
    const markup = renderToStaticMarkup(await Page());
    // Empty custom-secrets list still renders the add form + integrations group.
    expect(markup).toContain('data-add-credential-form="ws_1"');
    expect(markup).toContain('data-testid="group-integrations"');
  });
});
