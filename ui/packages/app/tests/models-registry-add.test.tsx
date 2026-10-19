import { SOURCE_URL, VERIFIED_AT } from "@/tests/helpers/catalogue-fixtures";
import { describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import type { Secret } from "@/lib/api/secrets";
import { createModelEntryActionMock, setProviderSelfManagedActionMock, replaceSecretActionMock, createSecretActionMock, ANTHROPIC_SECRET, ROTATED_API_KEY, renderDialog, fillKnownForm, catalogueState, FORM_MODEL } from "./helpers/models-registry-add-fixtures";

describe("AddModelEntryDialog — unified form shape", () => {
  it("keeps advanced model visibility beside Model in the tab-free form", async () => {
    await renderDialog();
    const dialog = screen.getByRole("dialog");

    expect(within(dialog).queryByRole("tablist")).toBeNull();
    expect(within(dialog).queryByLabelText(/base url/i)).toBeNull();

    const labels = Array.from(dialog.querySelectorAll("label")).map((l) => l.textContent);
    expect(labels).toEqual(["Name", "Provider", "Model", "Show advanced models", "API key"]);
    expect(within(dialog).getByRole("switch", { name: "Show advanced models" }).getAttribute("aria-checked")).toBe("false");
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
    await user.type(within(dialog).getByLabelText(/^model$/i), FORM_MODEL);
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

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

describe("AddModelEntryDialog — create-or-replace by name", () => {
  it("replaces the stored body whole when the name already exists with the same provider", async () => {
    const { onCreated, onSecretsChanged, user } = await renderDialog([ANTHROPIC_SECRET]);
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "anthropic-prod", key: ROTATED_API_KEY });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(replaceSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic-prod", { provider: "anthropic", api_key: ROTATED_API_KEY }));
    expect(createSecretActionMock).not.toHaveBeenCalled();
    await waitFor(() => expect(createModelEntryActionMock).toHaveBeenCalledWith(
      expect.objectContaining({ secret_ref: "anthropic-prod" }),
    ));
    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    // A same-shape replace leaves the list-visible metadata identical, so the
    // secrets refetch is skipped — only the entries list refreshes.
    expect(onSecretsChanged).not.toHaveBeenCalled();
  });

  it("errors without writing anything when the name is owned by a different provider's key", async () => {
    const openaiSecret: Secret = { kind: "provider_key", name: "openai-prod", provider: "openai", created_at: "2026-04-30T00:00:00Z" };
    const { user } = await renderDialog([openaiSecret]);
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "openai-prod", key: "sk-ant-e2e-xxxx" });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByText(/different provider/i)).toBeTruthy());
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });

  it("errors without writing when the name is owned by a custom endpoint secret (kind-blind guard closed)", async () => {
    const endpointSecret: Secret = {
      kind: "custom_endpoint",
      name: "vllm-gateway",
      provider: "openai-compatible",
      base_url: "https://vllm.corp/v1",
      created_at: "2026-04-30T00:00:00Z",
    };
    const { user } = await renderDialog([endpointSecret]);
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "vllm-gateway", key: "sk-ant-e2e-xxxx" });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    // Replacement is total — writing {provider, api_key} over an endpoint
    // secret would drop its base_url and break the entries referencing it.
    // The guard must catch non-provider_key kinds too.
    await waitFor(() => expect(within(dialog).getByText(/different provider/i)).toBeTruthy());
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });

  it("surfaces a register error after a successful replace, and leaves the dialog open", async () => {
    createModelEntryActionMock.mockResolvedValue({ ok: false, error: "duplicate", errorCode: "UZ-MODELS-003" });
    const { user } = await renderDialog([ANTHROPIC_SECRET]);
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "anthropic-prod", key: ROTATED_API_KEY });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(replaceSecretActionMock).toHaveBeenCalled());
    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("surfaces a replace error and never registers an entry", async () => {
    replaceSecretActionMock.mockResolvedValue({ ok: false, error: "rejected", errorCode: "UZ-REQ-001" });
    const { user } = await renderDialog([ANTHROPIC_SECRET]);
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "anthropic-prod", key: ROTATED_API_KEY });
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });
});

