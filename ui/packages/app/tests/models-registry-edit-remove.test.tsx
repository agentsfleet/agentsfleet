import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { TenantModelEntry, TenantModelEntryList } from "@/lib/types";

const listModelEntriesActionMock = vi.fn();
const listSecretsActionMock = vi.fn();
const setProviderSelfManagedActionMock = vi.fn();
const resetProviderActionMock = vi.fn();
const createModelEntryActionMock = vi.fn();
const updateModelEntryActionMock = vi.fn();
const deleteModelEntryActionMock = vi.fn();
const rotateSecretActionMock = vi.fn();

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  listModelEntriesAction: listModelEntriesActionMock,
  listSecretsAction: listSecretsActionMock,
  setProviderSelfManagedAction: setProviderSelfManagedActionMock,
  resetProviderAction: resetProviderActionMock,
  createModelEntryAction: createModelEntryActionMock,
  updateModelEntryAction: updateModelEntryActionMock,
  deleteModelEntryAction: deleteModelEntryActionMock,
  rotateSecretAction: rotateSecretActionMock,
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({
  createSecretAction: vi.fn(),
  deleteSecretAction: vi.fn(),
}));

function entry(overrides: Partial<TenantModelEntry>): TenantModelEntry {
  return {
    id: "0190aaaa-aaaa-7aaa-aaaa-aaaaaaaaaaaa",
    model_id: "claude-sonnet-5",
    secret_ref: "anthropic-prod",
    provider: "anthropic",
    kind: "provider_key",
    has_key: true,
    active: false,
    created_at: 1_777_507_200_000,
    ...overrides,
  };
}

function registry(models: TenantModelEntry[]): TenantModelEntryList {
  return { models, platform_default_available: true };
}

async function renderTable(initial: TenantModelEntryList) {
  const { default: ModelsRegistryTable } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
  );
  render(React.createElement(ModelsRegistryTable, { workspaceId: "ws_1", initial, initialSecrets: [] } as never));
}

async function openRowMenu(user: ReturnType<typeof userEvent.setup>, modelId: string) {
  await user.click(screen.getByRole("button", { name: new RegExp(`row actions for ${modelId}`, "i") }));
}

async function renderEditDialog(target: TenantModelEntry) {
  const { default: EditModelEntryDialog } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/EditModelEntryDialog"
  );
  const onOpenChange = vi.fn();
  const onSaved = vi.fn();
  const onPartialSuccess = vi.fn();
  render(
    React.createElement(EditModelEntryDialog, {
      workspaceId: "ws_1",
      target,
      onOpenChange,
      onSaved,
      onPartialSuccess,
    } as never),
  );
  const dialog = await screen.findByRole("dialog");
  return { dialog, onOpenChange, onSaved, onPartialSuccess, user: userEvent.setup() };
}

beforeEach(() => {
  vi.clearAllMocks();
  // Every successful Edit/Remove triggers ModelsRegistryTable's refresh();
  // give it a harmless default so tests that don't care about the re-fetch
  // don't hit `.ok` on an unmocked (undefined) resolution.
  listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([]) });
  listSecretsActionMock.mockResolvedValue({ ok: true, data: { secrets: [] } });
});
afterEach(() => cleanup());

describe("Row actions — Edit", () => {
  it("saves a model change via PATCH; entering a key also rotates the shared secret", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    updateModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-opus-4-8", secret_ref: "anthropic-prod", created_at: 1 } });
    rotateSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
    await renderTable(registry([target]));

    const user = userEvent.setup();
    await openRowMenu(user, "claude-sonnet-5");
    await user.click(screen.getByRole("menuitem", { name: /^edit$/i }));

    const dialog = await screen.findByRole("dialog");
    // The model catalogue is empty in this test (no ModelCatalogueProvider),
    // so ProviderModelSelect falls back to the static known-models list for
    // "anthropic" — a <Select>, not a free-text input.
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click(await screen.findByRole("option", { name: "claude-opus-4-8" }));
    await user.type(within(dialog).getByLabelText(/new api key/i), "sk-ant-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(updateModelEntryActionMock).toHaveBeenCalledWith("e1", { model_id: "claude-opus-4-8" }));
    await waitFor(() => expect(rotateSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic-prod", "sk-ant-rotated"));
  });

  it("changes only the model when no key is entered — rotate is never called", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    updateModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-opus-4-8", secret_ref: "anthropic-prod", created_at: 1 } });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click(await screen.findByRole("option", { name: "claude-opus-4-8" }));
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(updateModelEntryActionMock).toHaveBeenCalledWith("e1", { model_id: "claude-opus-4-8" }));
    expect(rotateSecretActionMock).not.toHaveBeenCalled();
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("rotates only the key when the model is unchanged — PATCH is never called", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    rotateSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    await user.type(within(dialog).getByLabelText(/new api key/i), "sk-ant-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(rotateSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic-prod", "sk-ant-rotated"));
    expect(updateModelEntryActionMock).not.toHaveBeenCalled();
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("surfaces a PATCH error and never rotates", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    updateModelEntryActionMock.mockResolvedValue({ ok: false, error: "conflict", errorCode: "UZ-MODELS-003" });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click(await screen.findByRole("option", { name: "claude-opus-4-8" }));
    await user.type(within(dialog).getByLabelText(/new api key/i), "sk-ant-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(rotateSecretActionMock).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("surfaces a rotate error", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    rotateSecretActionMock.mockResolvedValue({ ok: false, error: "rejected", errorCode: "UZ-REQ-001" });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    await user.type(within(dialog).getByLabelText(/new api key/i), "sk-ant-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("keeps the table in sync when the model rename commits but the key rotation fails", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    updateModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-opus-4-8", secret_ref: "anthropic-prod", created_at: 1 } });
    rotateSecretActionMock.mockResolvedValue({ ok: false, error: "rejected", errorCode: "UZ-REQ-001" });
    const { dialog, onSaved, onPartialSuccess, user } = await renderEditDialog(target);

    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click(await screen.findByRole("option", { name: "claude-opus-4-8" }));
    await user.type(within(dialog).getByLabelText(/new api key/i), "sk-ant-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    // The rename already committed server-side — the table must refresh even
    // though the dialog stays open (rotate failed) and onSaved never fires.
    await waitFor(() => expect(onPartialSuccess).toHaveBeenCalled());
    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("tracks with an empty provider fallback when the entry has none", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5", provider: undefined, kind: "custom_secret" });
    updateModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-opus-4-8", secret_ref: "anthropic-prod", created_at: 1 } });
    rotateSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    // No catalogue and no known provider → the model field is a free-text input.
    const model = within(dialog).getByLabelText(/^model$/i) as HTMLInputElement;
    await user.clear(model);
    await user.type(model, "claude-opus-4-8");
    await user.type(within(dialog).getByLabelText(/new api key/i), "sk-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("Cancel closes the dialog without saving", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    const { dialog, onOpenChange, user } = await renderEditDialog(target);

    await user.click(within(dialog).getByRole("button", { name: /^cancel$/i }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(updateModelEntryActionMock).not.toHaveBeenCalled();
    expect(rotateSecretActionMock).not.toHaveBeenCalled();
  });
});

describe("Row actions — dialog dismissal wiring (via the full table)", () => {
  it("closes the View details dialog on its own Close button", async () => {
    await renderTable(registry([entry({})]));
    const user = userEvent.setup();
    await openRowMenu(user, "claude-sonnet-5");
    await user.click(screen.getByRole("menuitem", { name: /view details/i }));
    const dialog = await screen.findByRole("dialog");

    await user.click(within(dialog).getByRole("button", { name: /^close$/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("closes the Edit dialog via Cancel, wired through the table's own state", async () => {
    await renderTable(registry([entry({})]));
    const user = userEvent.setup();
    await openRowMenu(user, "claude-sonnet-5");
    await user.click(screen.getByRole("menuitem", { name: /^edit$/i }));
    const dialog = await screen.findByRole("dialog");

    await user.click(within(dialog).getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(updateModelEntryActionMock).not.toHaveBeenCalled();
  });

  it("dismissing the Remove confirm without confirming clears the pending target", async () => {
    await renderTable(registry([entry({})]));
    const user = userEvent.setup();
    await openRowMenu(user, "claude-sonnet-5");
    await user.click(screen.getByRole("menuitem", { name: /^remove claude-sonnet-5$/i }));
    await screen.findByRole("alertdialog");

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deleteModelEntryActionMock).not.toHaveBeenCalled();
  });
});

describe("Row actions — View details", () => {
  it("renders nothing when no row is selected", async () => {
    const { default: ModelDetailsDialog } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog"
    );
    const { container } = render(
      React.createElement(ModelDetailsDialog, { target: null, onOpenChange: vi.fn() } as never),
    );
    expect(container.firstChild).toBeNull();
  });

  it("shows provider, endpoint, key name, and has-key for a full custom-endpoint entry", async () => {
    const { default: ModelDetailsDialog } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog"
    );
    const target = entry({
      model_id: "vllm-model",
      secret_ref: "vllm-gateway",
      provider: "openai-compatible",
      kind: "custom_endpoint",
      base_url: "https://vllm.corp/v1",
      has_key: true,
    });
    render(React.createElement(ModelDetailsDialog, { target, onOpenChange: vi.fn() } as never));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("https://vllm.corp/v1")).toBeTruthy();
    expect(within(dialog).getByText("vllm-gateway")).toBeTruthy();
    expect(within(dialog).getByText("Yes")).toBeTruthy();
  });

  it("shows Unknown provider and the keyless note for a minimal entry", async () => {
    const { default: ModelDetailsDialog } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog"
    );
    const target = entry({ provider: undefined, kind: "custom_secret", base_url: undefined, has_key: false });
    render(React.createElement(ModelDetailsDialog, { target, onOpenChange: vi.fn() } as never));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Unknown")).toBeTruthy();
    expect(within(dialog).getByText(/no — keyless endpoint/i)).toBeTruthy();
    expect(within(dialog).queryByText(/endpoint/i, { selector: "dt" })).toBeNull();
  });
});

describe("Row actions — Remove", () => {
  it("deletes a non-active entry only; the shared secret and sibling entry survive", async () => {
    const active = entry({ id: "e1", model_id: "claude-sonnet-5", active: true });
    const inactive = entry({ id: "e2", model_id: "claude-opus-4-8", active: false });
    deleteModelEntryActionMock.mockResolvedValue({ ok: true, data: undefined });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([active]) });
    await renderTable(registry([active, inactive]));

    const user = userEvent.setup();
    await openRowMenu(user, "claude-opus-4-8");
    await user.click(screen.getByRole("menuitem", { name: /^remove claude-opus-4-8$/i }));

    const confirm = await screen.findByRole("alertdialog");
    await user.click(within(confirm).getByRole("button", { name: /^remove$/i }));

    await waitFor(() => expect(deleteModelEntryActionMock).toHaveBeenCalledWith("e2"));
    await waitFor(() => expect(listModelEntriesActionMock).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText("claude-sonnet-5")).toBeTruthy());
  });

  it("surfaces a delete error inside the confirm dialog and still refreshes behind it", async () => {
    const inactive = entry({ id: "e2", model_id: "claude-opus-4-8", active: false });
    deleteModelEntryActionMock.mockResolvedValue({ ok: false, error: "conflict", errorCode: "UZ-MODELS-001" });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([inactive]) });
    await renderTable(registry([inactive]));

    const user = userEvent.setup();
    await openRowMenu(user, "claude-opus-4-8");
    await user.click(screen.getByRole("menuitem", { name: /^remove claude-opus-4-8$/i }));
    const confirm = await screen.findByRole("alertdialog");
    await user.click(within(confirm).getByRole("button", { name: /^remove$/i }));

    await waitFor(() => expect(within(confirm).getByText(/conflict/i)).toBeTruthy());
    await waitFor(() => expect(listModelEntriesActionMock).toHaveBeenCalled());
  });

  it("disables Remove with a reason on the active entry's row menu", async () => {
    const active = entry({ id: "e1", model_id: "claude-sonnet-5", active: true });
    await renderTable(registry([active]));

    const user = userEvent.setup();
    await openRowMenu(user, "claude-sonnet-5");
    const removeItem = screen.getByRole("menuitem", { name: /cannot remove claude-sonnet-5 while it is active/i });
    expect(removeItem.getAttribute("aria-disabled")).toBe("true");

    await user.click(removeItem);
    expect(deleteModelEntryActionMock).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});
