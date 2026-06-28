import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { EVENTS } from "@/lib/analytics/events";

// Consolidated "add a provider key" form. Locked mode (a switch-list row) hides
// the provider field; generic mode shows it and fills it from a pasted key's
// prefix. On save it stores `{provider, api_key, model}`; with `activate` it also
// points the tenant provider at the new credential.

const routerRefresh = vi.fn();
const createCredentialAction = vi.hoisted(() => vi.fn());
const setProviderSelfManagedAction = vi.hoisted(() => vi.fn());
const captureModelActivated = vi.hoisted(() => vi.fn());
const captureProductEvent = vi.hoisted(() => vi.fn());

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }) }));
vi.mock("@/app/(dashboard)/credentials/actions", () => ({ createCredentialAction }));
vi.mock("@/app/(dashboard)/settings/models/actions", () => ({ setProviderSelfManagedAction }));
vi.mock("@/app/(dashboard)/settings/models/lib/track", () => ({ captureModelActivated }));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent }));
vi.mock("@agentsfleet/design-system", async () => (await import("./helpers/models-component-mocks")).designSystemStub());
vi.mock("@/app/(dashboard)/settings/models/components/ProviderModelSelect", async () => (await import("./helpers/models-component-mocks")).providerModelSelectStub());

import ProviderKeyForm from "@/app/(dashboard)/settings/models/components/ProviderKeyForm";

beforeEach(() => {
  vi.clearAllMocks();
  createCredentialAction.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
  setProviderSelfManagedAction.mockResolvedValue({
    ok: true,
    data: { provider: "anthropic", mode: "self_managed", model: "m1" },
  });
});
afterEach(() => cleanup());

function fill(apiKey: string, model: string) {
  fireEvent.change(screen.getByLabelText("API key"), { target: { value: apiKey } });
  fireEvent.change(screen.getByLabelText("Model"), { target: { value: model } });
}

describe("ProviderKeyForm — locked mode", () => {
  it("hides the provider field and stores {provider, api_key, model}; no activation", async () => {
    const onDone = vi.fn();
    render(React.createElement(ProviderKeyForm, { workspaceId: "ws_1", provider: "anthropic", onDone }));
    expect(screen.queryByLabelText("Provider")).toBeNull();
    fill("sk-x", "m1");
    fireEvent.click(screen.getByRole("button", { name: "Save key" }));
    await waitFor(() =>
      expect(createCredentialAction).toHaveBeenCalledWith("ws_1", {
        name: "anthropic",
        data: { provider: "anthropic", api_key: "sk-x", model: "m1" },
      }),
    );
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.credential_added, { credential_name: "anthropic" });
    expect(setProviderSelfManagedAction).not.toHaveBeenCalled();
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("activates the credential as the tenant provider when `activate`", async () => {
    const onDone = vi.fn();
    render(
      React.createElement(ProviderKeyForm, { workspaceId: "ws_1", provider: "anthropic", activate: true, onDone }),
    );
    fill("sk-x", "m1");
    fireEvent.click(screen.getByRole("button", { name: "Save & make active" }));
    await waitFor(() =>
      expect(setProviderSelfManagedAction).toHaveBeenCalledWith({ credential_ref: "anthropic", model: "m1" }),
    );
    expect(captureModelActivated).toHaveBeenCalledWith({ provider: "anthropic", mode: "self_managed", model: "m1" });
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it("surfaces a store error and does not activate", async () => {
    createCredentialAction.mockResolvedValue({ ok: false, error: "boom" });
    const onDone = vi.fn();
    render(
      React.createElement(ProviderKeyForm, { workspaceId: "ws_1", provider: "anthropic", activate: true, onDone }),
    );
    fill("sk-x", "m1");
    fireEvent.click(screen.getByRole("button", { name: "Save & make active" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/Couldn't store the provider key/i));
    expect(setProviderSelfManagedAction).not.toHaveBeenCalled();
    expect(onDone).not.toHaveBeenCalled();
  });

  it("surfaces an activation error after a successful store", async () => {
    setProviderSelfManagedAction.mockResolvedValue({ ok: false, error: "activation rejected" });
    const onDone = vi.fn();
    render(
      React.createElement(ProviderKeyForm, { workspaceId: "ws_1", provider: "anthropic", activate: true, onDone }),
    );
    fill("sk-x", "m1");
    fireEvent.click(screen.getByRole("button", { name: "Save & make active" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/activation rejected/));
    expect(captureModelActivated).not.toHaveBeenCalled();
    expect(onDone).not.toHaveBeenCalled();
  });

  it("does nothing when the form is incomplete (Enter on an empty field)", () => {
    render(React.createElement(ProviderKeyForm, { workspaceId: "ws_1", provider: "anthropic", onDone: vi.fn() }));
    fireEvent.keyDown(screen.getByLabelText("API key"), { key: "Enter" });
    expect(createCredentialAction).not.toHaveBeenCalled();
  });

  it("renders a Cancel affordance when onCancel is provided", () => {
    const onCancel = vi.fn();
    render(
      React.createElement(ProviderKeyForm, { workspaceId: "ws_1", provider: "anthropic", onDone: vi.fn(), onCancel }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });
});

describe("ProviderKeyForm — generic mode", () => {
  it("paste-detects the provider from the key prefix, and submits via Enter", async () => {
    const onDone = vi.fn();
    render(React.createElement(ProviderKeyForm, { workspaceId: "ws_1", onDone }));
    // Generic mode shows the provider field.
    const providerInput = screen.getByLabelText("Provider") as HTMLInputElement;
    expect(providerInput).toBeTruthy();
    // Paste an anthropic-shaped key → provider auto-fills.
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: "sk-ant-xyz" } });
    expect((screen.getByLabelText("Provider") as HTMLInputElement).value).toBe("anthropic");
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "m1" } });
    // Submit via Enter on the model-less path is fine; use the api-key field.
    fireEvent.keyDown(screen.getByLabelText("API key"), { key: "Enter" });
    await waitFor(() =>
      expect(createCredentialAction).toHaveBeenCalledWith("ws_1", {
        name: "anthropic",
        data: { provider: "anthropic", api_key: "sk-ant-xyz", model: "m1" },
      }),
    );
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it("lets the provider be typed manually, resetting the model", () => {
    render(React.createElement(ProviderKeyForm, { workspaceId: "ws_1", onDone: vi.fn() }));
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "m1" } });
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "fireworks" } });
    expect((screen.getByLabelText("Provider") as HTMLInputElement).value).toBe("fireworks");
    // Typing a provider clears the model selection.
    expect((screen.getByLabelText("Model") as HTMLInputElement).value).toBe("");
    // A non-Enter key in a field is ignored.
    fireEvent.keyDown(screen.getByLabelText("API key"), { key: "a" });
    expect(createCredentialAction).not.toHaveBeenCalled();
  });

  it("leaves the provider untouched for an unrecognised key, and when re-detecting the same provider", () => {
    render(React.createElement(ProviderKeyForm, { workspaceId: "ws_1", onDone: vi.fn() }));
    const providerInput = () => screen.getByLabelText("Provider") as HTMLInputElement;
    // No recognised prefix → detect returns null, provider stays empty.
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: "mystery-key" } });
    expect(providerInput().value).toBe("");
    // Detect anthropic, then re-paste another anthropic key: detected === current
    // provider, so the provider/model are left as-is.
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: "sk-ant-1" } });
    expect(providerInput().value).toBe("anthropic");
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "m1" } });
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: "sk-ant-2" } });
    expect(providerInput().value).toBe("anthropic");
    expect((screen.getByLabelText("Model") as HTMLInputElement).value).toBe("m1");
  });
});
