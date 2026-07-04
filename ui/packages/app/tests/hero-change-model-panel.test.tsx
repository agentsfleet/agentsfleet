import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

// Hero "Change model" panel: re-point the active secret at a different model
// from this provider's catalogue (same key). Save → setProviderSelfManagedAction
// + model_changed (NOT model_added); onClose runs on success.

const routerRefresh = vi.fn();
const setProviderSelfManagedAction = vi.hoisted(() => vi.fn());
const captureModelChanged = vi.hoisted(() => vi.fn());

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }) }));
vi.mock("@/app/(dashboard)/settings/models/actions", () => ({ setProviderSelfManagedAction }));
vi.mock("@/app/(dashboard)/settings/models/lib/track", () => ({ captureModelChanged }));
vi.mock("@agentsfleet/design-system", async () => (await import("./helpers/models-component-mocks")).designSystemStub());
vi.mock("@/app/(dashboard)/settings/models/components/ProviderModelSelect", async () => (await import("./helpers/models-component-mocks")).providerModelSelectStub());

import HeroChangeModelPanel from "@/app/(dashboard)/settings/models/components/HeroChangeModelPanel";

beforeEach(() => {
  vi.clearAllMocks();
  setProviderSelfManagedAction.mockResolvedValue({
    ok: true,
    data: { provider: "anthropic", mode: "self_managed", model: "m2" },
  });
});
afterEach(() => cleanup());

function renderPanel(onClose = vi.fn()) {
  render(
    React.createElement(HeroChangeModelPanel, { provider: "anthropic", secretRef: "anthropic-prod", onClose }),
  );
  return onClose;
}

describe("HeroChangeModelPanel", () => {
  it("saves the new model against the same key and closes", async () => {
    const onClose = renderPanel();
    fireEvent.change(screen.getByLabelText("Change model"), { target: { value: "m2" } });
    fireEvent.click(screen.getByRole("button", { name: "Save model" }));
    await waitFor(() =>
      expect(setProviderSelfManagedAction).toHaveBeenCalledWith({ secret_ref: "anthropic-prod", model: "m2" }),
    );
    expect(captureModelChanged).toHaveBeenCalledWith({ provider: "anthropic", mode: "self_managed", model: "m2" });
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("surfaces an error and keeps the panel open", async () => {
    setProviderSelfManagedAction.mockResolvedValue({ ok: false, error: "model not in catalogue" });
    const onClose = renderPanel();
    fireEvent.change(screen.getByLabelText("Change model"), { target: { value: "ghost" } });
    fireEvent.click(screen.getByRole("button", { name: "Save model" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/model not in catalogue/));
    expect(captureModelChanged).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("does not save an empty model and cancels via the Cancel button", () => {
    const onClose = renderPanel();
    // Empty model: the Save button is gated, so no action fires.
    fireEvent.click(screen.getByRole("button", { name: "Save model" }));
    expect(setProviderSelfManagedAction).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
  });
});
