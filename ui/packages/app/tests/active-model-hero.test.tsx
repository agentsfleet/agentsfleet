import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CREDENTIAL_KIND, type Credential } from "@/lib/api/credentials";
import { PROVIDER_MODE, type TenantProvider } from "@/lib/types";

// The active-model hero: LIVE vs DEFAULT presentation, the live action set
// (Change model / Replace key / Switch to platform), Replace-key gating by the
// active credential's kind, and the context formatter.

const routerRefresh = vi.fn();
const resetProviderAction = vi.hoisted(() => vi.fn());
const captureProviderReset = vi.hoisted(() => vi.fn());

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }) }));
vi.mock("@/app/(dashboard)/settings/models/actions", () => ({ resetProviderAction }));
vi.mock("@/app/(dashboard)/settings/models/lib/track", () => ({ captureProviderReset }));
vi.mock("@agentsfleet/design-system", async () => (await import("./helpers/models-component-mocks")).designSystemStub());
vi.mock("@/app/(dashboard)/settings/models/components/HeroChangeModelPanel", () => ({
  default: ({ onClose }: { onClose: () => void }) =>
    React.createElement(
      "div",
      { "data-testid": "hero-change-model" },
      React.createElement("button", { "data-testid": "change-close", onClick: onClose }, "close"),
    ),
}));
vi.mock("@/app/(dashboard)/settings/models/components/HeroReplaceKeyPanel", () => ({
  default: ({ onClose }: { onClose: () => void }) =>
    React.createElement(
      "div",
      { "data-testid": "hero-replace-key" },
      React.createElement("button", { "data-testid": "replace-close", onClick: onClose }, "close"),
    ),
}));

import ActiveModelHero from "@/app/(dashboard)/settings/models/components/ActiveModelHero";

function selfManaged(over: Partial<TenantProvider> = {}): TenantProvider {
  return {
    mode: PROVIDER_MODE.self_managed,
    provider: "anthropic",
    model: "claude-sonnet-4-6",
    context_cap_tokens: 256000,
    credential_ref: "anthropic-prod",
    ...over,
  } as TenantProvider;
}

const providerKeyCred: Credential = {
  kind: CREDENTIAL_KIND.provider_key,
  name: "anthropic-prod",
  created_at: 1,
  provider: "anthropic",
  model: "claude-sonnet-4-6",
};

beforeEach(() => {
  vi.clearAllMocks();
  resetProviderAction.mockResolvedValue({ ok: true, data: {} });
});
afterEach(() => cleanup());

describe("ActiveModelHero — live (self-managed)", () => {
  it("renders LIVE with the model + provider-direct meta and toggles both panels", async () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: selfManaged(),
        credentials: [providerKeyCred],
      }),
    );
    const hero = screen.getByTestId("active-model-hero");
    expect(hero.getAttribute("data-live")).toBe("true");
    expect(screen.getByText("LIVE")).toBeTruthy();
    expect(screen.getByText("claude-sonnet-4-6")).toBeTruthy();
    expect(screen.getByText("Anthropic")).toBeTruthy();
    expect(screen.getByText("256k")).toBeTruthy();
    expect(screen.getByText("Provider direct")).toBeTruthy();
    expect(screen.getByText(/via anthropic-prod/)).toBeTruthy();

    // Change model panel toggles open then closed (via the toggle button).
    fireEvent.click(screen.getByRole("button", { name: "Change model" }));
    expect(screen.getByTestId("hero-change-model")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Change model" }));
    expect(screen.queryByTestId("hero-change-model")).toBeNull();

    // Re-open, then close via the panel's own onClose callback.
    fireEvent.click(screen.getByRole("button", { name: "Change model" }));
    fireEvent.click(screen.getByTestId("change-close"));
    expect(screen.queryByTestId("hero-change-model")).toBeNull();

    // Replace key is available for a provider_key credential; toggle it open and
    // closed via the button (covers both toggle branches), then reopen and close
    // via the panel's onClose callback.
    fireEvent.click(screen.getByRole("button", { name: "Replace key" }));
    expect(screen.getByTestId("hero-replace-key")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Replace key" }));
    expect(screen.queryByTestId("hero-replace-key")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Replace key" }));
    fireEvent.click(screen.getByTestId("replace-close"));
    expect(screen.queryByTestId("hero-replace-key")).toBeNull();
  });

  it("hides Replace key for a custom_secret active credential (can't rotate as a model key)", () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: selfManaged({ credential_ref: "STRIPE" }),
        credentials: [{ kind: CREDENTIAL_KIND.custom_secret, name: "STRIPE", created_at: 1 }],
      }),
    );
    expect(screen.queryByRole("button", { name: "Replace key" })).toBeNull();
    expect(screen.getByRole("button", { name: "Change model" })).toBeTruthy();
  });

  it("shows Replace key for a custom_endpoint credential and formats sub-1k context raw", () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: selfManaged({ provider: "openai-compatible", context_cap_tokens: 500, credential_ref: "vllm" }),
        credentials: [
          { kind: CREDENTIAL_KIND.custom_endpoint, name: "vllm", created_at: 1, provider: "openai-compatible", model: "m1", base_url: "https://x/v1" },
        ],
      }),
    );
    expect(screen.getByRole("button", { name: "Replace key" })).toBeTruthy();
    expect(screen.getByText("500")).toBeTruthy();
  });

  it("falls back to the provider id when there is no credential ref, and shows default context for zero", () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: selfManaged({ credential_ref: null, context_cap_tokens: 0 }),
        credentials: [],
      }),
    );
    expect(screen.getByText(/via anthropic/)).toBeTruthy();
    expect(screen.getByText("default")).toBeTruthy();
    // No credential ref → no Replace key, and the change panel can't open.
    expect(screen.queryByRole("button", { name: "Replace key" })).toBeNull();
  });

  it("switches to platform defaults, refreshing on success", async () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: selfManaged(),
        credentials: [providerKeyCred],
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Switch to platform defaults" }));
    await waitFor(() => expect(resetProviderAction).toHaveBeenCalled());
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
    // provider_reset records the provider being left behind.
    expect(captureProviderReset).toHaveBeenCalledWith("anthropic");
  });

  it("surfaces a reset error", async () => {
    resetProviderAction.mockResolvedValue({ ok: false, error: "reset failed" });
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: selfManaged(),
        credentials: [providerKeyCred],
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Switch to platform defaults" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/reset failed/));
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(captureProviderReset).not.toHaveBeenCalled();
  });
});

describe("ActiveModelHero — default (platform / no provider)", () => {
  it("renders DEFAULT with the platform copy and a bring-your-own-key anchor", () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: { ...selfManaged(), mode: PROVIDER_MODE.platform } as TenantProvider,
        credentials: [],
      }),
    );
    const hero = screen.getByTestId("active-model-hero");
    expect(hero.getAttribute("data-live")).toBe("false");
    expect(screen.getByText("DEFAULT")).toBeTruthy();
    expect(screen.getByText("Platform default model")).toBeTruthy();
    expect(screen.getByText("agentsfleet managed")).toBeTruthy();
    expect(screen.getByText("Tenant balance")).toBeTruthy();
    const link = screen.getByText("Bring your own key").closest("a");
    expect(link?.getAttribute("href")).toBe("#other-providers");
    expect(link?.getAttribute("data-size")).toBe("sm");
  });

  it("treats a null provider as the default view", () => {
    render(
      React.createElement(ActiveModelHero, { workspaceId: "ws_1", provider: null, credentials: [] }),
    );
    expect(screen.getByText("DEFAULT")).toBeTruthy();
  });
});
