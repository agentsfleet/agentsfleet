import { SOURCE_URL, VERIFIED_AT } from "@/tests/helpers/catalogue-fixtures";
import { describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import type { Secret } from "@/lib/api/secrets";
import { SECRETS_LOAD } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/secrets-load";
import { createModelEntryActionMock, setProviderSelfManagedActionMock, createSecretActionMock, ANTHROPIC_SECRET, ROTATED_API_KEY, renderDialog, fillKnownForm, catalogueState, FORM_MODEL } from "./helpers/models-registry-add-fixtures";

describe("AddModelEntryDialog — known provider, new key", () => {
  it("disables Save until name, provider, model, AND key are present for a named provider", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");
    const save = () => within(dialog).getByRole("button", { name: /^save$/i });

    expect(save().hasAttribute("disabled")).toBe(true);
    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^model$/i), FORM_MODEL);
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
      { id: "claude-sonnet-5", display_name: "Sonnet 5", advanced: false, provider: "anthropic", context_cap_tokens: 200000, input_nanos_per_mtok: "0", cached_input_nanos_per_mtok: "0", output_nanos_per_mtok: "0", context_source_url: SOURCE_URL, context_verified_at: VERIFIED_AT, prices: [] },
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

  it.each([
    { secrets: [] as Secret[], name: "anthropic", key: "sk-ant-e2e-xxxx", secretChanged: true },
    { secrets: [ANTHROPIC_SECRET], name: "anthropic-prod", key: ROTATED_API_KEY, secretChanged: false },
  ])("surfaces an activation error after registration with secrets=$secretChanged", async ({
    secrets, name, key, secretChanged,
  }) => {
    setProviderSelfManagedActionMock.mockResolvedValue({ ok: false, error: "rejected", errorCode: "UZ-PROVIDER-003" });
    const { onCreated, onSecretsChanged, user } = await renderDialog(secrets);
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name, key });
    await user.click(within(dialog).getByRole("button", { name: /save & make active/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    // The entry itself is already committed server-side by this point — the
    // table must reflect it even though activation failed and the dialog
    // stays open (matches ModelsRegistryTable.onSwitchEntry's "refresh
    // regardless of outcome" convention).
    expect(onCreated).toHaveBeenCalled();
    expect(onSecretsChanged).toHaveBeenCalledTimes(secretChanged ? 1 : 0);
    // Dialog stays open — the user can see the error, not silently closed.
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});

describe("AddModelEntryDialog — the stored-secret list is loaded before submit is possible", () => {
  it("test_add_dialog_loads_secrets_on_open — asks for secrets when opened, not only after one changes", async () => {
    // Regression guard. The eager page-level secrets preload was removed when
    // the Models page became page-bounded; for a while nothing replaced it on
    // the OPEN path, so `secrets` stayed [] in production while every test
    // injected it as a prop and passed.
    //
    // That is not a cosmetic gap. submit() resolves `existing` from this list
    // to choose replace-vs-create and to refuse a name owned by a different
    // provider, and the secrets POST upserts server-side — so an unloaded list
    // does not mean "no options to pick", it means a re-used name silently
    // overwrites the credential already holding it.
    // renderDialog opens the dialog as part of its setup, so reaching an open
    // dialog IS the trigger under test.
    const { onSecretsNeeded } = await renderDialog([]);
    expect(onSecretsNeeded).toHaveBeenCalledTimes(1);
  });

  it("keeps Save disabled while the secret list is in flight, even with a complete form", async () => {
    // Firing the load on open is worthless if a fast hand can submit before it
    // lands: `existing` resolves to undefined against a not-yet-loaded list,
    // skipping the name-ownership guard and taking the create path — an
    // upsert over whatever already holds the name. Fail closed until ready.
    const { user } = await renderDialog([], SECRETS_LOAD.loading);
    const dialog = screen.getByRole("dialog");
    await fillKnownForm(user, dialog, { name: "anthropic", key: "sk-ant-e2e-xxxx" });

    expect(screen.getByText(/checking your stored secrets/i)).toBeTruthy();
    expect((within(dialog).getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(true);
    expect((within(dialog).getByRole("button", { name: /save & make active/i }) as HTMLButtonElement).disabled).toBe(true);
    expect(createSecretActionMock).not.toHaveBeenCalled();
  });

  it("a failed secret-list load keeps Save disabled and offers a retry wired to the same load", async () => {
    const { user, onSecretsNeeded } = await renderDialog([], SECRETS_LOAD.error);
    const dialog = screen.getByRole("dialog");
    await fillKnownForm(user, dialog, { name: "anthropic", key: "sk-ant-e2e-xxxx" });

    expect(screen.getByText(/couldn't load your stored secrets/i)).toBeTruthy();
    expect((within(dialog).getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(true);

    // Retry re-fires the load the open fired — once on open, once here.
    await user.click(within(dialog).getByRole("button", { name: /retry/i }));
    expect(onSecretsNeeded).toHaveBeenCalledTimes(2);
  });
});

