import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { SECRET_KIND, type Secret } from "@/lib/api/secrets";
import { PROVIDER_MODE, type TenantProvider } from "@/lib/types";

// The Models "Providers" list: exactly 4 fixed rows (Default, Anthropic,
// Other provider, Custom — OpenAI-compatible). The LIVE badge renders on
// whichever row is active — there is no separate hero card. Default never
// exposes an edit action (locked; edited only via the admin Model Library).
// Anthropic/Other-provider rows support add/switch/change-model/replace-key/
// delete; delete is blocked on the currently active secret. Other-provider
// names whichever provider currently occupies its single slot and offers a
// picker across any additional stored non-Anthropic keys.

const routerRefresh = vi.fn();
const resetProviderAction = vi.hoisted(() => vi.fn());
const setProviderSelfManagedAction = vi.hoisted(() => vi.fn());
const deleteSecretAction = vi.hoisted(() => vi.fn());
const captureModelActivated = vi.hoisted(() => vi.fn());
const captureProviderReset = vi.hoisted(() => vi.fn());

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }) }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({ resetProviderAction, setProviderSelfManagedAction }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({ deleteSecretAction }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/lib/track", () => ({
  captureModelActivated,
  captureProviderReset,
  captureModelChanged: vi.fn(),
  captureKeyRotated: vi.fn(),
}));
vi.mock("@agentsfleet/design-system", async () => (await import("./helpers/models-component-mocks")).designSystemStub());
vi.mock("lucide-react", async () => (await import("./helpers/models-component-mocks")).lucideStub());
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderEditPanel", () => ({
  default: ({ onClose }: { onClose: () => void }) =>
    React.createElement(
      "div",
      { "data-testid": "provider-edit-panel" },
      React.createElement("button", { "data-testid": "edit-close", onClick: onClose }, "close"),
    ),
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderKeyForm", () => ({
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
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/CustomEndpointForm", () => ({
  default: ({ activate, onDone, onCancel }: { activate?: boolean; onDone: () => void; onCancel?: () => void }) =>
    React.createElement(
      "div",
      { "data-testid": "custom-endpoint-form", "data-activate": String(!!activate) },
      React.createElement("button", { "data-testid": "cef-done", onClick: onDone }, "done"),
      React.createElement("button", { "data-testid": "cef-cancel", onClick: onCancel }, "cancel"),
    ),
}));

import ProviderSwitchList from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderSwitchList";

function providerOf(over: Partial<TenantProvider> = {}): TenantProvider {
  return {
    mode: PROVIDER_MODE.self_managed,
    provider: "anthropic",
    model: "claude-sonnet-4-6",
    context_cap_tokens: 256000,
    secret_ref: "anthropic-prod",
    platform_default_available: true,
    ...over,
  } as TenantProvider;
}

const ANTHROPIC_SECRET: Secret = {
  kind: SECRET_KIND.provider_key,
  name: "anthropic-prod",
  created_at: 1,
  provider: "anthropic",
  model: "claude-sonnet-4-6",
};
const OPENAI_SECRET: Secret = {
  kind: SECRET_KIND.provider_key,
  name: "openai-key",
  created_at: 1,
  provider: "openai",
  model: "gpt-4",
};
const GROQ_SECRET: Secret = {
  kind: SECRET_KIND.provider_key,
  name: "groq-key",
  created_at: 1,
  provider: "groq",
};
const CUSTOM_ENDPOINT: Secret = {
  kind: SECRET_KIND.custom_endpoint,
  name: "vllm",
  created_at: 1,
  provider: "openai-compatible",
  model: "m1",
  base_url: "https://x/v1",
};

function rowOf(text: string | RegExp) {
  const el = screen.getByText(text);
  const row = el.closest("[data-row]");
  if (!row) throw new Error(`no row container for ${String(text)}`);
  return within(row as HTMLElement);
}

// Stable per-row lookup (data-testid) — text-based lookup is ambiguous once a
// row's own MetaGrid can echo the same provider name shown in its title.
function row(testId: "row-default" | "row-anthropic" | "row-other" | "row-custom") {
  return within(screen.getByTestId(testId));
}

beforeEach(() => {
  vi.clearAllMocks();
  resetProviderAction.mockResolvedValue({ ok: true, data: {} });
  deleteSecretAction.mockResolvedValue({ ok: true, data: undefined });
  setProviderSelfManagedAction.mockResolvedValue({
    ok: true,
    data: { provider: "openai", mode: "self_managed", model: "gpt-4" },
  });
});
afterEach(() => cleanup());

function renderList(provider: TenantProvider | null, secrets: Secret[]) {
  render(React.createElement(ProviderSwitchList, { workspaceId: "ws_1", provider, secrets }));
}

describe("ProviderSwitchList — exactly 4 fixed rows", () => {
  it("renders exactly 4 rows regardless of how many secrets are stored", () => {
    renderList(providerOf(), [ANTHROPIC_SECRET, OPENAI_SECRET, GROQ_SECRET, CUSTOM_ENDPOINT]);
    const list = screen.getByTestId("provider-switch-list");
    expect(list.querySelectorAll(":scope > [data-row], :scope > div > [data-row]").length).toBe(4);
    expect(screen.getByTestId("row-default")).toBeTruthy();
    expect(screen.getByTestId("row-anthropic")).toBeTruthy();
    expect(screen.getByTestId("row-other")).toBeTruthy();
    expect(screen.getByTestId("row-custom")).toBeTruthy();
  });

  it("renders 4 rows even with zero stored secrets and a null provider", () => {
    renderList(null, []);
    const list = screen.getByTestId("provider-switch-list");
    expect(list.querySelectorAll(":scope > [data-row], :scope > div > [data-row]").length).toBe(4);
  });
});

describe("ProviderSwitchList — LIVE lives on the active row, no separate hero", () => {
  it("shows LIVE on Anthropic's row when it's active, and on Default when platform mode", () => {
    renderList(providerOf(), [ANTHROPIC_SECRET]);
    expect(row("row-anthropic").getByText("Live")).toBeTruthy();
    expect(screen.queryByTestId("active-model-hero")).toBeNull();

    cleanup();
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, []);
    expect(rowOf("Default").getByText("Live")).toBeTruthy();
  });

  it("shows LIVE on the Other-provider row when a non-Anthropic secret is active", () => {
    renderList(providerOf({ provider: "openai", secret_ref: "openai-key", model: "gpt-4" }), [OPENAI_SECRET]);
    expect(row("row-other").getByText("Live")).toBeTruthy();
  });
});

describe("ProviderSwitchList — Default row is always read-only", () => {
  it("never renders an Edit action on the Default row, live or not", () => {
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, []);
    expect(rowOf("Default").queryByRole("button", { name: "Edit" })).toBeNull();

    cleanup();
    renderList(providerOf(), [ANTHROPIC_SECRET]); // Default not active this time
    expect(rowOf("Default").queryByRole("button", { name: "Edit" })).toBeNull();
  });

  it("disables Switch on Default with an explanatory note when no platform default is configured", async () => {
    renderList(providerOf({ platform_default_available: false }), [ANTHROPIC_SECRET]);
    const btn = rowOf("Default").getByRole("button", { name: "Switch" });
    expect(btn.getAttribute("aria-disabled")).toBe("true");
    expect(screen.getByText(/No default is configured/)).toBeTruthy();
    // Clicking a disabled button must not fire the action (guards in the handler).
    fireEvent.click(btn);
    await new Promise((r) => setTimeout(r, 0));
    expect(resetProviderAction).not.toHaveBeenCalled();
  });

  it("switches to platform defaults when available", async () => {
    renderList(providerOf({ platform_default_available: true }), [ANTHROPIC_SECRET]);
    fireEvent.click(rowOf("Default").getByRole("button", { name: "Switch" }));
    await waitFor(() => expect(resetProviderAction).toHaveBeenCalled());
    expect(captureProviderReset).toHaveBeenCalledWith("anthropic");
  });
});

describe("ProviderSwitchList — Anthropic row", () => {
  it("offers Add key when nothing is stored", () => {
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, []);
    fireEvent.click(row("row-anthropic").getByRole("button", { name: "Add key" }));
    const form = screen.getByTestId("provider-key-form");
    expect(form.getAttribute("data-provider")).toBe("anthropic");
    expect(form.getAttribute("data-activate")).toBe("true");
  });

  it("switches to a stored, non-active Anthropic key and can delete it", async () => {
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, [ANTHROPIC_SECRET]);
    fireEvent.click(row("row-anthropic").getByRole("button", { name: "Switch" }));
    await waitFor(() =>
      expect(setProviderSelfManagedAction).toHaveBeenCalledWith({ secret_ref: "anthropic-prod", model: "claude-sonnet-4-6" }),
    );

    fireEvent.click(row("row-anthropic").getByRole("button", { name: "Delete anthropic-prod" }));
    await waitFor(() => expect(deleteSecretAction).toHaveBeenCalledWith("ws_1", "anthropic-prod"));
  });

  it("cannot delete the active Anthropic secret", () => {
    renderList(providerOf(), [ANTHROPIC_SECRET]);
    const btn = row("row-anthropic").getByRole("button", { name: /Cannot delete anthropic-prod/ });
    expect(btn.getAttribute("aria-disabled")).toBe("true");
  });

  it("opens the combined edit panel for the active Anthropic secret via the pencil action", () => {
    renderList(providerOf(), [ANTHROPIC_SECRET]);
    const anthropicRow = row("row-anthropic");
    fireEvent.click(anthropicRow.getByRole("button", { name: "Edit" }));
    expect(screen.getByTestId("provider-edit-panel")).toBeTruthy();
    fireEvent.click(screen.getByTestId("edit-close"));
    expect(screen.queryByTestId("provider-edit-panel")).toBeNull();
  });
});

describe("ProviderSwitchList — Other-provider row", () => {
  it("shows the generic label and paste-detect copy when nothing is stored", () => {
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, []);
    expect(screen.getByText("Other provider")).toBeTruthy();
    expect(screen.getByText(/Paste a key/)).toBeTruthy();
  });

  it("names the specific provider currently occupying the slot", () => {
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, [OPENAI_SECRET]);
    expect(screen.getByText("Other provider — OpenAI")).toBeTruthy();
  });

  it("offers a picker across every stored non-Anthropic secret, plus add-another, none hidden", () => {
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, [OPENAI_SECRET, GROQ_SECRET]);
    // First stored one displays as the row's own entry.
    expect(screen.getByText("Other provider — OpenAI")).toBeTruthy();
    // The remaining one is still reachable via the picker, not hidden.
    expect(screen.getByText(/groq/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Add another" })).toBeTruthy();
  });

  it("switches to a secondary stored key from the picker without losing the others", async () => {
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, [OPENAI_SECRET, GROQ_SECRET]);
    const groqRow = screen.getByText(/groq/i).closest("div")!;
    fireEvent.click(within(groqRow).getByRole("button", { name: "Switch" }));
    await waitFor(() =>
      expect(setProviderSelfManagedAction).toHaveBeenCalledWith({ secret_ref: "groq-key", model: undefined }),
    );
    // "without losing the others" means the switch is additive, never a
    // silent delete — assert the sibling secret stays reachable and no
    // delete call rides along with the switch.
    expect(deleteSecretAction).not.toHaveBeenCalled();
    expect(screen.getByText("Other provider — OpenAI")).toBeTruthy();
    expect(screen.getByText(/groq/i)).toBeTruthy();
  });

  it("cannot delete the active other-provider secret but can delete a non-active one", () => {
    renderList(providerOf({ provider: "openai", secret_ref: "openai-key", model: "gpt-4" }), [OPENAI_SECRET, GROQ_SECRET]);
    expect(row("row-other").getByRole("button", { name: /Cannot delete openai-key/ })).toBeTruthy();
    const groqRow = screen.getByText(/groq/i).closest("div")!;
    expect(within(groqRow).getByRole("button", { name: "Delete groq-key" })).toBeTruthy();
  });

  it("add-another opens the generic (unlocked) ProviderKeyForm", () => {
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, [OPENAI_SECRET]);
    fireEvent.click(screen.getByRole("button", { name: "Add another" }));
    expect(screen.getByTestId("provider-key-form").getAttribute("data-provider")).toBe("generic");
  });
});

describe("ProviderSwitchList — Custom endpoint row (unchanged behavior)", () => {
  it("switches to a stored custom endpoint and opens the add form when none exists", async () => {
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, [CUSTOM_ENDPOINT]);
    fireEvent.click(rowOf("OpenAI-compatible").getByRole("button", { name: "Switch" }));
    await waitFor(() => expect(setProviderSelfManagedAction).toHaveBeenCalledWith({ secret_ref: "vllm", model: "m1" }));

    cleanup();
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, []);
    fireEvent.click(row("row-custom").getByRole("button", { name: "Add key" }));
    expect(screen.getByTestId("custom-endpoint-form").getAttribute("data-activate")).toBe("true");
  });
});

describe("ProviderSwitchList — errors and pending state", () => {
  it("surfaces a friendly switch error routed through presentErrorString", async () => {
    setProviderSelfManagedAction.mockResolvedValue({ ok: false, error: "switch failed" });
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, [ANTHROPIC_SECRET]);
    fireEvent.click(row("row-anthropic").getByRole("button", { name: "Switch" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/^Couldn't switch providers/));
  });

  it("surfaces a friendly platform-switch error", async () => {
    resetProviderAction.mockResolvedValue({ ok: false, error: "platform reset failed" });
    renderList(providerOf({ platform_default_available: true }), [ANTHROPIC_SECRET]);
    fireEvent.click(rowOf("Default").getByRole("button", { name: "Switch" }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/^Couldn't switch to platform defaults/),
    );
  });

  it("shows a switching spinner while an action is in flight", async () => {
    let resolveSwitch!: (v: unknown) => void;
    setProviderSelfManagedAction.mockReturnValue(new Promise((r) => (resolveSwitch = r)));
    renderList({ ...providerOf(), mode: PROVIDER_MODE.platform }, [ANTHROPIC_SECRET]);
    fireEvent.click(row("row-anthropic").getByRole("button", { name: "Switch" }));
    await waitFor(() => expect(screen.getByText("Switching")).toBeTruthy());
    resolveSwitch({ ok: true, data: { provider: "anthropic", mode: "self_managed", model: "claude-sonnet-4-6" } });
    await waitFor(() => expect(routerRefresh).toHaveBeenCalled());
  });
});
