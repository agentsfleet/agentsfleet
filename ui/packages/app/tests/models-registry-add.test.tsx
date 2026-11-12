import { SOURCE_URL, VERIFIED_AT } from "@/tests/helpers/catalogue-fixtures";
import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Secret } from "@/lib/api/secrets";
import { createModelEntryActionMock, setProviderSelfManagedActionMock, replaceSecretActionMock, createSecretActionMock, ANTHROPIC_SECRET, ROTATED_API_KEY, renderDialog, fillKnownForm, catalogueState, FORM_MODEL, modelIdentity } from "./helpers/models-registry-add-fixtures";

describe("AddModelEntryDialog — unified form shape", () => {
  it("waits for the signed-in owner before opening or loading model credentials", async () => {
    const { default: AddModelEntryDialog } = await import("@/app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog");
    const props = {workspaceId: "ws_1", secrets: [], secretsLoad: "ready" as const,
      onCreated: vi.fn(), onSecretsChanged: vi.fn(), onSecretsNeeded: vi.fn()};
    const user = userEvent.setup();
    Object.assign(modelIdentity, {isLoaded: false, isSignedIn: false, userId: null});
    const view = render(<AddModelEntryDialog {...props} />);
    let create = screen.getByRole("button", {name: "Create model"});
    expect(create.hasAttribute("disabled")).toBe(true);
    await user.click(create);
    expect(props.onSecretsNeeded).not.toHaveBeenCalled();
    Object.assign(modelIdentity, {isLoaded: true});
    view.rerender(<AddModelEntryDialog {...props} />);
    expect(create.hasAttribute("disabled")).toBe(true);
    Object.assign(modelIdentity, {isSignedIn: true});
    view.rerender(<AddModelEntryDialog {...props} />);
    expect(create.hasAttribute("disabled")).toBe(true);
    modelIdentity.userId = "model-owner";
    view.rerender(<AddModelEntryDialog {...props} />);
    create = screen.getByRole("button", {name: "Create model"});
    expect(create.hasAttribute("disabled")).toBe(false);
    await user.click(create);
    expect(await screen.findByRole("dialog", {name: "Create model"})).toBeTruthy();
    expect(props.onSecretsNeeded).toHaveBeenCalledOnce();
  });

  it("keeps advanced model visibility beside Model in the tab-free form", async () => {
    await renderDialog();
    const dialog = screen.getByRole("dialog");

    expect(within(dialog).queryByRole("tablist")).toBeNull();
    expect(within(dialog).queryByLabelText(/base url/i)).toBeNull();

    const labels = Array.from(dialog.querySelectorAll("label")).map((l) => l.textContent);
    expect(labels).toEqual(["Provider", "Model", "Show advanced models", "Add new key", "Use saved key", "Key name", "API key", "Make active"]);
    expect(within(dialog).getByRole("switch", { name: "Show advanced models" }).getAttribute("aria-checked")).toBe("false");
  });

  it("typing an API key or a provider never mutates the free-form Name", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    const name = within(dialog).getByLabelText(/^key name$/i) as HTMLInputElement;
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    expect(name.value).toBe("");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    expect(name.value).toBe("");

    await user.type(name, "my-second-anthropic");
    await user.type(within(dialog).getByLabelText(/^model$/i), FORM_MODEL);
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    await user.click(within(dialog).getByRole("button", { name: /^create model$/i }));

    await waitFor(() =>
      expect(createSecretActionMock).toHaveBeenCalledWith("ws_1", expect.objectContaining({ name: "my-second-anthropic" })),
    );
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
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
      { id: "claude-sonnet-5", display_name: "Sonnet 5", advanced: false, provider: "anthropic", context_cap_tokens: 200000, input_nanos_per_mtok: "0", cached_input_nanos_per_mtok: "0", output_nanos_per_mtok: "0", context_source_url: SOURCE_URL, context_verified_at: VERIFIED_AT, prices: [] },
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

describe("AddModelEntryDialog — explicit credential choice", () => {
  it("blocks a held name without rotating it", async () => {
    const {user} = await renderDialog([ANTHROPIC_SECRET]);
    const dialog = screen.getByRole("dialog");
    await fillKnownForm(user, dialog, {name: ANTHROPIC_SECRET.name, key: ROTATED_API_KEY});
    expect(within(dialog).getByRole("button", {name: "Create model"}).hasAttribute("disabled")).toBe(true);
    expect(within(dialog).getByText(/That name is already in use/)).toBeTruthy();
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });

  it("reuses a compatible saved key without sending or replacing credential bytes", async () => {
    const other: Secret = {kind: "provider_key", name: "openai-key", provider: "openai", created_at: "2026-04-30T00:00:00Z"};
    const {user} = await renderDialog([ANTHROPIC_SECRET, other]);
    const dialog = within(screen.getByRole("dialog"));
    await user.type(dialog.getByLabelText("Provider", {exact: true}), "anthropic");
    await user.type(dialog.getByLabelText("Model", {exact: true}), FORM_MODEL);
    await user.click(dialog.getByRole("radio", {name: "Use saved key"}));
    await user.click(dialog.getByLabelText("Saved key", {exact: true}));
    expect(screen.queryByRole("option", {name: other.name})).toBeNull();
    await user.click(screen.getByRole("option", {name: ANTHROPIC_SECRET.name}));
    expect(dialog.queryByLabelText("API key", {exact: true})).toBeNull();
    await user.click(dialog.getByRole("button", {name: "Create model"}));
    await waitFor(() => expect(createModelEntryActionMock).toHaveBeenCalledWith({workspace_id: "ws_1", model_id: FORM_MODEL, secret_ref: ANTHROPIC_SECRET.name}));
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
    expect(setProviderSelfManagedActionMock).not.toHaveBeenCalled();
  });

  it("clears a selected saved key when switching back to adding a new credential", async () => {
    const {user} = await renderDialog([ANTHROPIC_SECRET]);
    const dialog = within(screen.getByRole("dialog"));
    await user.type(dialog.getByLabelText("Provider", {exact: true}), "anthropic");
    await user.click(dialog.getByRole("radio", {name: "Use saved key"}));
    await user.click(dialog.getByLabelText("Saved key", {exact: true}));
    await user.click(screen.getByRole("option", {name: ANTHROPIC_SECRET.name}));
    await user.click(dialog.getByRole("radio", {name: "Add new key"}));
    expect((dialog.getByLabelText("API key", {exact: true}) as HTMLInputElement).value).toBe("");
    await user.click(dialog.getByRole("radio", {name: "Use saved key"}));
    expect(dialog.getByLabelText("Saved key", {exact: true}).textContent).toBe("Select a saved key");
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });

  it("explains an empty compatible-key list and returns to adding a credential", async () => {
    const {user} = await renderDialog(); const dialog = within(screen.getByRole("dialog"));
    await user.type(dialog.getByLabelText("Provider", {exact: true}), "anthropic");
    await user.click(dialog.getByRole("radio", {name: "Use saved key"}));
    expect(dialog.getByText("No saved keys for this provider. Add a new key to continue.")).toBeTruthy();
    expect(dialog.getByLabelText("Saved key", {exact: true}).hasAttribute("disabled")).toBe(true);
    await user.click(dialog.getByRole("radio", {name: "Add new key"}));
    expect(dialog.getByLabelText("API key", {exact: true})).toBeTruthy();
  });
});
