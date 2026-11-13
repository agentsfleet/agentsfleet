import { describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import { createModelEntryActionMock, setProviderSelfManagedActionMock, createSecretActionMock, renderDialog, fillKnownForm } from "./helpers/models-registry-add-fixtures";

const CREDENTIAL = {name: "anthropic", key: "sample-provider-key"};

describe("Create model with a new provider key", () => {
  it("requires provider, model, key name and key before creating", async () => {
    const {user} = await renderDialog();
    const dialog = screen.getByRole("dialog");
    const create = within(dialog).getByRole("button", {name: "Create model"});
    expect(create.hasAttribute("disabled")).toBe(true);
    await fillKnownForm(user, dialog, CREDENTIAL);
    expect(create.hasAttribute("disabled")).toBe(false);
    await user.click(create);
    await waitFor(() => expect(createModelEntryActionMock).toHaveBeenCalledTimes(1));
    expect(createSecretActionMock).toHaveBeenCalledWith("ws_1", {name: "anthropic", data: {provider: "anthropic", api_key: CREDENTIAL.key}});
    expect(setProviderSelfManagedActionMock).not.toHaveBeenCalled();
  });

  it("retains a stored key and retries registration alone after a refusal", async () => {
    createModelEntryActionMock.mockResolvedValueOnce({ok: false, error: "Registration refused", status: 422});
    const {user, onSecretsChanged} = await renderDialog();
    const dialog = screen.getByRole("dialog");
    await fillKnownForm(user, dialog, CREDENTIAL);
    await user.click(within(dialog).getByRole("button", {name: "Create model"}));
    const retry = await within(dialog).findByRole("button", {name: "Retry registration"});
    expect(within(dialog).getByRole("alert").textContent).toContain("Credential retained");
    expect((within(dialog).getByLabelText("API key", {exact: true}) as HTMLInputElement).value).toBe("");
    expect(onSecretsChanged).toHaveBeenCalled();
    await user.click(retry);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createSecretActionMock).toHaveBeenCalledTimes(1);
    expect(createModelEntryActionMock).toHaveBeenCalledTimes(2);
  });

  it("keeps the registered model visible and retries activation alone", async () => {
    setProviderSelfManagedActionMock.mockResolvedValueOnce({ok: false, error: "Activation refused", status: 422});
    const {user, onCreated} = await renderDialog();
    const dialog = screen.getByRole("dialog");
    await fillKnownForm(user, dialog, CREDENTIAL);
    await user.click(within(dialog).getByRole("checkbox", {name: "Make active"}));
    await user.click(within(dialog).getByRole("button", {name: "Create model"}));
    const retry = await within(dialog).findByRole("button", {name: "Retry activation"});
    expect(onCreated).toHaveBeenCalledTimes(1);
    expect(within(dialog).getByRole("alert").textContent).toContain("Model created");
    await user.click(retry);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createSecretActionMock).toHaveBeenCalledTimes(1);
    expect(createModelEntryActionMock).toHaveBeenCalledTimes(1);
    expect(setProviderSelfManagedActionMock).toHaveBeenCalledTimes(2);
  });

  it("requires a credential choice after storage cannot be confirmed", async () => {
    createSecretActionMock.mockResolvedValueOnce({ok: false, error: "Connection interrupted"});
    const {user} = await renderDialog();
    const dialog = screen.getByRole("dialog");
    await fillKnownForm(user, dialog, CREDENTIAL);
    await user.click(within(dialog).getByRole("button", {name: "Create model"}));
    const choose = await within(dialog).findByRole("button", {name: "Choose credential"});
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
    await user.click(choose);
    await waitFor(() => expect((within(dialog).getByLabelText("Key name") as HTMLInputElement).value).toBe(""));
    expect(createSecretActionMock).toHaveBeenCalledTimes(1);
  });
});
