import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

// ProviderEditPanel: the single combined edit surface for an active row —
// replaces the old two-button (Change model / Replace key) split. Leaving the
// key blank keeps the current secret; leaving the model unchanged keeps the
// current model. Save fires only the calls whose field actually changed.

const routerRefresh = vi.fn();
const rotateSecretAction = vi.hoisted(() => vi.fn());
const setProviderSelfManagedAction = vi.hoisted(() => vi.fn());
const captureKeyRotated = vi.hoisted(() => vi.fn());
const captureModelChanged = vi.hoisted(() => vi.fn());

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }) }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  rotateSecretAction,
  setProviderSelfManagedAction,
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/lib/track", () => ({
  captureKeyRotated,
  captureModelChanged,
}));
vi.mock("@agentsfleet/design-system", async () => (await import("./helpers/models-component-mocks")).designSystemStub());
vi.mock(
  "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderModelSelect",
  async () => (await import("./helpers/models-component-mocks")).providerModelSelectStub(),
);

import ProviderEditPanel from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderEditPanel";

beforeEach(() => {
  vi.clearAllMocks();
  rotateSecretAction.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
  setProviderSelfManagedAction.mockResolvedValue({
    ok: true,
    data: { provider: "anthropic", mode: "self_managed", model: "m2" },
  });
});
afterEach(() => cleanup());

function renderPanel(onClose = vi.fn()) {
  render(
    React.createElement(ProviderEditPanel, {
      workspaceId: "ws_1",
      provider: "anthropic",
      secretRef: "anthropic-prod",
      currentModel: "claude-sonnet-4-6",
      onClose,
    }),
  );
  return onClose;
}

describe("ProviderEditPanel", () => {
  it("rotates only the key when the model is left unchanged", async () => {
    const onClose = renderPanel();
    fireEvent.change(screen.getByLabelText("New API key"), { target: { value: "sk-ant-new" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(rotateSecretAction).toHaveBeenCalledWith("ws_1", "anthropic-prod", "sk-ant-new"));
    expect(captureKeyRotated).toHaveBeenCalledWith("anthropic");
    expect(setProviderSelfManagedAction).not.toHaveBeenCalled();
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("changes only the model when the key is left blank", async () => {
    const onClose = renderPanel();
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "m2" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(setProviderSelfManagedAction).toHaveBeenCalledWith({ secret_ref: "anthropic-prod", model: "m2" }),
    );
    expect(captureModelChanged).toHaveBeenCalledWith({ provider: "anthropic", mode: "self_managed", model: "m2" });
    expect(rotateSecretAction).not.toHaveBeenCalled();
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("rotates the key and changes the model together in one save", async () => {
    const onClose = renderPanel();
    fireEvent.change(screen.getByLabelText("New API key"), { target: { value: "sk-ant-new" } });
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "m2" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(rotateSecretAction).toHaveBeenCalledWith("ws_1", "anthropic-prod", "sk-ant-new"));
    await waitFor(() =>
      expect(setProviderSelfManagedAction).toHaveBeenCalledWith({ secret_ref: "anthropic-prod", model: "m2" }),
    );
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("disables Save when neither the key nor the model changed", () => {
    renderPanel();
    expect(screen.getByRole("button", { name: "Save" }).getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(rotateSecretAction).not.toHaveBeenCalled();
    expect(setProviderSelfManagedAction).not.toHaveBeenCalled();
  });

  it("does not save an empty model and cancels via the Cancel button", () => {
    const onClose = renderPanel();
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "" } });
    fireEvent.change(screen.getByLabelText("New API key"), { target: { value: "sk-ant-new" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(rotateSecretAction).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("surfaces a friendly key-rotation error, keeps the panel open, and never reaches the model update", async () => {
    rotateSecretAction.mockResolvedValue({ ok: false, error: "key too short" });
    const onClose = renderPanel();
    fireEvent.change(screen.getByLabelText("New API key"), { target: { value: "sk-bad" } });
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "m2" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/^Couldn't update the key and model/));
    expect(screen.getByRole("alert").textContent).toMatch(/key too short/);
    expect(setProviderSelfManagedAction).not.toHaveBeenCalled();
    expect(captureKeyRotated).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("surfaces a friendly model-update error and keeps the panel open", async () => {
    setProviderSelfManagedAction.mockResolvedValue({ ok: false, error: "model not in catalogue" });
    const onClose = renderPanel();
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "ghost" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/^Couldn't update the key and model/));
    expect(screen.getByRole("alert").textContent).toMatch(/model not in catalogue/);
    expect(captureModelChanged).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });
});
