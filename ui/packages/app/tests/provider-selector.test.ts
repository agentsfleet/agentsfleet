import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const {
  setProviderSelfManagedActionMock,
  resetProviderActionMock,
  routerRefresh,
} = vi.hoisted(() => ({
  setProviderSelfManagedActionMock: vi.fn(),
  resetProviderActionMock: vi.fn(),
  routerRefresh: vi.fn(),
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh }),
}));
vi.mock("@/app/(dashboard)/settings/models/actions", () => ({
  setProviderSelfManagedAction: setProviderSelfManagedActionMock,
  resetProviderAction: resetProviderActionMock,
}));
vi.mock("@/app/(dashboard)/credentials/actions", () => ({
  createCredentialAction: vi.fn(),
}));
vi.mock("lucide-react", () => ({
  Loader2Icon: (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": "Loader2Icon" }),
}));

import ModeRadio from "@/app/(dashboard)/settings/models/components/ModeRadio";
import Step1Credential from "@/app/(dashboard)/settings/models/components/Step1Credential";
import Step2Model from "@/app/(dashboard)/settings/models/components/Step2Model";
import ProviderSelector from "@/app/(dashboard)/settings/models/components/ProviderSelector";
import { RadioGroup } from "@usezombie/design-system";
import { PROVIDER_MODE } from "@/lib/types";

// ModeRadio renders a Radix RadioGroupItem internally; render it inside a
// RadioGroup at the test boundary to mirror the production composition.
function inRadioGroup(children: React.ReactNode, value: string) {
  return React.createElement(RadioGroup, { value, onValueChange: () => {} }, children);
}

const CRED = { name: "fw-key", created_at: 1_777_507_200_000 } as const;
const WORKSPACE_ID = "ws_provider_test";

beforeEach(() => {
  setProviderSelfManagedActionMock.mockReset();
  resetProviderActionMock.mockReset();
  routerRefresh.mockReset();
});
afterEach(() => cleanup());

// ── ModeRadio (presentational) ──────────────────────────────────────────

describe("ModeRadio", () => {
  it("renders label + description and reflects checked state via data-active", () => {
    const { container } = render(
      inRadioGroup(
        React.createElement(ModeRadio, {
          value: PROVIDER_MODE.self_managed,
          checked: true,
          label: "Use my own provider key",
          description: "your provider, your key.",
        }),
        PROVIDER_MODE.self_managed,
      ),
    );
    expect(screen.getByText("Use my own provider key")).toBeTruthy();
    expect(container.querySelector("[data-active='true']")).toBeTruthy();
  });

  it("delegates click selection to the parent RadioGroup's onValueChange", () => {
    const onValueChange = vi.fn();
    render(
      React.createElement(
        RadioGroup,
        {
          defaultValue: PROVIDER_MODE.self_managed,
          onValueChange,
          "aria-label": "wrap",
        },
        React.createElement(ModeRadio, {
          value: PROVIDER_MODE.platform,
          checked: false,
          label: "Platform-managed",
          description: "we charge from your tenant balance.",
        }),
      ),
    );
    fireEvent.click(screen.getByRole("radio"));
    // ModeRadio no longer carries its own onClick; the parent's
    // onValueChange owns selection. Asserting via the RadioGroup
    // callback proves the delegation, not the redundant double-fire.
    expect(onValueChange).toHaveBeenCalledWith(PROVIDER_MODE.platform);
    expect(onValueChange).toHaveBeenCalledTimes(1);
  });
});

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
    // The old empty-vault dead-end Alert is gone — an inline create form shows instead.
    expect(screen.queryByTestId("provider-key-no-credentials")).toBeNull();
    expect(screen.getByText("Add a new provider key")).toBeTruthy();
    const link = screen.getByText("Manage all credentials →") as HTMLAnchorElement;
    expect(link.getAttribute("href")).toBe("/credentials");
    // The secondary link carries the active workspace id so QA can attribute the click.
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

  it("submits self-managed PUT with the picked credential and refreshes the route", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({
      ok: true,
      data: { mode: PROVIDER_MODE.self_managed },
    });
    render(React.createElement(ProviderSelector, { ...defaultProps }));

    fireEvent.click(screen.getByRole("radio", { name: /use my own provider key/i }));
    fireEvent.click(screen.getByRole("button", { name: /save self-managed key/i }));

    await waitFor(() => expect(setProviderSelfManagedActionMock).toHaveBeenCalledTimes(1));
    expect(setProviderSelfManagedActionMock).toHaveBeenCalledWith({
      credential_ref: CRED.name,
      model: undefined,
    });
    expect(routerRefresh).toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.getByText(/Switched to self-managed\. Run a test event/)).toBeTruthy(),
    );
  });

  it("calls DELETE on reset to platform default", async () => {
    resetProviderActionMock.mockResolvedValue({
      ok: true,
      data: { mode: PROVIDER_MODE.platform },
    });
    render(
      React.createElement(ProviderSelector, {
        ...defaultProps,
        currentMode: PROVIDER_MODE.self_managed,
        currentCredentialRef: CRED.name,
      }),
    );
    fireEvent.click(screen.getByRole("radio", { name: /platform-managed/i }));
    fireEvent.click(screen.getByRole("button", { name: /reset to platform default/i }));
    await waitFor(() => expect(resetProviderActionMock).toHaveBeenCalledTimes(1));
  });

  it("surfaces API errors as an alert and does not refresh", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({
      ok: false,
      error: "credential_data_malformed",
      status: 400,
    });
    render(React.createElement(ProviderSelector, { ...defaultProps }));
    fireEvent.click(screen.getByRole("radio", { name: /use my own provider key/i }));
    fireEvent.click(screen.getByRole("button", { name: /save self-managed key/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("credential_data_malformed"),
    );
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("returns a 'Not authenticated' alert when the server action reports unauth", async () => {
    resetProviderActionMock.mockResolvedValue({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    render(React.createElement(ProviderSelector, { ...defaultProps }));
    fireEvent.click(screen.getByRole("button", { name: /reset to platform default/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("Not authenticated"),
    );
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("blocks self-managed submit when no credential is picked", async () => {
    render(
      React.createElement(ProviderSelector, {
        ...defaultProps,
        currentMode: PROVIDER_MODE.self_managed,
        credentials: [], // empty vault
      }),
    );
    // Submit button is disabled when vault is empty AND mode is self_managed.
    expect(
      (screen.getByRole("button", { name: /save self-managed key/i }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(setProviderSelfManagedActionMock).not.toHaveBeenCalled();
  });
});
