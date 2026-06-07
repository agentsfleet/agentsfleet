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

beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({ getToken: vi.fn().mockResolvedValue("token_123") });
});
afterEach(() => vi.clearAllMocks());

describe("/credentials route", () => {
  it("redirects into the unified Models & Credentials page", async () => {
    const { default: CredentialsPage } = await import("../app/(dashboard)/credentials/page");
    expect(() => CredentialsPage()).toThrow("redirect:/settings/models#credentials");
  });
});

describe("unified Models & Credentials page", () => {
  it("renders both a Model section and a Credentials section with the stored credentials", async () => {
    vi.mocked(resolveActiveWorkspace).mockResolvedValue({ id: "ws_1", name: "Acme" } as never);
    vi.mocked(getTenantProvider).mockResolvedValue({
      mode: "platform",
      provider: "fireworks",
      model: "kimi-k2.6",
      context_cap_tokens: 256000,
      credential_ref: null,
    });
    vi.mocked(listCredentials).mockResolvedValue({
      credentials: [{ name: "fly", created_at: 1_777_507_200_000 }],
    });
    vi.mocked(getModelCaps).mockResolvedValue({
      version: "1",
      models: [
        { id: "kimi-k2.6", provider: "fireworks", context_cap_tokens: 256000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
      ],
      rates: { run_nanos_per_sec: 0, event_nanos: 0 },
      billing: { starter_credit_nanos: 0, free_trial_end_ms: 0, free_trial_stage_nanos: 0 },
    });

    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());

    // Both section headings render on one page.
    expect(markup).toContain(">Model<");
    expect(markup).toContain(">Credentials<");
    // The credentials section is anchorable from the in-page "manage" link.
    expect(markup).toContain('id="credentials"');
    // The stored credential is listed in the Credentials section.
    expect(markup).toContain("fly");
    // Page title reflects the union.
    expect(markup).toContain("Models &amp; Credentials");
  });

  it("renders the no-workspace empty state under the unified title", async () => {
    vi.mocked(resolveActiveWorkspace).mockResolvedValue(null);
    const { default: Page } = await import("../app/(dashboard)/settings/models/page");
    const markup = renderToStaticMarkup(await Page());
    expect(markup).toContain("Models &amp; Credentials");
    expect(markup).toContain("No workspace yet");
  });
});
