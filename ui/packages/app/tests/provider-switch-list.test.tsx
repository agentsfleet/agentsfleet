import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { CREDENTIAL_KIND, type Credential } from "@/lib/api/credentials";
import { PROVIDER_MODE, type TenantProvider } from "@/lib/types";
import type { ModelCap } from "@/lib/api/model_caps";

// The "switch anytime" list: a Platform row (live only), one row per named
// provider (Switch when keyed / Add key & model when not), stored custom
// endpoints (Switch), an Add-endpoint row, and a generic paste-detect row. The
// active credential is excluded everywhere (it's the hero).

const routerRefresh = vi.fn();
const resetProviderAction = vi.hoisted(() => vi.fn());
const setProviderSelfManagedAction = vi.hoisted(() => vi.fn());
const captureModelActivated = vi.hoisted(() => vi.fn());
const captureProviderReset = vi.hoisted(() => vi.fn());
const { catalogueState } = vi.hoisted(() => ({
  catalogueState: { models: [] as ModelCap[], loading: false, error: false },
}));

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }) }));
vi.mock("@/app/(dashboard)/settings/models/actions", () => ({ resetProviderAction, setProviderSelfManagedAction }));
vi.mock("@/app/(dashboard)/settings/models/lib/track", () => ({ captureModelActivated, captureProviderReset }));
vi.mock("@/app/(dashboard)/settings/models/components/ModelCatalogueProvider", () => ({
  useModelCatalogue: () => catalogueState,
}));
vi.mock("@agentsfleet/design-system", async () => (await import("./helpers/models-component-mocks")).designSystemStub());
vi.mock("lucide-react", async () => (await import("./helpers/models-component-mocks")).lucideStub());
vi.mock("@/app/(dashboard)/settings/models/components/ProviderKeyForm", () => ({
  default: ({
    provider,
    activate,
    onDone,
    onCancel,
  }: {
    provider?: string;
    activate?: boolean;
    onDone: () => void;
    onCancel?: () => void;
  }) =>
    React.createElement(
      "div",
      { "data-testid": "provider-key-form", "data-provider": provider ?? "generic", "data-activate": String(!!activate) },
      React.createElement("button", { "data-testid": "pkf-done", onClick: onDone }, "done"),
      React.createElement("button", { "data-testid": "pkf-cancel", onClick: onCancel }, "cancel"),
    ),
}));
vi.mock("@/app/(dashboard)/settings/models/components/CustomEndpointForm", () => ({
  default: ({ activate, onDone, onCancel }: { activate?: boolean; onDone: () => void; onCancel?: () => void }) =>
    React.createElement(
      "div",
      { "data-testid": "custom-endpoint-form", "data-activate": String(!!activate) },
      React.createElement("button", { "data-testid": "cef-done", onClick: onDone }, "done"),
      React.createElement("button", { "data-testid": "cef-cancel", onClick: onCancel }, "cancel"),
    ),
}));

import ProviderSwitchList from "@/app/(dashboard)/settings/models/components/ProviderSwitchList";

const cap = (provider: string): ModelCap => ({
  id: `${provider}-model`,
  provider,
  context_cap_tokens: 1,
  input_nanos_per_mtok: 1,
  cached_input_nanos_per_mtok: 1,
  output_nanos_per_mtok: 1,
});

function liveProvider(credentialRef = "anthropic-prod"): TenantProvider {
  return {
    mode: PROVIDER_MODE.self_managed,
    provider: "anthropic",
    model: "claude-sonnet-4-6",
    context_cap_tokens: 256000,
    credential_ref: credentialRef,
  } as TenantProvider;
}

const ROSTER: Credential[] = [
  { kind: CREDENTIAL_KIND.provider_key, name: "anthropic-prod", created_at: 1, provider: "anthropic", model: "claude-sonnet-4-6" },
  { kind: CREDENTIAL_KIND.provider_key, name: "openai-key", created_at: 1, provider: "openai", model: "gpt-4" },
  { kind: CREDENTIAL_KIND.provider_key, name: "groq-key", created_at: 1, provider: "groq" },
  { kind: CREDENTIAL_KIND.custom_endpoint, name: "vllm", created_at: 1, provider: "openai-compatible", model: "m1", base_url: "https://x/v1" },
  { kind: CREDENTIAL_KIND.custom_endpoint, name: "vllm2", created_at: 1, provider: "openai-compatible", base_url: "https://y/v1" },
  { kind: CREDENTIAL_KIND.custom_endpoint, name: "vllm3", created_at: 1, provider: "openai-compatible" },
];

function rowOf(text: string | RegExp) {
  const el = screen.getByText(text);
  const row = el.closest("[data-row]");
  if (!row) throw new Error(`no row container for ${String(text)}`);
  return within(row as HTMLElement);
}

beforeEach(() => {
  vi.clearAllMocks();
  catalogueState.models = [cap("anthropic"), cap("openai"), cap("openai-compatible"), cap("fireworks")];
  resetProviderAction.mockResolvedValue({ ok: true, data: {} });
  setProviderSelfManagedAction.mockResolvedValue({
    ok: true,
    data: { provider: "openai", mode: "self_managed", model: "gpt-4" },
  });
});
afterEach(() => cleanup());

function renderLive(credentialRef = "anthropic-prod") {
  render(
    React.createElement(ProviderSwitchList, {
      workspaceId: "ws_1",
      provider: liveProvider(credentialRef),
      credentials: ROSTER,
    }),
  );
}

describe("ProviderSwitchList — live roster", () => {
  it("renders the platform row, keyed/unkeyed provider rows, and custom endpoints", () => {
    renderLive();
    expect(screen.getByText("Platform defaults")).toBeTruthy();
    // anthropic is the active credential → excluded from the list.
    expect(screen.queryByText("Anthropic")).toBeNull();
    // openai keyed → "Key saved · gpt-4"; groq keyed but model-less; fireworks unkeyed.
    expect(screen.getByText("Key saved · gpt-4")).toBeTruthy();
    expect(screen.getByText("Key saved · model not set")).toBeTruthy();
    expect(screen.getByText("Not configured")).toBeTruthy();
    // Custom endpoints: vllm has a model, vllm2 falls back to its base_url, and
    // vllm3 (neither) shows the model-not-set placeholder.
    expect(screen.getByText("vllm · m1")).toBeTruthy();
    expect(screen.getByText("vllm2 · https://y/v1")).toBeTruthy();
    expect(screen.getByText("vllm3 · model not set")).toBeTruthy();
    // The two add affordances.
    expect(screen.getByText("OpenAI-compatible gateway, OpenRouter, or self-hosted")).toBeTruthy();
    expect(screen.getByText("Other provider")).toBeTruthy();
  });

  it("switches to a keyed provider (with and without a stored model) and to a custom endpoint", async () => {
    renderLive();
    rowOf("Key saved · gpt-4").getByRole("button", { name: "Switch" }).click();
    await waitFor(() =>
      expect(setProviderSelfManagedAction).toHaveBeenCalledWith({ credential_ref: "openai-key", model: "gpt-4" }),
    );
    expect(captureModelActivated).toHaveBeenCalled();

    rowOf("Key saved · model not set").getByRole("button", { name: "Switch" }).click();
    await waitFor(() =>
      expect(setProviderSelfManagedAction).toHaveBeenCalledWith({ credential_ref: "groq-key", model: undefined }),
    );

    rowOf("vllm · m1").getByRole("button", { name: "Switch" }).click();
    await waitFor(() =>
      expect(setProviderSelfManagedAction).toHaveBeenCalledWith({ credential_ref: "vllm", model: "m1" }),
    );
  });

  it("switches to platform defaults from the platform row", async () => {
    renderLive();
    rowOf("Built-in provider · no key").getByRole("button", { name: "Switch" }).click();
    await waitFor(() => expect(resetProviderAction).toHaveBeenCalled());
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
    // provider_reset records the active provider being left behind.
    expect(captureProviderReset).toHaveBeenCalledWith("anthropic");
  });

  it("expands the locked add-key form, the endpoint form, and the generic add form", () => {
    renderLive();
    // Unkeyed provider → locked ProviderKeyForm with the provider id.
    fireEvent.click(rowOf("Not configured").getByRole("button", { name: "Add key & model" }));
    const lockedForm = screen.getByTestId("provider-key-form");
    expect(lockedForm.getAttribute("data-provider")).toBe("fireworks");
    expect(lockedForm.getAttribute("data-activate")).toBe("true");
    // Toggling the same row closes it.
    fireEvent.click(rowOf("Not configured").getByRole("button", { name: "Add key & model" }));
    expect(screen.queryByTestId("provider-key-form")).toBeNull();

    // Add endpoint → CustomEndpointForm (activate).
    fireEvent.click(screen.getByRole("button", { name: "Add endpoint" }));
    expect(screen.getByTestId("custom-endpoint-form").getAttribute("data-activate")).toBe("true");

    // Other provider → generic ProviderKeyForm (no locked provider).
    fireEvent.click(rowOf("Paste a key — we'll detect common providers, or pick one").getByRole("button", { name: "Add key & model" }));
    expect(screen.getByTestId("provider-key-form").getAttribute("data-provider")).toBe("generic");
  });

  it("closes each expanded form via its onDone and onCancel callbacks", () => {
    renderLive();
    // Locked provider-key form: onDone closes it, onCancel closes a reopen.
    fireEvent.click(rowOf("Not configured").getByRole("button", { name: "Add key & model" }));
    fireEvent.click(screen.getByTestId("pkf-done"));
    expect(screen.queryByTestId("provider-key-form")).toBeNull();
    fireEvent.click(rowOf("Not configured").getByRole("button", { name: "Add key & model" }));
    fireEvent.click(screen.getByTestId("pkf-cancel"));
    expect(screen.queryByTestId("provider-key-form")).toBeNull();

    // Custom-endpoint form callbacks.
    fireEvent.click(screen.getByRole("button", { name: "Add endpoint" }));
    fireEvent.click(screen.getByTestId("cef-done"));
    expect(screen.queryByTestId("custom-endpoint-form")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Add endpoint" }));
    fireEvent.click(screen.getByTestId("cef-cancel"));
    expect(screen.queryByTestId("custom-endpoint-form")).toBeNull();

    // Generic provider-key form callbacks.
    fireEvent.click(rowOf("Paste a key — we'll detect common providers, or pick one").getByRole("button", { name: "Add key & model" }));
    fireEvent.click(screen.getByTestId("pkf-done"));
    expect(screen.queryByTestId("provider-key-form")).toBeNull();
    fireEvent.click(rowOf("Paste a key — we'll detect common providers, or pick one").getByRole("button", { name: "Add key & model" }));
    fireEvent.click(screen.getByTestId("pkf-cancel"));
    expect(screen.queryByTestId("provider-key-form")).toBeNull();
  });

  it("excludes a custom endpoint when it is the active credential", () => {
    renderLive("vllm");
    // vllm is active → not offered as a switch row; vllm2 still is.
    expect(screen.queryByText("vllm · m1")).toBeNull();
    expect(screen.getByText("vllm2 · https://y/v1")).toBeTruthy();
  });

  it("surfaces a switch error", async () => {
    setProviderSelfManagedAction.mockResolvedValue({ ok: false, error: "switch failed" });
    renderLive();
    rowOf("Key saved · gpt-4").getByRole("button", { name: "Switch" }).click();
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/switch failed/));
  });

  it("surfaces a platform-switch error", async () => {
    resetProviderAction.mockResolvedValue({ ok: false, error: "platform reset failed" });
    renderLive();
    rowOf("Built-in provider · no key").getByRole("button", { name: "Switch" }).click();
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/platform reset failed/));
  });

  it("shows a switching spinner while an action is in flight", async () => {
    let resolveSwitch!: (v: unknown) => void;
    setProviderSelfManagedAction.mockReturnValue(new Promise((r) => (resolveSwitch = r)));
    renderLive();
    rowOf("Key saved · gpt-4").getByRole("button", { name: "Switch" }).click();
    await waitFor(() => expect(screen.getByText("Switching")).toBeTruthy());
    resolveSwitch({ ok: true, data: { provider: "openai", mode: "self_managed", model: "gpt-4" } });
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });
});

describe("ProviderSwitchList — not live", () => {
  it("omits the platform row and still offers keyed-provider switches", () => {
    render(
      React.createElement(ProviderSwitchList, {
        workspaceId: "ws_1",
        provider: { ...liveProvider(), mode: PROVIDER_MODE.platform } as TenantProvider,
        credentials: ROSTER,
      }),
    );
    expect(screen.queryByText("Platform defaults")).toBeNull();
    // With no active ref, the anthropic key is now a switchable row.
    expect(screen.getByText("Key saved · claude-sonnet-4-6")).toBeTruthy();
  });

  it("treats a null provider as not live", () => {
    render(
      React.createElement(ProviderSwitchList, { workspaceId: "ws_1", provider: null, credentials: [] }),
    );
    expect(screen.queryByText("Platform defaults")).toBeNull();
    expect(screen.getByText("Other provider")).toBeTruthy();
  });
});
