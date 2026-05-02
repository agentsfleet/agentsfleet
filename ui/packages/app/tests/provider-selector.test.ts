import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const TOKEN = "tok_provider_test";

const {
  getTokenFn,
  setTenantProviderByokMock,
  resetTenantProviderMock,
  routerRefresh,
} = vi.hoisted(() => ({
  getTokenFn: vi.fn(),
  setTenantProviderByokMock: vi.fn(),
  resetTenantProviderMock: vi.fn(),
  routerRefresh: vi.fn(),
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefresh }),
}));
vi.mock("@/lib/auth/client", () => ({
  useClientToken: () => ({ getToken: getTokenFn }),
}));
vi.mock("@/lib/api/tenant_provider", () => ({
  setTenantProviderByok: setTenantProviderByokMock,
  resetTenantProvider: resetTenantProviderMock,
}));
vi.mock("lucide-react", () => ({
  Loader2Icon: (p: Record<string, unknown>) =>
    React.createElement("svg", { ...p, "data-icon": "Loader2Icon" }),
}));

import ModeRadio from "@/app/(dashboard)/settings/provider/components/ModeRadio";
import ByokFields from "@/app/(dashboard)/settings/provider/components/ByokFields";
import ProviderSelector from "@/app/(dashboard)/settings/provider/components/ProviderSelector";
import { PROVIDER_MODE } from "@/lib/types";

const CRED = { name: "fw-byok", created_at: "2026-04-30T00:00:00Z" } as const;
const WORKSPACE_ID = "ws_provider_test";

beforeEach(() => {
  getTokenFn.mockReset();
  setTenantProviderByokMock.mockReset();
  resetTenantProviderMock.mockReset();
  routerRefresh.mockReset();
});
afterEach(() => cleanup());

// ── ModeRadio (presentational) ──────────────────────────────────────────

describe("ModeRadio", () => {
  it("renders label + description and reflects checked state via data-active", () => {
    const { container } = render(
      React.createElement(ModeRadio, {
        value: PROVIDER_MODE.byok,
        checked: true,
        onChange: () => {},
        label: "Bring your own key",
        description: "your provider, your key.",
      }),
    );
    expect(screen.getByText("Bring your own key")).toBeTruthy();
    expect(container.querySelector("[data-active='true']")).toBeTruthy();
  });

  it("fires onChange when the radio is clicked", () => {
    const onChange = vi.fn();
    render(
      React.createElement(ModeRadio, {
        value: PROVIDER_MODE.platform,
        checked: false,
        onChange,
        label: "Platform-managed",
        description: "we charge from your tenant balance.",
      }),
    );
    fireEvent.click(screen.getByRole("radio"));
    expect(onChange).toHaveBeenCalledTimes(1);
  });
});

// ── ByokFields (presentational) ─────────────────────────────────────────

describe("ByokFields", () => {
  const baseProps = {
    workspaceId: WORKSPACE_ID,
    credentials: [CRED],
    credentialRef: CRED.name,
    onCredentialRefChange: () => {},
    modelOverride: "",
    onModelOverrideChange: () => {},
  };

  it("shows the empty-state CTA linking to /credentials when vault is empty", () => {
    render(React.createElement(ByokFields, { ...baseProps, credentials: [] }));
    expect(screen.getByTestId("byok-no-credentials")).toBeTruthy();
    const link = screen.getByText("Add a credential first") as HTMLAnchorElement;
    expect(link.getAttribute("href")).toBe("/credentials");
    // CTA carries the active workspace id so QA can attribute the click.
    expect(link.getAttribute("data-workspace-id")).toBe(WORKSPACE_ID);
  });

  it("renders a select with one option per credential", () => {
    render(React.createElement(ByokFields, baseProps));
    const select = screen.getByLabelText(/credential/i) as HTMLSelectElement;
    expect(select.value).toBe(CRED.name);
    expect(select.options.length).toBe(1);
  });

  it("propagates credential and model edits to the parent", () => {
    const onCred = vi.fn();
    const onModel = vi.fn();
    render(
      React.createElement(ByokFields, {
        ...baseProps,
        credentials: [CRED, { name: "anth", created_at: CRED.created_at }],
        onCredentialRefChange: onCred,
        onModelOverrideChange: onModel,
      }),
    );
    fireEvent.change(screen.getByLabelText(/credential/i), { target: { value: "anth" } });
    expect(onCred).toHaveBeenCalledWith("anth");
    fireEvent.change(screen.getByLabelText(/model override/i), { target: { value: "claude-sonnet-4-6" } });
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
  };

  it("submits BYOK PUT with the picked credential and refreshes the route", async () => {
    getTokenFn.mockResolvedValue(TOKEN);
    setTenantProviderByokMock.mockResolvedValue({ mode: "byok" });
    render(React.createElement(ProviderSelector, { ...defaultProps }));

    fireEvent.click(screen.getByRole("radio", { name: /bring your own key/i }));
    fireEvent.click(screen.getByRole("button", { name: /save byok config/i }));

    await waitFor(() => expect(setTenantProviderByokMock).toHaveBeenCalledTimes(1));
    expect(setTenantProviderByokMock).toHaveBeenCalledWith(
      { credential_ref: CRED.name, model: undefined },
      TOKEN,
    );
    expect(routerRefresh).toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.getByText(/Switched to BYOK\. Run a test event/)).toBeTruthy(),
    );
  });

  it("calls DELETE on reset to platform default", async () => {
    getTokenFn.mockResolvedValue(TOKEN);
    resetTenantProviderMock.mockResolvedValue({ mode: "platform" });
    render(
      React.createElement(ProviderSelector, {
        ...defaultProps,
        currentMode: PROVIDER_MODE.byok,
        currentCredentialRef: CRED.name,
      }),
    );
    fireEvent.click(screen.getByRole("radio", { name: /platform-managed/i }));
    fireEvent.click(screen.getByRole("button", { name: /reset to platform default/i }));
    await waitFor(() => expect(resetTenantProviderMock).toHaveBeenCalledTimes(1));
  });

  it("surfaces API errors as an alert and does not refresh", async () => {
    getTokenFn.mockResolvedValue(TOKEN);
    setTenantProviderByokMock.mockRejectedValue(new Error("credential_data_malformed"));
    render(React.createElement(ProviderSelector, { ...defaultProps }));
    fireEvent.click(screen.getByRole("radio", { name: /bring your own key/i }));
    fireEvent.click(screen.getByRole("button", { name: /save byok config/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("credential_data_malformed"),
    );
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("returns a 'Not authenticated' alert when getToken resolves null", async () => {
    getTokenFn.mockResolvedValue(null);
    render(React.createElement(ProviderSelector, { ...defaultProps }));
    fireEvent.click(screen.getByRole("button", { name: /reset to platform default/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain("Not authenticated"),
    );
    expect(resetTenantProviderMock).not.toHaveBeenCalled();
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("blocks BYOK submit when no credential is picked", async () => {
    getTokenFn.mockResolvedValue(TOKEN);
    render(
      React.createElement(ProviderSelector, {
        ...defaultProps,
        currentMode: PROVIDER_MODE.byok,
        credentials: [], // empty vault
      }),
    );
    // Submit button is disabled when vault is empty AND mode is byok.
    expect(
      (screen.getByRole("button", { name: /save byok config/i }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(setTenantProviderByokMock).not.toHaveBeenCalled();
  });
});
