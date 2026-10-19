import { describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { updateModelEntryActionMock, replaceSecretActionMock, entry, registry, renderTable } from "./helpers/models-registry-fixtures";
import { clickRowAction, renderEditDialog, EDIT_MODEL } from "./helpers/models-registry-edit-fixtures";

describe("Row actions — Edit", () => {
  it("saves a model change via PATCH; entering a key also replaces the shared secret whole", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    updateModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: EDIT_MODEL, secret_ref: "anthropic-prod", created_at: "1970-01-01T00:00:00.001Z" } });
    replaceSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
    await renderTable(registry([target]));

    const user = userEvent.setup();
    await clickRowAction(user, /edit claude-sonnet-5/i);

    const dialog = await screen.findByRole("dialog");
    await user.clear(within(dialog).getByLabelText(/^model$/i));
    await user.type(within(dialog).getByLabelText(/^model$/i), EDIT_MODEL);
    await user.type(within(dialog).getByLabelText(/api key/i), "sk-ant-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(updateModelEntryActionMock).toHaveBeenCalledWith("e1", { model_id: EDIT_MODEL }));
    await waitFor(() => expect(replaceSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic-prod", { provider: "anthropic", api_key: "sk-ant-rotated" }));
  });

  it("changes only the model when no key is entered — replace is never called", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    updateModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: EDIT_MODEL, secret_ref: "anthropic-prod", created_at: "1970-01-01T00:00:00.001Z" } });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    await user.clear(within(dialog).getByLabelText(/^model$/i));
    await user.type(within(dialog).getByLabelText(/^model$/i), EDIT_MODEL);
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(updateModelEntryActionMock).toHaveBeenCalledWith("e1", { model_id: EDIT_MODEL }));
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("replaces only the secret when the model is unchanged — PATCH is never called", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    replaceSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    await user.type(within(dialog).getByLabelText(/api key/i), "sk-ant-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(replaceSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic-prod", { provider: "anthropic", api_key: "sk-ant-rotated" }));
    expect(updateModelEntryActionMock).not.toHaveBeenCalled();
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("a failed replace strands only the model write, and the table is re-read to show it", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    updateModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: EDIT_MODEL, secret_ref: "anthropic-prod", created_at: "1970-01-01T00:00:00.001Z" } });
    replaceSecretActionMock.mockResolvedValue({ ok: false, error: "rejected", errorCode: "UZ-REQ-001" });
    const { dialog, onSaved, onCommitted, user } = await renderEditDialog(target);

    await user.clear(within(dialog).getByLabelText(/^model$/i));
    await user.type(within(dialog).getByLabelText(/^model$/i), EDIT_MODEL);
    await user.type(within(dialog).getByLabelText(/api key/i), "sk-ant-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    // The entry writes FIRST, so the half left behind is the recoverable one:
    // one row, visible, changed back in two clicks. The credential — shared,
    // unreadable, unrestorable — was never rotated.
    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(updateModelEntryActionMock).toHaveBeenCalledWith("e1", { model_id: EDIT_MODEL });
    // The stranded write is SHOWN, not narrated: the table re-reads so it
    // displays the model the server actually holds.
    await waitFor(() => expect(onCommitted).toHaveBeenCalled());
    // Still a failure — the dialog stays open so the credential can be retried.
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("surfaces a replace error", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    replaceSecretActionMock.mockResolvedValue({ ok: false, error: "rejected", errorCode: "UZ-REQ-001" });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    await user.type(within(dialog).getByLabelText(/api key/i), "sk-ant-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("a failed model write never reaches the credential — nothing is rotated", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    updateModelEntryActionMock.mockResolvedValue({ ok: false, error: "conflict", errorCode: "UZ-MODELS-003" });
    const { dialog, onSaved, onCommitted, user } = await renderEditDialog(target);

    await user.clear(within(dialog).getByLabelText(/^model$/i));
    await user.type(within(dialog).getByLabelText(/^model$/i), EDIT_MODEL);
    await user.type(within(dialog).getByLabelText(/api key/i), "sk-ant-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    // This is what the ordering buys. The permanent write is last, so a failure
    // ahead of it means the shared credential was never touched — nothing to
    // strand, nothing to re-read, and every entry referencing the secret keeps
    // authenticating.
    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
    expect(onCommitted).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("a validation failure runs before EITHER write", async () => {
    // Hoisting matters once the entry writes first: a check firing between the
    // two would strand the rename for a reason the caller could have been given
    // before anything was written.
    const target = entry({ id: "e1", model_id: "claude-sonnet-5", kind: "custom_endpoint", provider: undefined, base_url: "https://api.example.com", has_key: true });
    const { dialog, onSaved, onCommitted, user } = await renderEditDialog(target);

    const baseUrl = within(dialog).getByLabelText(/base url/i);
    await user.clear(baseUrl);
    await user.type(baseUrl, "http://insecure.example.com");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(updateModelEntryActionMock).not.toHaveBeenCalled();
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
    expect(onCommitted).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("an opaque secret hides the key field and points at the CLI; model edits still work", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5", provider: undefined, kind: "custom_secret" });
    updateModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-opus-4-8", secret_ref: "anthropic-prod", created_at: "1970-01-01T00:00:00.001Z" } });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    // The dashboard cannot recompose an opaque body, so there is no key input
    // — replacement is the CLI's job and the hint names the command.
    expect(within(dialog).queryByLabelText(/api key/i)).toBeNull();
    expect(within(dialog).getByText(/agentsfleet secret update/i)).toBeTruthy();

    // No catalogue and no known provider → the model field is a free-text input.
    const model = within(dialog).getByLabelText(/^model$/i) as HTMLInputElement;
    await user.clear(model);
    await user.type(model, "claude-opus-4-8");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });

  it("a custom endpoint edit replaces base URL and key together in one body", async () => {
    const target = entry({
      id: "e1", model_id: "vllm-model", secret_ref: "vllm-gateway",
      provider: "openai-compatible", kind: "custom_endpoint", base_url: "https://old.vllm.corp/v1",
    });
    replaceSecretActionMock.mockResolvedValue({ ok: true, data: { name: "vllm-gateway" } });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    const baseUrl = within(dialog).getByLabelText(/base url/i) as HTMLInputElement;
    await user.clear(baseUrl);
    await user.type(baseUrl, "https://new.vllm.corp/v1");
    await user.type(within(dialog).getByLabelText(/api key/i), "sk-custom-new");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(replaceSecretActionMock).toHaveBeenCalledWith("ws_1", "vllm-gateway", {
      provider: "openai-compatible",
      base_url: "https://new.vllm.corp/v1",
      api_key: "sk-custom-new",
    }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("a non-https base URL is refused client-side — no request is sent", async () => {
    const target = entry({
      id: "e1", model_id: "vllm-model", secret_ref: "vllm-gateway",
      provider: "openai-compatible", kind: "custom_endpoint", base_url: "https://old.vllm.corp/v1",
    });
    const { dialog, user } = await renderEditDialog(target);

    const baseUrl = within(dialog).getByLabelText(/base url/i) as HTMLInputElement;
    await user.clear(baseUrl);
    await user.type(baseUrl, "http://insecure.corp/v1");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });

  it("composes an empty provider fallback when a provider-key entry carries none", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5", provider: undefined });
    replaceSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    // No catalogue and no known provider → the model field is a free-text input.
    await user.type(within(dialog).getByLabelText(/api key/i), "sk-rotated");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(replaceSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic-prod", {
      provider: "",
      api_key: "sk-rotated",
    }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("a cleared model field disables Save — an empty model can never submit", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5", provider: undefined });
    const { dialog, user } = await renderEditDialog(target);

    // Free-text model input (no catalogue, unknown provider): clearing it
    // renders the empty-model arm and pins the disabled Save.
    const model = within(dialog).getByLabelText(/^model$/i) as HTMLInputElement;
    await user.clear(model);

    await waitFor(() => expect((within(dialog).getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(true));
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });

  it("a keyed custom endpoint refuses a base-URL-only change that would drop the key", async () => {
    // Whole-body replace can't preserve a key it can never read back. Changing
    // only the base URL with the key blank must be refused, not silently PUT a
    // keyless body that deletes the stored credential (adversarial-review F3).
    const target = entry({
      id: "e1", model_id: "vllm-model", secret_ref: "vllm-gateway",
      provider: "openai-compatible", kind: "custom_endpoint",
      base_url: "https://old.vllm.corp/v1", has_key: true,
    });
    const { dialog, user } = await renderEditDialog(target);

    const baseUrl = within(dialog).getByLabelText(/base url/i) as HTMLInputElement;
    await user.clear(baseUrl);
    await user.type(baseUrl, "https://new.vllm.corp/v1");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });

  it("a keyless custom endpoint allows a base-URL-only change", async () => {
    // has_key false → there is no key to drop, so the base URL may change alone.
    const target = entry({
      id: "e1", model_id: "vllm-model", secret_ref: "vllm-gateway",
      provider: "openai-compatible", kind: "custom_endpoint",
      base_url: "https://old.vllm.corp/v1", has_key: false,
    });
    replaceSecretActionMock.mockResolvedValue({ ok: true, data: { name: "vllm-gateway" } });
    const { dialog, onSaved, user } = await renderEditDialog(target);

    const baseUrl = within(dialog).getByLabelText(/base url/i) as HTMLInputElement;
    await user.clear(baseUrl);
    await user.type(baseUrl, "https://new.vllm.corp/v1");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(replaceSecretActionMock).toHaveBeenCalledWith("ws_1", "vllm-gateway", {
      provider: "openai-compatible",
      base_url: "https://new.vllm.corp/v1",
    }));
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("a stored-key entry prompts to re-enter the key; a keyless one does not", async () => {
    // The key placeholder has three arms: has_key (re-enter), keyless custom
    // (blank ok), keyless named (enter). Cover the named-keyless arm here; the
    // other two are exercised by the refuse/allow tests above.
    const target = entry({ id: "e1", model_id: "claude-sonnet-5", has_key: false });
    const { dialog } = await renderEditDialog(target);
    const key = within(dialog).getByLabelText(/api key/i) as HTMLInputElement;
    expect(key.placeholder).toMatch(/enter the key/i);
  });

  it("a custom endpoint with no stored base URL renders the field empty", async () => {
    // `base_url` is optional on the entry row; the ?? fallback keeps the
    // dirty-check well-defined instead of comparing against undefined.
    const target = entry({
      id: "e1", model_id: "vllm-model", secret_ref: "vllm-gateway",
      provider: "openai-compatible", kind: "custom_endpoint", base_url: undefined,
    });
    const { dialog } = await renderEditDialog(target);
    expect((within(dialog).getByLabelText(/base url/i) as HTMLInputElement).value).toBe("");
  });

  it("Cancel closes the dialog without saving", async () => {
    const target = entry({ id: "e1", model_id: "claude-sonnet-5" });
    const { dialog, onOpenChange, user } = await renderEditDialog(target);

    await user.click(within(dialog).getByRole("button", { name: /^cancel$/i }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(updateModelEntryActionMock).not.toHaveBeenCalled();
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });
});
