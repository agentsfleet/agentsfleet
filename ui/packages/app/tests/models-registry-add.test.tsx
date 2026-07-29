import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Secret } from "@/lib/api/secrets";
import { subscribeOnboardingRefresh } from "@/lib/onboarding-refresh";
import { SECRETS_LOAD, type SecretsLoad } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/secrets-load";

const createModelEntryActionMock = vi.fn();
const setProviderSelfManagedActionMock = vi.fn();
const replaceSecretActionMock = vi.fn();
const createSecretActionMock = vi.fn();
let unsubscribeRefresh: (() => void) | null = null;

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  createModelEntryAction: createModelEntryActionMock,
  setProviderSelfManagedAction: setProviderSelfManagedActionMock,
  replaceSecretAction: replaceSecretActionMock,
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
    status: "ready" as const,
    preload: vi.fn(),
  },
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider", () => ({
  useModelCatalogue: () => catalogueState,
  // These suites exercise form shape, not prefetch policy — that has its own
  // suite in model-catalogue-provider.test.tsx. Hover speculation is off so a
  // stray pointer event cannot perturb the call counts asserted below.
  maySpeculateOnHover: () => false,
}));

const ANTHROPIC_SECRET: Secret = {
  kind: "provider_key",
  name: "anthropic-prod",
  provider: "anthropic",
  created_at: 1_777_507_200_000,
};
const ROTATED_API_KEY = "sk-ant-rotated-key";

async function renderDialog(secrets: Secret[] = [], secretsLoad: SecretsLoad = SECRETS_LOAD.ready) {
  const { default: AddModelEntryDialog } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog"
  );
  const onCreated = vi.fn();
  const onSecretsChanged = vi.fn();
  const onSecretsNeeded = vi.fn();
  render(
    React.createElement(AddModelEntryDialog, { workspaceId: "ws_1", secrets, secretsLoad, onCreated, onSecretsChanged, onSecretsNeeded } as never),
  );
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: /create model/i }));
  await screen.findByRole("dialog");
  return { onCreated, onSecretsChanged, onSecretsNeeded, user };
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
  replaceSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
  createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: 1 } });
  setProviderSelfManagedActionMock.mockResolvedValue({
    ok: true,
    data: { mode: "self_managed", provider: "anthropic", model: "claude-sonnet-5", context_cap_tokens: 200000, secret_ref: "anthropic", platform_default_available: true },
  });
});
afterEach(() => {
  unsubscribeRefresh?.();
  unsubscribeRefresh = null;
  cleanup();
});

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
    const openaiSecret: Secret = { kind: "provider_key", name: "openai-prod", provider: "openai", created_at: 1_777_507_200_000 };
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
      created_at: 1_777_507_200_000,
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
    const onboardingRefresh = vi.fn();
    unsubscribeRefresh = subscribeOnboardingRefresh("ws_1", onboardingRefresh);
    createModelEntryActionMock.mockResolvedValue({ ok: false, error: "duplicate", errorCode: "UZ-MODELS-003" });
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await fillKnownForm(user, dialog, { name: "anthropic", key: "sk-ant-e2e-xxxx" });
    await user.click(within(dialog).getByRole("button", { name: /save & make active/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(setProviderSelfManagedActionMock).not.toHaveBeenCalled();
    expect(onboardingRefresh).toHaveBeenCalledTimes(1);
  });

  it.each([
    { secrets: [] as Secret[], name: "anthropic", key: "sk-ant-e2e-xxxx", secretChanged: true },
    { secrets: [ANTHROPIC_SECRET], name: "anthropic-prod", key: ROTATED_API_KEY, secretChanged: false },
  ])("surfaces an activation error after registration with secrets=$secretChanged", async ({
    secrets, name, key, secretChanged,
  }) => {
    const onboardingRefresh = vi.fn();
    unsubscribeRefresh = subscribeOnboardingRefresh("ws_1", onboardingRefresh);
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
    expect(onboardingRefresh).toHaveBeenCalledTimes(secretChanged ? 1 : 0);
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

