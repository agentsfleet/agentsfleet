import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Secret } from "@/lib/api/secrets";

// Split from models-registry-add.test.tsx (RULE FLL — the combined file grew
// past the 350-line cap): AddModelEntryDialog's "Custom endpoint" tab only.
// Known-provider + reuse-existing-key coverage lives in the sibling file.

const createModelEntryActionMock = vi.fn();
const setProviderSelfManagedActionMock = vi.fn();
const createSecretActionMock = vi.fn();

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  createModelEntryAction: createModelEntryActionMock,
  setProviderSelfManagedAction: setProviderSelfManagedActionMock,
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({
  createSecretAction: createSecretActionMock,
}));
vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: vi.fn(), push: vi.fn() }) }));

async function renderDialog() {
  const { default: AddModelEntryDialog } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog"
  );
  const onCreated = vi.fn();
  render(React.createElement(AddModelEntryDialog, { workspaceId: "ws_1", secrets: [] as Secret[], onCreated } as never));
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: /add model/i }));
  await screen.findByRole("dialog");
  return { onCreated, user };
}

beforeEach(() => {
  vi.clearAllMocks();
  createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
  createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: 1 } });
  setProviderSelfManagedActionMock.mockResolvedValue({
    ok: true,
    data: { mode: "self_managed", provider: "anthropic", model: "claude-sonnet-5", context_cap_tokens: 200000, secret_ref: "anthropic", platform_default_available: true },
  });
});
afterEach(() => cleanup());

describe("AddModelEntryDialog — custom endpoint", () => {
  it("registers a keyless entry and activates it on Save & make active", async () => {
    const { onCreated, user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByRole("tab", { name: /custom endpoint/i }));
    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    await user.type(within(dialog).getByLabelText(/base url/i), "https://vllm.corp/v1");
    // API key left blank — keyless endpoint.
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");

    await user.click(within(dialog).getByRole("button", { name: /save & make active/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalled());
    const [, body] = createSecretActionMock.mock.calls[0] as [string, { name: string; data: Record<string, unknown> }];
    expect(body.name).toBe("vllm-gateway");
    expect(body.data.api_key).toBeUndefined();
    expect(body.data.base_url).toBe("https://vllm.corp/v1");

    await waitFor(() =>
      expect(createModelEntryActionMock).toHaveBeenCalledWith({ model_id: "vllm-model", secret_ref: "vllm-gateway" }),
    );
    await waitFor(() =>
      expect(setProviderSelfManagedActionMock).toHaveBeenCalledWith({ secret_ref: "vllm-gateway", model: "vllm-model" }),
    );
    await waitFor(() => expect(onCreated).toHaveBeenCalled());
  });

  it("disables Save when the custom shape is incomplete", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByRole("tab", { name: /custom endpoint/i }));
    expect(within(dialog).getByRole("button", { name: /^save$/i }).hasAttribute("disabled")).toBe(true);

    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    // Name filled, base URL + model still empty.
    expect(within(dialog).getByRole("button", { name: /^save$/i }).hasAttribute("disabled")).toBe(true);

    expect(createSecretActionMock).not.toHaveBeenCalled();
  });

  it("surfaces a register error for the custom shape without activating", async () => {
    createModelEntryActionMock.mockResolvedValue({ ok: false, error: "duplicate", errorCode: "UZ-MODELS-003" });
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByRole("tab", { name: /custom endpoint/i }));
    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    await user.type(within(dialog).getByLabelText(/base url/i), "https://vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");
    await user.click(within(dialog).getByRole("button", { name: /save & make active/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(setProviderSelfManagedActionMock).not.toHaveBeenCalled();
  });

  it("rejects a non-https base URL inline, without calling the backend", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByRole("tab", { name: /custom endpoint/i }));
    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    await user.type(within(dialog).getByLabelText(/base url/i), "http://vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");

    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert").textContent).toMatch(/use https:\/\/ for the base url/i));
    expect(createSecretActionMock).not.toHaveBeenCalled();
  });

  it("carries a pasted key through in the secret body when one is entered", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByRole("tab", { name: /custom endpoint/i }));
    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    await user.type(within(dialog).getByLabelText(/base url/i), "https://vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/api key \(optional\)/i), "sk-vllm-key");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalled());
    const [, body] = createSecretActionMock.mock.calls[0] as [string, { data: Record<string, unknown> }];
    expect(body.data.api_key).toBe("sk-vllm-key");
  });

  it("surfaces a store error for the custom shape", async () => {
    createSecretActionMock.mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-VAULT-002" });
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByRole("tab", { name: /custom endpoint/i }));
    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    await user.type(within(dialog).getByLabelText(/base url/i), "https://vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });
});
