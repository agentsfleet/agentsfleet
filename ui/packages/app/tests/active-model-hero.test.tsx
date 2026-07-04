import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SECRET_KIND, type Secret } from "@/lib/api/secrets";
import { PROVIDER_MODE, type TenantProvider } from "@/lib/types";

// The active-model hero: LIVE vs DEFAULT presentation, the live action set
// (Change model / Replace key / Switch to platform), Replace-key gating by the
// active secret's kind, and the context formatter.

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
vi.mock("@/app/(dashboard)/settings/models/components/ProviderKeyForm", () => ({
  default: ({ activate, onDone, onCancel }: { activate?: boolean; onDone: () => void; onCancel?: () => void }) =>
    React.createElement(
      "div",
      { "data-testid": "provider-key-form", "data-activate": String(!!activate) },
      React.createElement("button", { "data-testid": "pkf-done", onClick: onDone }, "done"),
      React.createElement("button", { "data-testid": "pkf-cancel", onClick: onCancel }, "cancel"),
    ),
}));

import ActiveModelHero from "@/app/(dashboard)/settings/models/components/ActiveModelHero";

function selfManaged(over: Partial<TenantProvider> = {}): TenantProvider {
  return {
    mode: PROVIDER_MODE.self_managed,
    provider: "anthropic",
    model: "claude-sonnet-4-6",
    context_cap_tokens: 256000,
    secret_ref: "anthropic-prod",
    ...over,
  } as TenantProvider;
}

const providerKeyCred: Secret = {
  kind: SECRET_KIND.provider_key,
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
        secrets: [providerKeyCred],
      }),
    );
    const hero = screen.getByTestId("active-model-hero");
    expect(hero.getAttribute("data-live")).toBe("true");
    expect(screen.getByText("LIVE")).toBeTruthy();
    expect(screen.getByText("claude-sonnet-4-6")).toBeTruthy();
    expect(screen.getByText("Anthropic")).toBeTruthy();
    expect(screen.getByText("256k")).toBeTruthy();
    expect(screen.getByText("Provider direct")).toBeTruthy();
    expect(screen.getByText("anthropic-prod")).toBeTruthy(); // secret ref (mono span after "via")

    // Change model panel toggles open then closed (via the toggle button).
    fireEvent.click(screen.getByRole("button", { name: "Change model" }));
    expect(screen.getByTestId("hero-change-model")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Change model" }));
    expect(screen.queryByTestId("hero-change-model")).toBeNull();

    // Re-open, then close via the panel's own onClose callback.
    fireEvent.click(screen.getByRole("button", { name: "Change model" }));
    fireEvent.click(screen.getByTestId("change-close"));
    expect(screen.queryByTestId("hero-change-model")).toBeNull();

    // Replace key is available for a provider_key secret; toggle it open and
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

  it("hides Replace key for a custom_secret active secret (can't rotate as a model key)", () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: selfManaged({ secret_ref: "STRIPE" }),
        secrets: [{ kind: SECRET_KIND.custom_secret, name: "STRIPE", created_at: 1 }],
      }),
    );
    expect(screen.queryByRole("button", { name: "Replace key" })).toBeNull();
    expect(screen.getByRole("button", { name: "Change model" })).toBeTruthy();
  });

  it("shows Replace key for a custom_endpoint secret and formats sub-1k context raw", () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: selfManaged({ provider: "openai-compatible", context_cap_tokens: 500, secret_ref: "vllm" }),
        secrets: [
          { kind: SECRET_KIND.custom_endpoint, name: "vllm", created_at: 1, provider: "openai-compatible", model: "m1", base_url: "https://x/v1" },
        ],
      }),
    );
    expect(screen.getByRole("button", { name: "Replace key" })).toBeTruthy();
    expect(screen.getByText("500")).toBeTruthy();
  });

  it("falls back to the provider id when there is no secret ref, and shows default context for zero", () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: selfManaged({ secret_ref: null, context_cap_tokens: 0 }),
        secrets: [],
      }),
    );
    expect(screen.getByText("anthropic")).toBeTruthy(); // secretRef null → provider id after "via"
    expect(screen.getByText("default")).toBeTruthy();
    // No secret ref → no Replace key, and the change panel can't open.
    expect(screen.queryByRole("button", { name: "Replace key" })).toBeNull();
  });

  it("switches to platform defaults, refreshing on success", async () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: selfManaged(),
        secrets: [providerKeyCred],
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Switch to platform defaults" }));
    await waitFor(() => expect(resetProviderAction).toHaveBeenCalled());
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
    // provider_reset records the provider being left behind.
    expect(captureProviderReset).toHaveBeenCalledWith("anthropic");
  });

  it("surfaces a friendly reset error routed through presentErrorString, not the raw string", async () => {
    resetProviderAction.mockResolvedValue({ ok: false, error: "reset failed" });
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: selfManaged(),
        secrets: [providerKeyCred],
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Switch to platform defaults" }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/^Couldn't switch to platform defaults/),
    );
    expect(screen.getByRole("alert").textContent).toMatch(/reset failed/);
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(captureProviderReset).not.toHaveBeenCalled();
  });
});

describe("ActiveModelHero — default (platform / no provider)", () => {
  it("renders DEFAULT as a row with no bring-your-own-key anchor", () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: { ...selfManaged(), mode: PROVIDER_MODE.platform } as TenantProvider,
        secrets: [],
      }),
    );
    const hero = screen.getByTestId("active-model-hero");
    expect(hero.getAttribute("data-live")).toBe("false");
    expect(screen.getByText("DEFAULT")).toBeTruthy();
    expect(screen.getByText("Platform default model")).toBeTruthy();
    expect(screen.getByText("agentsfleet managed")).toBeTruthy();
    expect(screen.getByText("Tenant balance")).toBeTruthy();
    // Dimension 1.3: the pill already states the status, so the old redundant
    // "Managed by agentsfleet · no key needed" subtext is gone.
    expect(screen.queryByText("Managed by agentsfleet · no key needed")).toBeNull();
    // Dimension 1.2: no scroll-anchor button remains at all.
    expect(screen.queryByRole("button", { name: /bring your own key/i })).toBeNull();
    expect(screen.queryByRole("link", { name: /bring your own key/i })).toBeNull();
  });

  it("opens the generic add-key form inline on one click, and closes it again on toggle", () => {
    render(
      React.createElement(ActiveModelHero, {
        workspaceId: "ws_1",
        provider: { ...selfManaged(), mode: PROVIDER_MODE.platform } as TenantProvider,
        secrets: [],
      }),
    );
    const addButton = screen.getByRole("button", { name: "Add key & model" });
    expect(screen.queryByTestId("provider-key-form")).toBeNull();

    fireEvent.click(addButton);
    const form = screen.getByTestId("provider-key-form");
    expect(form.getAttribute("data-activate")).toBe("true");

    fireEvent.click(addButton);
    expect(screen.queryByTestId("provider-key-form")).toBeNull();

    // Re-open, then close via the form's own onDone/onCancel callbacks.
    fireEvent.click(addButton);
    fireEvent.click(screen.getByTestId("pkf-done"));
    expect(screen.queryByTestId("provider-key-form")).toBeNull();
    fireEvent.click(addButton);
    fireEvent.click(screen.getByTestId("pkf-cancel"));
    expect(screen.queryByTestId("provider-key-form")).toBeNull();
  });

  it("treats a null provider as the default view", () => {
    render(
      React.createElement(ActiveModelHero, { workspaceId: "ws_1", provider: null, secrets: [] }),
    );
    expect(screen.getByText("DEFAULT")).toBeTruthy();
  });
});
