import { describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listModelEntriesActionMock, listSecretsActionMock, createModelEntryActionMock, replaceSecretActionMock, createSecretActionMock, entry, registry, renderTable } from "./helpers/models-registry-fixtures";

const MODEL_ID = "claude-sonnet-5-5";
const SAVED = {kind: "provider_key", name: "anthropic", provider: "anthropic", created_at: "2026-04-30T00:00:00Z"};
type User = ReturnType<typeof userEvent.setup>;

async function open(user: User) {
  await user.click(screen.getByRole("button", {name: "Create model"}));
  const dialog = within(await screen.findByRole("dialog"));
  await dialog.findByLabelText("Key name");
  return dialog;
}

async function fill(user: User, dialog: ReturnType<typeof within>) {
  await user.type(dialog.getByLabelText("Provider"), "anthropic");
  await user.type(dialog.getByLabelText("Model"), MODEL_ID);
  await user.type(dialog.getByLabelText("Key name"), SAVED.name);
  await user.type(dialog.getByLabelText("API key"), crypto.randomUUID());
}

function registration() {
  createSecretActionMock.mockResolvedValue({ok: true, data: {name: SAVED.name}});
  createModelEntryActionMock.mockResolvedValue({ok: true, data: entry({id: "e1", secret_ref: SAVED.name})});
  listModelEntriesActionMock.mockResolvedValue({ok: true, data: registry([entry({id: "e1", secret_ref: SAVED.name})])});
}

describe("Models registry stored secrets", () => {
  it("refreshes after storage and requires explicit reuse when the name already exists", async () => {
    registration();
    listSecretsActionMock.mockResolvedValueOnce({ok: true, data: {secrets: []}})
      .mockResolvedValue({ok: true, data: {secrets: [SAVED]}});
    await renderTable(registry([]));
    const user = userEvent.setup();
    const dialog = await open(user);
    await fill(user, dialog);
    await user.click(dialog.getByRole("button", {name: "Create model"}));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(listSecretsActionMock).toHaveBeenCalledWith("ws_1");

    const reopened = await open(user);
    await fill(user, reopened);
    expect(reopened.getByText(/That name is already in use/)).toBeTruthy();
    expect(reopened.getByRole("button", {name: "Create model"})).toHaveProperty("disabled", true);
    await user.click(reopened.getByRole("button", {name: "Use saved key"}));
    await user.click(reopened.getByRole("combobox", {name: "Saved key"}));
    await user.click(screen.getByRole("option", {name: SAVED.name}));
    expect(reopened.queryByLabelText("API key")).toBeNull();
    await user.click(reopened.getByRole("button", {name: "Create model"}));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createSecretActionMock).toHaveBeenCalledTimes(1);
    expect(createModelEntryActionMock).toHaveBeenCalledTimes(2);
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });

  it("retains saved-key metadata after a transient refresh failure without writing its value", async () => {
    registration();
    listSecretsActionMock.mockResolvedValueOnce({ok: true, data: {secrets: [SAVED]}})
      .mockResolvedValue({ok: false, error: "Temporarily unavailable", status: 503});
    await renderTable(registry([]));
    const user = userEvent.setup();
    const first = await open(user);
    await user.click(first.getByRole("button", {name: "Cancel"}));
    const reopened = await open(user);
    await user.type(reopened.getByLabelText("Provider"), SAVED.provider);
    await user.type(reopened.getByLabelText("Model"), MODEL_ID);
    await user.click(reopened.getByRole("button", {name: "Use saved key"}));
    await user.click(reopened.getByRole("combobox", {name: "Saved key"}));
    await user.click(screen.getByRole("option", {name: SAVED.name}));
    await user.click(reopened.getByRole("button", {name: "Create model"}));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createModelEntryActionMock).toHaveBeenCalledWith({workspace_id: "ws_1", model_id: MODEL_ID, secret_ref: SAVED.name});
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });

  it.each(["refused", "rejected"])("blocks a first %s secret read and recovers through Try again", async failure => {
    registration();
    if (failure === "rejected") listSecretsActionMock.mockRejectedValueOnce(new Error("Connection lost"));
    else listSecretsActionMock.mockResolvedValueOnce({ok: false, error: "Temporarily unavailable", status: 503});
    listSecretsActionMock.mockResolvedValue({ok: true, data: {secrets: []}});
    await renderTable(registry([]));
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", {name: "Create model"}));
    const dialog = within(await screen.findByRole("dialog"));
    expect((await dialog.findByRole("alert")).textContent).toContain("Couldn’t load saved keys.");
    expect(dialog.queryByLabelText("API key")).toBeNull();
    expect(dialog.queryByRole("button", {name: "Create model"})).toBeNull();
    expect(createSecretActionMock).not.toHaveBeenCalled();
    await user.click(dialog.getByRole("button", {name: "Try again"}));
    await dialog.findByLabelText("Key name");
    await fill(user, dialog);
    await user.click(dialog.getByRole("button", {name: "Create model"}));
    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalledTimes(1));
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });
});
