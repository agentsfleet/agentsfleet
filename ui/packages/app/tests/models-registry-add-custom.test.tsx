import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Secret } from "@/lib/api/secrets";

// Split from models-registry-add.test.tsx (RULE FLL — the combined file grew
// past the 350-line cap): the OpenAI-compatible provider path only (the
// unified form's custom-endpoint shape — Base URL reveal, optional key).
// Known-provider + reuse-existing-key coverage lives in the sibling file.

const createModelEntryActionMock = vi.fn();
const setProviderSelfManagedActionMock = vi.fn();
const createSecretActionMock = vi.fn();

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  createModelEntryAction: createModelEntryActionMock,
  setProviderSelfManagedAction: setProviderSelfManagedActionMock,
  rotateSecretAction: vi.fn(),
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({
  createSecretAction: createSecretActionMock,
}));

// One library row so the Provider field renders as the catalogue-backed
// <Select> — the OpenAI-compatible option is appended to it either way.
const { catalogueState } = vi.hoisted(() => ({
  catalogueState: {
    models: [
      { id: "claude-sonnet-5", provider: "anthropic", context_cap_tokens: 200000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
    ],
    loading: false,
    error: false,
  },
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider", () => ({
  useModelCatalogue: () => catalogueState,
}));

async function renderDialog(secrets: Secret[] = []) {
  const { default: AddModelEntryDialog } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog"
  );
  const onCreated = vi.fn();
  const onSecretsChanged = vi.fn();
  render(
    React.createElement(AddModelEntryDialog, {
      workspaceId: "ws_1",
      secrets,
      onCreated,
      onSecretsChanged,
    } as never),
  );
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: /add model/i }));
  await screen.findByRole("dialog");
  return { onCreated, onSecretsChanged, user };
}

/** Picks "Custom — OpenAI-compatible" in the Provider dropdown — the move
 * that replaces the old Custom-endpoint tab and reveals the Base URL field. */
async function pickCustomProvider(user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement) {
  await user.click(within(dialog).getByLabelText(/^provider$/i));
  await user.click(await screen.findByRole("option", { name: /custom — openai-compatible/i }));
}

beforeEach(() => {
  vi.clearAllMocks();
  createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "vllm-gateway" } });
  createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "vllm-model", secret_ref: "vllm-gateway", created_at: 1 } });
  setProviderSelfManagedActionMock.mockResolvedValue({
    ok: true,
    data: { mode: "self_managed", provider: "openai-compatible", model: "vllm-model", context_cap_tokens: 32000, secret_ref: "vllm-gateway", platform_default_available: true },
  });
});
afterEach(() => cleanup());

describe("AddModelEntryDialog — OpenAI-compatible provider", () => {
  it("reveals Base URL only after the OpenAI-compatible provider is picked, and marks the key optional", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    expect(within(dialog).queryByLabelText(/base url/i)).toBeNull();
    expect(within(dialog).getByLabelText(/^api key$/i)).toBeTruthy();

    await pickCustomProvider(user, dialog);

    expect(within(dialog).getByLabelText(/base url/i)).toBeTruthy();
    expect(within(dialog).getByLabelText(/api key \(optional\)/i)).toBeTruthy();
  });

  it("registers a keyless entry and activates it on Save & make active", async () => {
    const { onCreated, user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    await pickCustomProvider(user, dialog);
    await user.type(within(dialog).getByLabelText(/base url/i), "https://vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");
    // API key left blank — keyless endpoint.

    await user.click(within(dialog).getByRole("button", { name: /save & make active/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalled());
    const [, body] = createSecretActionMock.mock.calls[0] as [string, { name: string; data: Record<string, unknown> }];
    expect(body.name).toBe("vllm-gateway");
    expect(body.data.api_key).toBeUndefined();
    expect(body.data.base_url).toBe("https://vllm.corp/v1");
    expect(body.data.provider).toBe("openai-compatible");

    await waitFor(() =>
      expect(createModelEntryActionMock).toHaveBeenCalledWith({ model_id: "vllm-model", secret_ref: "vllm-gateway" }),
    );
    await waitFor(() =>
      expect(setProviderSelfManagedActionMock).toHaveBeenCalledWith({ secret_ref: "vllm-gateway", model: "vllm-model" }),
    );
    await waitFor(() => expect(onCreated).toHaveBeenCalled());
  });

  it("disables Save while the custom shape is incomplete — key stays optional", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");
    const save = () => within(dialog).getByRole("button", { name: /^save$/i });

    await pickCustomProvider(user, dialog);
    expect(save().hasAttribute("disabled")).toBe(true);

    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    // Name + provider filled; base URL + model still empty.
    expect(save().hasAttribute("disabled")).toBe(true);

    await user.type(within(dialog).getByLabelText(/base url/i), "https://vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");
    // No API key — still submittable for a custom endpoint.
    expect(save().hasAttribute("disabled")).toBe(false);
    expect(createSecretActionMock).not.toHaveBeenCalled();
  });

  it("surfaces a register error for the custom shape without activating", async () => {
    createModelEntryActionMock.mockResolvedValue({ ok: false, error: "duplicate", errorCode: "UZ-MODELS-003" });
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    await pickCustomProvider(user, dialog);
    await user.type(within(dialog).getByLabelText(/base url/i), "https://vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");
    await user.click(within(dialog).getByRole("button", { name: /save & make active/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(setProviderSelfManagedActionMock).not.toHaveBeenCalled();
  });

  it("rejects a non-https base URL inline, without calling the backend", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    await pickCustomProvider(user, dialog);
    await user.type(within(dialog).getByLabelText(/base url/i), "http://vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");

    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert").textContent).toMatch(/use https:\/\/ for the base url/i));
    expect(createSecretActionMock).not.toHaveBeenCalled();
  });

  it("carries a pasted key through in the secret body when one is entered", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    await pickCustomProvider(user, dialog);
    await user.type(within(dialog).getByLabelText(/base url/i), "https://vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");
    await user.type(within(dialog).getByLabelText(/api key \(optional\)/i), "sk-vllm-key");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalled());
    const [, body] = createSecretActionMock.mock.calls[0] as [string, { data: Record<string, unknown> }];
    expect(body.data.api_key).toBe("sk-vllm-key");
  });

  it("errors without writing when the name is owned by a named provider's key (upsert-collision guard)", async () => {
    const anthropicSecret: Secret = { kind: "provider_key", name: "anthropic-prod", provider: "anthropic", created_at: 1 };
    const { user } = await renderDialog([anthropicSecret]);
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic-prod");
    await pickCustomProvider(user, dialog);
    await user.type(within(dialog).getByLabelText(/base url/i), "https://vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    // The secrets POST is a server-side upsert — proceeding would destroy the
    // anthropic credential's body. The guard must error before any write.
    await waitFor(() => expect(within(dialog).getByText(/different provider/i)).toBeTruthy());
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });

  it("reusing an existing custom endpoint's name rewrites that endpoint in place (reconfigure motion)", async () => {
    const endpointSecret: Secret = {
      kind: "custom_endpoint",
      name: "vllm-gateway",
      provider: "openai-compatible",
      base_url: "https://old.vllm.corp/v1",
      created_at: 1,
    };
    const { user } = await renderDialog([endpointSecret]);
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    await pickCustomProvider(user, dialog);
    await user.type(within(dialog).getByLabelText(/base url/i), "https://new.vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalled());
    const [, body] = createSecretActionMock.mock.calls[0] as [string, { data: Record<string, unknown> }];
    expect(body.data.base_url).toBe("https://new.vllm.corp/v1");
  });

  it("surfaces a store error for the custom shape", async () => {
    createSecretActionMock.mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-VAULT-002" });
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^name$/i), "vllm-gateway");
    await pickCustomProvider(user, dialog);
    await user.type(within(dialog).getByLabelText(/base url/i), "https://vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/^model$/i), "vllm-model");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });
});
