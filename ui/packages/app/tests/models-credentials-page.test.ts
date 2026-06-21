import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

// Server-component page tests: render the async page to static markup with the
// data layer mocked at module boundaries. Mirrors tests/app-pages.test.ts.

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const auth = vi.fn();

vi.mock("next/navigation", () => ({
  redirect,
  useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));
vi.mock("@clerk/nextjs/server", () => ({ auth }));

vi.mock("@/lib/workspace", () => ({
  resolveActiveWorkspace: vi.fn(),
}));
vi.mock("@/lib/api/tenant_provider", () => ({
  getTenantProvider: vi.fn(),
}));
vi.mock("@/lib/api/credentials", () => ({
  listCredentials: vi.fn(),
}));
vi.mock("@/lib/api/model_caps", () => ({
  getModelCaps: vi.fn(),
  uniqueModelIds: (models: Array<{ id: string }>) =>
    Array.from(new Map(models.map((m) => [m.id, m])).values()),
}));

vi.mock("lucide-react", () => {
  const make = (name: string) => (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": name });
  return {
    ZapIcon: make("ZapIcon"),
    KeyRoundIcon: make("KeyRoundIcon"),
    PencilIcon: make("PencilIcon"),
    Trash2Icon: make("Trash2Icon"),
    Loader2Icon: make("Loader2Icon"),
  };
});

import { resolveActiveWorkspace } from "@/lib/workspace";
import { getTenantProvider } from "@/lib/api/tenant_provider";
import { listCredentials } from "@/lib/api/credentials";
import { getModelCaps } from "@/lib/api/model_caps";
import { PROVIDER_MODE } from "@/lib/types";

const WORKSPACE_FIXTURE = { id: "ws_1", name: "Acme" };
const FIREWORKS_PROVIDER = "fireworks";
const FIREWORKS_MODEL_ID = "kimi-k2.6";
const ANTHROPIC_PROVIDER = "anthropic";
const CLAUDE_MODEL_ID = "claude-sonnet-4-5";
const FLY_CREDENTIAL_NAME = "fly";
const ANTHROPIC_CREDENTIAL_NAME = "anthropic-prod";
const CREATED_AT_MS = 1_777_507_200_000;
const CONTEXT_CAP_TOKENS = 256000;
const ZERO_RATE_NANOS = 0;
const ZERO_TIMESTAMP_MS = 0;

function modelCap(provider: string, id: string) {
  return {
    id,
    provider,
    context_cap_tokens: CONTEXT_CAP_TOKENS,
    input_nanos_per_mtok: ZERO_RATE_NANOS,
    cached_input_nanos_per_mtok: ZERO_RATE_NANOS,
    output_nanos_per_mtok: ZERO_RATE_NANOS,
  };
}

function modelCatalogue(provider: string, id: string) {
  return {
    version: "1",
    models: [modelCap(provider, id)],
    rates: { run_nanos_per_sec: ZERO_RATE_NANOS, event_nanos: ZERO_RATE_NANOS },
    billing: {
      starter_credit_nanos: ZERO_RATE_NANOS,
      free_trial_end_ms: ZERO_TIMESTAMP_MS,
      free_trial_stage_nanos: ZERO_RATE_NANOS,
    },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_123") });
});
afterEach(() => vi.clearAllMocks());

describe("/credentials route", () => {
  it("redirects into the unified Models & Credentials page", async () => {
    const { default: CredentialsPage } = await import("../app/(dashboard)/credentials/page");
    expect(() => CredentialsPage()).toThrow("redirect:/settings/models?tab=credentials#credentials");
  });
});

describe("unified Models & Credentials page", () => {
  it("renders model setup by default and stored credentials on the credentials tab", async () => {
    vi.mocked(resolveActiveWorkspace).mockResolvedValue(WORKSPACE_FIXTURE as never);
    vi.mocked(getTenantProvider).mockResolvedValue({
      mode: PROVIDER_MODE.platform,
      provider: FIREWORKS_PROVIDER,
      model: FIREWORKS_MODEL_ID,
      context_cap_tokens: CONTEXT_CAP_TOKENS,
      credential_ref: null,
    });
    vi.mocked(listCredentials).mockResolvedValue({
      credentials: [{ name: FLY_CREDENTIAL_NAME, created_at: CREATED_AT_MS }],
    });
    vi.mocked(getModelCaps).mockResolvedValue(modelCatalogue(FIREWORKS_PROVIDER, FIREWORKS_MODEL_ID));

    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());

    // The default tab keeps setup focused on the model path.
    expect(markup).toContain(">Model setup<");
    expect(markup).toContain(">Current setup<");
    expect(markup).toContain(">Credentials<");
    expect(markup).not.toContain('id="credentials"');

    const credentialsMarkup = renderToStaticMarkup(
      await Page({ searchParams: Promise.resolve({ tab: "credentials" }) }),
    );
    expect(credentialsMarkup).toContain('id="credentials"');
    // The stored credential is listed in the Credentials section.
    expect(credentialsMarkup).toContain(FLY_CREDENTIAL_NAME);
    expect(credentialsMarkup).toContain("Credential vault");
    // Page title reflects the union.
    expect(markup).toContain("Models &amp; Credentials");
  });

  it("renders the self-managed provider state with the chosen credential", async () => {
    vi.mocked(resolveActiveWorkspace).mockResolvedValue(WORKSPACE_FIXTURE as never);
    vi.mocked(getTenantProvider).mockResolvedValue({
      mode: PROVIDER_MODE.self_managed,
      provider: ANTHROPIC_PROVIDER,
      model: CLAUDE_MODEL_ID,
      context_cap_tokens: CONTEXT_CAP_TOKENS,
      credential_ref: ANTHROPIC_CREDENTIAL_NAME,
    });
    vi.mocked(listCredentials).mockResolvedValue({
      credentials: [{ name: ANTHROPIC_CREDENTIAL_NAME, created_at: CREATED_AT_MS }],
    });
    vi.mocked(getModelCaps).mockResolvedValue(modelCatalogue(ANTHROPIC_PROVIDER, CLAUDE_MODEL_ID));

    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());

    expect(markup).toContain("Own provider key");
    expect(markup).toContain(ANTHROPIC_PROVIDER);
    expect(markup).toContain(CLAUDE_MODEL_ID);
    expect(markup).toContain(ANTHROPIC_CREDENTIAL_NAME);
  });

  it("renders the no-workspace empty state under the unified title", async () => {
    vi.mocked(resolveActiveWorkspace).mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("Models &amp; Credentials");
    expect(markup).toContain("No workspace yet");
  });
});
