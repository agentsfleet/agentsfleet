import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const {
  setProviderSelfManagedActionMock,
  resetProviderActionMock,
  routerRefresh,
  createCredentialActionMock,
  captureProductEventMock,
} = vi.hoisted(() => ({
  setProviderSelfManagedActionMock: vi.fn(),
  resetProviderActionMock: vi.fn(),
  routerRefresh: vi.fn(),
  createCredentialActionMock: vi.fn(),
  captureProductEventMock: vi.fn(),
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh }),
}));
vi.mock("@/app/(dashboard)/settings/models/actions", () => ({
  setProviderSelfManagedAction: setProviderSelfManagedActionMock,
  resetProviderAction: resetProviderActionMock,
}));
vi.mock("@/app/(dashboard)/credentials/actions", () => ({
  createCredentialAction: createCredentialActionMock,
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: captureProductEventMock,
}));
vi.mock("lucide-react", () => ({
  Loader2Icon: (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": "Loader2Icon" }),
}));

import Step1Credential from "@/app/(dashboard)/settings/models/components/Step1Credential";
import Step2Model from "@/app/(dashboard)/settings/models/components/Step2Model";
import ProviderSelector from "@/app/(dashboard)/settings/models/components/ProviderSelector";
import { PROVIDER_MODE } from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { WORKSPACE_CREDENTIALS_PATH } from "@/lib/fleet-credentials";

const CRED = { name: "fw-key", created_at: 1_777_507_200_000 } as const;
const WORKSPACE_ID = "ws_provider_test";

beforeEach(() => {
  setProviderSelfManagedActionMock.mockReset();
  resetProviderActionMock.mockReset();
  routerRefresh.mockReset();
  createCredentialActionMock.mockReset();
  captureProductEventMock.mockReset();
});
afterEach(() => cleanup());

// ── Step1Credential (presentational) ───────────────────────────────────

describe("Step1Credential", () => {
  const baseProps = {
    workspaceId: WORKSPACE_ID,
    credentials: [CRED],
    catalogue: [],
    credentialRef: CRED.name,
    onCredentialRefChange: () => {},
  };

  it("shows the inline create form (no dead-end) plus a manage link when the vault is empty", () => {
    render(React.createElement(Step1Credential, { ...baseProps, credentials: [] }));
    expect(screen.queryByTestId("provider-key-no-credentials")).toBeNull();
    expect(screen.getByText("Add a new provider key")).toBeTruthy();
    const link = screen.getByText("Manage credential vault →") as HTMLAnchorElement;
    // Credentials now live at the top-level vault route.
    expect(link.getAttribute("href")).toBe(WORKSPACE_CREDENTIALS_PATH);
    expect(link.getAttribute("data-workspace-id")).toBe(WORKSPACE_ID);
  });

  it("renders a credential combobox showing the current value", () => {
    render(React.createElement(Step1Credential, baseProps));
    const trigger = screen.getByLabelText(/credential/i);
    expect(trigger.getAttribute("role")).toBe("combobox");
    expect(trigger.textContent).toContain(CRED.name);
  });

  it("propagates credential selection to the parent", () => {
    const onCred = vi.fn();
    render(
      React.createElement(Step1Credential, {
        ...baseProps,
        credentials: [CRED, { name: "anth", created_at: CRED.created_at }],
        onCredentialRefChange: onCred,
      }),
    );
    const trigger = screen.getByLabelText(/credential/i);
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    fireEvent.click(screen.getByText("anth"));
    expect(onCred).toHaveBeenCalledWith("anth");
  });

  it("'+ New key' toggles the inline create form when credentials already exist", () => {
    render(React.createElement(Step1Credential, baseProps));
    expect(screen.queryByText("Add a new provider key")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /new key/i }));
    expect(screen.getByText("Add a new provider key")).toBeTruthy();
  });

  it("selects a freshly created credential from the inline form (empty vault)", async () => {
    createCredentialActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    const onCred = vi.fn();
    render(
      React.createElement(Step1Credential, { ...baseProps, credentials: [], onCredentialRefChange: onCred }),
    );
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "anthropic" } });
    fireEvent.change(screen.getByLabelText(/api key/i), { target: { value: "sk-ant-x" } });
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "claude-sonnet-4-6" } });
    fireEvent.click(screen.getByRole("button", { name: /save key/i }));
    await waitFor(() => expect(onCred).toHaveBeenCalledWith("anthropic"));
  });
});

// ── Step2Model (presentational) ────────────────────────────────────────

describe("Step2Model", () => {
  const MODELS = [
    { id: "claude-sonnet-4-6", provider: "anthropic", context_cap_tokens: 256000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
    { id: "kimi-k2.6", provider: "moonshot", context_cap_tokens: 256000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
  ];

  it("renders a catalogue-backed picker and propagates the picked model", () => {
    const onModel = vi.fn();
    render(React.createElement(Step2Model, { catalogue: MODELS, model: "", onModelChange: onModel }));
    const trigger = screen.getByLabelText(/model/i);
    expect(trigger.getAttribute("role")).toBe("combobox");
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    fireEvent.click(screen.getByText("kimi-k2.6"));
    expect(onModel).toHaveBeenCalledWith("kimi-k2.6");
  });

  it("falls back to a free-text input when the catalogue is empty", () => {
    const onModel = vi.fn();
    render(React.createElement(Step2Model, { catalogue: [], model: "", onModelChange: onModel }));
    const input = screen.getByLabelText(/model/i);
    expect(input.tagName).toBe("INPUT");
    fireEvent.change(input, { target: { value: "claude-sonnet-4-6" } });
    expect(onModel).toHaveBeenCalledWith("claude-sonnet-4-6");
  });

  it("reflects a preselected model and clears back to the credential default", () => {
    const onModel = vi.fn();
    render(React.createElement(Step2Model, { catalogue: MODELS, model: "kimi-k2.6", onModelChange: onModel }));
    const trigger = screen.getByLabelText(/model/i);
    expect(trigger.textContent).toContain("kimi-k2.6");
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    fireEvent.click(screen.getByText(/use the credential's model/i));
    expect(onModel).toHaveBeenCalledWith("");
  });

  it("dedupes a catalogue with the same model_id across providers (regression: duplicate React key)", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const dupCatalogue = [
      { id: "claude-opus-4-8", provider: "anthropic", context_cap_tokens: 256_000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
      { id: "claude-opus-4-8", provider: "pioneer", context_cap_tokens: 256_000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
      { id: "claude-sonnet-4-6", provider: "anthropic", context_cap_tokens: 256_000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
    ];
    const onModel = vi.fn();
    render(React.createElement(Step2Model, { catalogue: dupCatalogue, model: "", onModelChange: onModel }));
    const trigger = screen.getByLabelText(/model/i);
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    expect(screen.getAllByText("claude-opus-4-8")).toHaveLength(1);
    const dupKeyWarning = consoleError.mock.calls.some((args) =>
      args.some((a) => typeof a === "string" && a.includes("same key")),
    );
    expect(dupKeyWarning).toBe(false);
    consoleError.mockRestore();
  });
});

// ── ProviderSelector (orchestration) ────────────────────────────────────

describe("ProviderSelector", () => {
  const defaultProps = {
    workspaceId: WORKSPACE_ID,
    currentMode: PROVIDER_MODE.platform,
    currentCredentialRef: null,
    currentModel: "",
    credentials: [CRED],
    catalogue: [],
  };

  it("test_models_two_option_cards: active card shows Current + no action button; inactive shows the switch action", () => {
    // Platform is the saved mode here → its card is active (Current, no button),
    // the own-key card carries the switch action.
    render(React.createElement(ProviderSelector, { ...defaultProps }));
    const platformCard = screen.getByTestId("option-card-platform");
    const ownKeyCard = screen.getByTestId("option-card-self_managed");

    // Active platform card: "Current" badge + "Active — nothing to do", no button.
    expect(platformCard.textContent).toContain("Current");
    expect(platformCard.querySelector('[data-testid="active-note"]')).toBeTruthy();
    expect(platformCard.querySelector("button")).toBeNull();

    // Inactive own-key card: the switch action button, no "Current" badge.
    expect(ownKeyCard.textContent).not.toContain("Current");
    expect(ownKeyCard.querySelector('[data-testid="active-note"]')).toBeNull();
    expect(screen.getByRole("button", { name: /switch to own key/i })).toBeTruthy();
    // The active option carries no action; the only card-level action is the switch.
    expect(screen.queryByRole("button", { name: /use platform defaults/i })).toBeNull();
  });

  it("marks own-key as Current and surfaces the platform switch action when self-managed is saved", () => {
    render(
      React.createElement(ProviderSelector, {
        ...defaultProps,
        currentMode: PROVIDER_MODE.self_managed,
        currentCredentialRef: CRED.name,
      }),
    );
    const ownKeyCard = screen.getByTestId("option-card-self_managed");
    expect(ownKeyCard.textContent).toContain("Current");
    expect(ownKeyCard.querySelector('[data-testid="active-note"]')).toBeTruthy();
    expect(ownKeyCard.querySelector("button")).toBeNull();
    expect(screen.getByRole("button", { name: /use platform defaults/i })).toBeTruthy();
  });

  it("'Switch to own key' reveals the config form; Cancel returns to the cards", () => {
    render(React.createElement(ProviderSelector, { ...defaultProps }));
    expect(screen.queryByText("Own-key model setup")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /switch to own key/i }));
    expect(screen.getByText("Own-key model setup")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /^cancel$/i }));
    expect(screen.queryByText("Own-key model setup")).toBeNull();
  });

  it("saves the own-key setup → setProviderSelfManaged + analytics + refresh", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({
      ok: true,
      data: { mode: PROVIDER_MODE.self_managed, provider: "anthropic", model: "claude-sonnet-4-6" },
    });
    render(React.createElement(ProviderSelector, { ...defaultProps }));
    fireEvent.click(screen.getByRole("button", { name: /switch to own key/i }));
    fireEvent.click(screen.getByRole("button", { name: /save model setup/i }));

    await waitFor(() => expect(setProviderSelfManagedActionMock).toHaveBeenCalledTimes(1));
    expect(setProviderSelfManagedActionMock).toHaveBeenCalledWith({
      credential_ref: CRED.name,
      model: undefined,
    });
    expect(routerRefresh).toHaveBeenCalled();
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.model_added, {
      provider: "anthropic",
      mode: PROVIDER_MODE.self_managed,
      model: "claude-sonnet-4-6",
    });
    await waitFor(() => expect(screen.getByText(/Saved\. Run a test event/)).toBeTruthy());
  });

  it("clicking 'Use platform defaults' calls reset and refreshes (no add event)", async () => {
    resetProviderActionMock.mockResolvedValue({ ok: true, data: { mode: PROVIDER_MODE.platform } });
    render(
      React.createElement(ProviderSelector, {
        ...defaultProps,
        currentMode: PROVIDER_MODE.self_managed,
        currentCredentialRef: CRED.name,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /use platform defaults/i }));
    await waitFor(() => expect(resetProviderActionMock).toHaveBeenCalledTimes(1));
    expect(routerRefresh).toHaveBeenCalled();
    expect(captureProductEventMock).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText(/Using platform defaults/)).toBeTruthy());
  });

  it("surfaces a self-managed save error as an alert and does not refresh", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({
      ok: false,
      error: "credential_data_malformed",
      status: 400,
    });
    render(React.createElement(ProviderSelector, { ...defaultProps }));
    fireEvent.click(screen.getByRole("button", { name: /switch to own key/i }));
    fireEvent.click(screen.getByRole("button", { name: /save model setup/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("credential_data_malformed"),
    );
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(captureProductEventMock).not.toHaveBeenCalled();
  });

  it("surfaces a platform-reset error as an alert and does not refresh", async () => {
    resetProviderActionMock.mockResolvedValue({ ok: false, error: "Not authenticated", status: 401 });
    render(
      React.createElement(ProviderSelector, {
        ...defaultProps,
        currentMode: PROVIDER_MODE.self_managed,
        currentCredentialRef: CRED.name,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /use platform defaults/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("Not authenticated"),
    );
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("disables Save model setup while the own-key form has no credential", () => {
    render(
      React.createElement(ProviderSelector, {
        ...defaultProps,
        currentMode: PROVIDER_MODE.platform,
        credentials: [], // empty vault
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /switch to own key/i }));
    const save = screen.getByRole("button", { name: /save model setup/i }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    expect(setProviderSelfManagedActionMock).not.toHaveBeenCalled();
  });
});
