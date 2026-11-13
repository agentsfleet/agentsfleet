import { describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import type { Secret } from "@/lib/api/secrets";
import { createModelEntryActionMock, setProviderSelfManagedActionMock, createSecretActionMock, replaceSecretActionMock, renderDialog, FORM_MODEL } from "./helpers/models-registry-add-fixtures";

const PROVIDER = "openai-compatible";
const URL = "https://models.example/v1";
async function customForm(user: Awaited<ReturnType<typeof renderDialog>>["user"], endpoint = URL) {
  const dialog = within(screen.getByRole("dialog"));
  await user.type(dialog.getByLabelText("Provider", {exact: true}), PROVIDER);
  await user.type(dialog.getByLabelText("Model", {exact: true}), FORM_MODEL);
  await user.type(dialog.getByLabelText("Key name"), "gateway");
  await user.type(dialog.getByLabelText("Base URL"), endpoint);
  return dialog;
}

describe("Create model with a compatible endpoint", () => {
  it("requires a key for an HTTPS endpoint and supports optional activation", async () => {
    const {user} = await renderDialog();
    const dialog = await customForm(user);
    expect(dialog.getByRole("button", {name: "Create model"}).hasAttribute("disabled")).toBe(true);
    await user.type(dialog.getByLabelText("API key", {exact: true}), "gateway-fixture-key");
    await user.click(dialog.getByRole("checkbox", {name: "Make active"}));
    await user.click(dialog.getByRole("button", {name: "Create model"}));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createSecretActionMock).toHaveBeenCalledWith("ws_1", {name: "gateway", data: {provider: PROVIDER, base_url: URL, api_key: "gateway-fixture-key"}});
    expect(createModelEntryActionMock).toHaveBeenCalledWith({workspace_id: "ws_1", model_id: FORM_MODEL, secret_ref: "gateway"});
    expect(setProviderSelfManagedActionMock).toHaveBeenCalledWith({workspace_id: "ws_1", model: FORM_MODEL, secret_ref: "gateway"});
  });

  it("refuses an insecure endpoint before any write", async () => {
    const {user} = await renderDialog();
    const dialog = await customForm(user, "http://models.example/v1");
    await user.type(dialog.getByLabelText("API key", {exact: true}), "gateway-fixture-key");
    await user.click(dialog.getByRole("button", {name: "Create model"}));
    expect(dialog.getByRole("alert").textContent).toMatch(/https/i);
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });

  it("reuses a saved endpoint without rewriting its URL or hidden credential", async () => {
    const saved: Secret = {name: "gateway", kind: "custom_endpoint", provider: PROVIDER, base_url: URL, created_at: "2026-04-30T00:00:00Z"};
    const {user} = await renderDialog([saved]);
    const dialog = within(screen.getByRole("dialog"));
    await user.type(dialog.getByLabelText("Provider", {exact: true}), PROVIDER);
    await user.type(dialog.getByLabelText("Model", {exact: true}), FORM_MODEL);
    await user.click(dialog.getByRole("radio", {name: "Use saved key"}));
    await user.click(dialog.getByLabelText("Saved key", {exact: true}));
    await user.click(screen.getByRole("option", {name: saved.name}));
    expect(dialog.getByText(`Endpoint: ${URL}`)).toBeTruthy();
    expect(dialog.queryByLabelText("Base URL")).toBeNull();
    await user.click(dialog.getByRole("button", {name: "Create model"}));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });
});
