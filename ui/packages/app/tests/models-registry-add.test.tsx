import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Secret } from "@/lib/api/secrets";

const createModelEntryActionMock = vi.fn();
const setProviderSelfManagedActionMock = vi.fn();
const rotateSecretActionMock = vi.fn();
const createSecretActionMock = vi.fn();

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  createModelEntryAction: createModelEntryActionMock,
  setProviderSelfManagedAction: setProviderSelfManagedActionMock,
  rotateSecretAction: rotateSecretActionMock,
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({
  createSecretAction: createSecretActionMock,
}));

// Model catalogue state the Provider/Model pickers read — empty by default
// (free-text provider fallback); tests populate it to exercise the
// catalogue-backed <Select> branch.
const { catalogueState } = vi.hoisted(() => ({
  catalogueState: {
    models: [] as Array<{
      id: string;
      provider: string;
      context_cap_tokens: number;
      input_nanos_per_mtok: number;
      cached_input_nanos_per_mtok: number;
      output_nanos_per_mtok: number;
    }>,
    loading: false,
    error: false,
  },
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider", () => ({
  useModelCatalogue: () => catalogueState,
}));

const ANTHROPIC_SECRET: Secret = {
  kind: "provider_key",
  name: "anthropic-prod",
  provider: "anthropic",
  created_at: 1_777_507_200_000,
};
const ROTATED_API_KEY = "sk-ant-rotated-key";

async function renderDialog(secrets: Secret[] = []) {
  const { default: AddModelEntryDialog } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog"
  );
  const onCreated = vi.fn();
  const onSecretsChanged = vi.fn();
  render(
    React.createElement(AddModelEntryDialog, { workspaceId: "ws_1", secrets, onCreated, onSecretsChanged } as never),
  );
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: /create model/i }));
  await screen.findByRole("dialog");
  return { onCreated, onSecretsChanged, user };
}

/** Walks the unified form in its field order: Name → Provider → Model → API key.
 * Empty catalogue → provider is the free-text fallback; the model picker
 * fills from the static known-models list for the typed provider. */
async function fillKnownForm(
  user: ReturnType<typeof userEvent.setup>,
  dialog: HTMLElement,
  { name, provider = "anthropic", key }: { name: string; provider?: string; key: string },
) {
  await user.type(within(dialog).getByLabelText(/^name$/i), name);
  await user.type(within(dialog).getByLabelText(/^provider$/i), provider);
  await user.click(within(dialog).getByLabelText(/^model$/i));
  await user.click((await screen.findAllByRole("option"))[0]!);
  await user.type(within(dialog).getByLabelText(/^api key$/i), key);
}

beforeEach(() => {
  vi.clearAllMocks();
  catalogueState.models = [];
  createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
  rotateSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
  createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: 1 } });
  setProviderSelfManagedActionMock.mockResolvedValue({
    ok: true,
    data: { mode: "self_managed", provider: "anthropic", model: "claude-sonnet-5", context_cap_tokens: 200000, secret_ref: "anthropic", platform_default_available: true },
  });
});
afterEach(() => cleanup());

describe("AddModelEntryDialog — unified form shape", () => {
  it("renders one tab-free form ordered Name → Provider → Model → API key, with no Base URL for a named provider", async () => {
    await renderDialog();
    const dialog = screen.getByRole("dialog");

    expect(within(dialog).queryByRole("tablist")).toBeNull();
    expect(within(dialog).queryByLabelText(/base url/i)).toBeNull();

    const labels = Array.from(dialog.querySelectorAll("label")).map((l) => l.textContent);
    expect(labels).toEqual(["Name", "Provider", "Model", "API key"]);
  });

  it("typing an API key or a provider never mutates the free-form Name", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    const name = within(dialog).getByLabelText(/^name$/i) as HTMLInputElement;
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    expect(name.value).toBe("");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    expect(name.value).toBe("");

    await user.type(name, "my-second-anthropic");
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() =>
      expect(createSecretActionMock).toHaveBeenCalledWith("ws_1", expect.objectContaining({ name: "my-second-anthropic" })),
    );
    expect(rotateSecretActionMock).not.toHaveBeenCalled();
  });

  it("closes from Cancel without creating a secret or model entry", async () => {
    const { user } = await renderDialog();
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
    expect(setProviderSelfManagedActionMock).not.toHaveBeenCalled();
  });

  it("lists the library's providers plus the OpenAI-compatible option pinned last", async () => {
    catalogueState.models = [
      { id: "claude-sonnet-5", provider: "anthropic", context_cap_tokens: 200000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
    ];
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByLabelText(/^provider$/i));
    const options = await screen.findAllByRole("option");
    const labels = options.map((o) => o.textContent);
    expect(labels[0]).toBe("Anthropic");
    expect(labels[labels.length - 1]).toBe("Custom — OpenAI-compatible");
  });
});

describe("AddModelEntryDialog — create-or-rotate by name", () => {
  it("rotates the stored key in place when the name already exists with the same provider", async () => {
    const { onCreated, onSecretsChanged, user } = await renderDialog([ANTHROPIC_SECRET]);
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "anthropic-prod", key: ROTATED_API_KEY });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(rotateSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic-prod", ROTATED_API_KEY));
    expect(createSecretActionMock).not.toHaveBeenCalled();
    await waitFor(() => expect(createModelEntryActionMock).toHaveBeenCalledWith(
      expect.objectContaining({ secret_ref: "anthropic-prod" }),
    ));
    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    // A rotate leaves the secret's list-visible metadata identical, so the
    // secrets refetch is skipped — only the entries list refreshes.
    expect(onSecretsChanged).not.toHaveBeenCalled();
  });

  it("errors without writing anything when the name is owned by a different provider's key", async () => {
    const openaiSecret: Secret = { kind: "provider_key", name: "openai-prod", provider: "openai", created_at: 1_777_507_200_000 };
    const { user } = await renderDialog([openaiSecret]);
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "openai-prod", key: "sk-ant-e2e-xxxx" });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByText(/different provider/i)).toBeTruthy());
    expect(rotateSecretActionMock).not.toHaveBeenCalled();
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });

  it("errors without writing when the name is owned by a custom endpoint secret (kind-blind guard closed)", async () => {
    const endpointSecret: Secret = {
      kind: "custom_endpoint",
      name: "vllm-gateway",
      provider: "openai-compatible",
      base_url: "https://vllm.corp/v1",
      created_at: 1_777_507_200_000,
    };
    const { user } = await renderDialog([endpointSecret]);
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "vllm-gateway", key: "sk-ant-e2e-xxxx" });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    // The secrets POST is an upsert — writing {provider, api_key} over an
    // endpoint secret would destroy its base_url and break the entries
    // referencing it. The guard must catch non-provider_key kinds too.
    await waitFor(() => expect(within(dialog).getByText(/different provider/i)).toBeTruthy());
    expect(rotateSecretActionMock).not.toHaveBeenCalled();
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });

  it("surfaces a register error after a successful rotate, and leaves the dialog open", async () => {
    createModelEntryActionMock.mockResolvedValue({ ok: false, error: "duplicate", errorCode: "UZ-MODELS-003" });
    const { user } = await renderDialog([ANTHROPIC_SECRET]);
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "anthropic-prod", key: ROTATED_API_KEY });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(rotateSecretActionMock).toHaveBeenCalled());
    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("surfaces a rotate error and never registers an entry", async () => {
    rotateSecretActionMock.mockResolvedValue({ ok: false, error: "rejected", errorCode: "UZ-REQ-001" });
    const { user } = await renderDialog([ANTHROPIC_SECRET]);
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "anthropic-prod", key: ROTATED_API_KEY });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });
});

describe("AddModelEntryDialog — known provider, new key", () => {
  it("disables Save until name, provider, model, AND key are present for a named provider", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");
    const save = () => within(dialog).getByRole("button", { name: /^save$/i });

    expect(save().hasAttribute("disabled")).toBe(true);
    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    // Everything but the key — a named provider is never keyless.
    expect(save().hasAttribute("disabled")).toBe(true);

    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    expect(save().hasAttribute("disabled")).toBe(false);
    expect(createSecretActionMock).not.toHaveBeenCalled();
  });

  it("stores a secret with provider + api_key and no model field in the body", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "anthropic", key: "sk-ant-e2e-xxxx" });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalled());
    const [, body] = createSecretActionMock.mock.calls[0] as [string, { name: string; data: Record<string, unknown> }];
    expect(body.data.provider).toBe("anthropic");
    expect(body.data.api_key).toBe("sk-ant-e2e-xxxx");
    expect("model" in body.data).toBe(false);
    await waitFor(() => expect(createModelEntryActionMock).toHaveBeenCalled());
  });

  it("picks the provider from the catalogue-backed <Select> when the library has rows", async () => {
    catalogueState.models = [
      { id: "claude-sonnet-5", provider: "anthropic", context_cap_tokens: 200000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
    ];
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic");
    await user.click(within(dialog).getByLabelText(/^provider$/i));
    await user.click(await screen.findByRole("option", { name: /^anthropic$/i }));
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.type(within(dialog).getByLabelText(/^api key$/i), "a-key-with-no-known-prefix");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalled());
    const [, body] = createSecretActionMock.mock.calls[0] as [string, { data: Record<string, unknown> }];
    expect(body.data.provider).toBe("anthropic");
  });

  it("surfaces a store error and never registers an entry", async () => {
    createSecretActionMock.mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-VAULT-002" });
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "anthropic", key: "sk-ant-e2e-xxxx" });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });

  it("surfaces a register error without activating, when the entry create fails", async () => {
    createModelEntryActionMock.mockResolvedValue({ ok: false, error: "duplicate", errorCode: "UZ-MODELS-003" });
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "anthropic", key: "sk-ant-e2e-xxxx" });
    await user.click(within(dialog).getByRole("button", { name: /save & make active/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(setProviderSelfManagedActionMock).not.toHaveBeenCalled();
  });

  it("surfaces an activation error after a successful register, but still refreshes so a retry doesn't 409", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({ ok: false, error: "rejected", errorCode: "UZ-PROVIDER-003" });
    const { onCreated, onSecretsChanged, user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "anthropic", key: "sk-ant-e2e-xxxx" });
    await user.click(within(dialog).getByRole("button", { name: /save & make active/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    // The entry itself is already committed server-side by this point — the
    // table must reflect it even though activation failed and the dialog
    // stays open (matches ModelsRegistryTable.onSwitchEntry's "refresh
    // regardless of outcome" convention).
    expect(onCreated).toHaveBeenCalled();
    expect(onSecretsChanged).toHaveBeenCalled();
    // Dialog stays open — the user can see the error, not silently closed.
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});
