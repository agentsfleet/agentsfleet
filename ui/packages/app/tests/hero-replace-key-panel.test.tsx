import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

// Hero "Replace key" panel: PATCH-rotates only the secret of the active
// credential — provider + model are preserved. Save → rotateSecretAction +
// key_rotated (provider id only, no secret); onClose runs on success. The footer
// reassures the model is unchanged.

const routerRefresh = vi.fn();
const rotateSecretAction = vi.hoisted(() => vi.fn());
const captureKeyRotated = vi.hoisted(() => vi.fn());

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh, push: vi.fn() }) }));
vi.mock("@/app/(dashboard)/settings/models/actions", () => ({ rotateSecretAction }));
vi.mock("@/app/(dashboard)/settings/models/lib/track", () => ({ captureKeyRotated }));
vi.mock("@agentsfleet/design-system", async () => (await import("./helpers/models-component-mocks")).designSystemStub());

import HeroReplaceKeyPanel from "@/app/(dashboard)/settings/models/components/HeroReplaceKeyPanel";

beforeEach(() => {
  vi.clearAllMocks();
  rotateSecretAction.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
});
afterEach(() => cleanup());

function renderPanel(onClose = vi.fn()) {
  render(
    React.createElement(HeroReplaceKeyPanel, {
      workspaceId: "ws_1",
      secretRef: "anthropic-prod",
      provider: "anthropic",
      currentModel: "claude-sonnet-4-6",
      onClose,
    }),
  );
  return onClose;
}

describe("HeroReplaceKeyPanel", () => {
  it("rotates the secret and closes; the footer pins the current model", async () => {
    const onClose = renderPanel();
    expect(screen.getByText(/Model stays claude-sonnet-4-6/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText("New API key"), { target: { value: "sk-ant-new" } });
    fireEvent.click(screen.getByRole("button", { name: "Save key" }));
    await waitFor(() =>
      expect(rotateSecretAction).toHaveBeenCalledWith("ws_1", "anthropic-prod", "sk-ant-new"),
    );
    expect(captureKeyRotated).toHaveBeenCalledWith("anthropic");
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(routerRefresh).toHaveBeenCalled();
  });

  it("surfaces a rotation error and keeps the panel open", async () => {
    rotateSecretAction.mockResolvedValue({ ok: false, error: "key too short" });
    const onClose = renderPanel();
    fireEvent.change(screen.getByLabelText("New API key"), { target: { value: "sk-bad" } });
    fireEvent.click(screen.getByRole("button", { name: "Save key" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/key too short/));
    expect(captureKeyRotated).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("does not rotate an empty key and cancels via the Cancel button", () => {
    const onClose = renderPanel();
    fireEvent.click(screen.getByRole("button", { name: "Save key" }));
    expect(rotateSecretAction).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
  });
});
