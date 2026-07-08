import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Secret } from "@/lib/api/secrets";

const createModelEntryActionMock = vi.fn();
const setProviderSelfManagedActionMock = vi.fn();
const createSecretActionMock = vi.fn();
const routerRefreshMock = vi.fn();

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  createModelEntryAction: createModelEntryActionMock,
  setProviderSelfManagedAction: setProviderSelfManagedActionMock,
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({
  createSecretAction: createSecretActionMock,
}));
// finish() calls router.refresh() so the page-level `secrets` prop (a server
// read, never copied into local state) picks up a secret created just now.
vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefreshMock, push: vi.fn() }) }));

// Model catalogue state the ProviderModelSelect/Provider-field pickers read —
// empty by default (Input-fallback shape); one test below populates it to
// exercise the catalogue-backed <Select> branch for the Provider field.
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

async function renderDialog(secrets: Secret[] = []) {
  const { default: AddModelEntryDialog } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog"
  );
  const onCreated = vi.fn();
  render(React.createElement(AddModelEntryDialog, { workspaceId: "ws_1", secrets, onCreated } as never));
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: /add model/i }));
  await screen.findByRole("dialog");
  return { onCreated, user };
}

beforeEach(() => {
  vi.clearAllMocks();
  catalogueState.models = [];
  createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
  createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: 1 } });
  setProviderSelfManagedActionMock.mockResolvedValue({
    ok: true,
    data: { mode: "self_managed", provider: "anthropic", model: "claude-sonnet-5", context_cap_tokens: 200000, secret_ref: "anthropic", platform_default_available: true },
  });
});
afterEach(() => cleanup());

describe("AddModelEntryDialog — reuse-existing-key", () => {
  it("does nothing when Save is clicked with no stored key or model chosen", async () => {
    const { onCreated, user } = await renderDialog([ANTHROPIC_SECRET]);
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByRole("button", { name: /use existing key/i }));
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    expect(createModelEntryActionMock).not.toHaveBeenCalled();
    expect(onCreated).not.toHaveBeenCalled();
  });

  it("surfaces a register error when reusing a stored key", async () => {
    createModelEntryActionMock.mockResolvedValue({ ok: false, error: "duplicate", errorCode: "UZ-MODELS-003" });
    const { user } = await renderDialog([ANTHROPIC_SECRET]);
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByRole("button", { name: /use existing key/i }));
    await user.click(within(dialog).getByLabelText(/stored key/i));
    await user.click(await screen.findByText(/anthropic-prod/i));
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
  });

  it("shows no key field and creates an entry sharing the stored secret_ref", async () => {
    const { onCreated, user } = await renderDialog([ANTHROPIC_SECRET]);

    await user.click(screen.getByRole("button", { name: /use existing key/i }));
    expect(screen.queryByLabelText(/^api key$/i)).toBeNull();

    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByLabelText(/stored key/i));
    await user.click(await screen.findByText(/anthropic-prod/i));

    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);

    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(createModelEntryActionMock).toHaveBeenCalled());
    expect(createModelEntryActionMock).toHaveBeenCalledWith(
      expect.objectContaining({ secret_ref: "anthropic-prod" }),
    );
    expect(createSecretActionMock).not.toHaveBeenCalled();
    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    // Picks up any secret created elsewhere since page load, not just this one.
    expect(routerRefreshMock).toHaveBeenCalled();
  });
});

describe("AddModelEntryDialog — known provider, new key", () => {
  it("does nothing when Save is clicked with the new-key shape incomplete", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    expect(createSecretActionMock).not.toHaveBeenCalled();
    expect(within(dialog).queryByRole("alert")).toBeNull();
  });

  it("paste-detects the provider and stores a secret with no model field in the body", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    await waitFor(() => expect((within(dialog).getByLabelText(/^provider$/i) as HTMLInputElement).value).toBe("anthropic"));

    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);

    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalled());
    const [, body] = createSecretActionMock.mock.calls[0] as [string, { name: string; data: Record<string, unknown> }];
    expect(body.data.provider).toBe("anthropic");
    expect(body.data.api_key).toBe("sk-ant-e2e-xxxx");
    expect("model" in body.data).toBe(false);
    await waitFor(() => expect(createModelEntryActionMock).toHaveBeenCalled());
  });

  it("picks the provider from a catalogue-backed <Select> when the catalogue has rows", async () => {
    catalogueState.models = [
      { id: "claude-sonnet-5", provider: "anthropic", context_cap_tokens: 200000, input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0 },
    ];
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^api key$/i), "a-key-with-no-known-prefix");
    await user.click(within(dialog).getByLabelText(/^provider$/i));
    await user.click(await screen.findByRole("option", { name: /anthropic/i }));
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalled());
    const [, body] = createSecretActionMock.mock.calls[0] as [string, { data: Record<string, unknown> }];
    expect(body.data.provider).toBe("anthropic");
  });

  it("lets the key name be edited directly, independent of the detected provider", async () => {
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    // A key with no recognised prefix — the provider field stays a free-text
    // <Input> (empty catalogue), so both it and Key name are typed manually.
    await user.type(within(dialog).getByLabelText(/^api key$/i), "a-key-with-no-known-prefix");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");

    const keyName = within(dialog).getByLabelText(/key name/i) as HTMLInputElement;
    await user.clear(keyName);
    await user.type(keyName, "anthropic-secondary");

    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() =>
      expect(createSecretActionMock).toHaveBeenCalledWith(
        "ws_1",
        expect.objectContaining({ name: "anthropic-secondary" }),
      ),
    );
  });

  it("surfaces a store error and never registers an entry", async () => {
    createSecretActionMock.mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-VAULT-002" });
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(createModelEntryActionMock).not.toHaveBeenCalled();
  });

  it("surfaces a register error without activating, when the entry create fails", async () => {
    createModelEntryActionMock.mockResolvedValue({ ok: false, error: "duplicate", errorCode: "UZ-MODELS-003" });
    const { user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.click(within(dialog).getByRole("button", { name: /save & make active/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(setProviderSelfManagedActionMock).not.toHaveBeenCalled();
  });

  it("surfaces an activation error after a successful register", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({ ok: false, error: "rejected", errorCode: "UZ-PROVIDER-003" });
    const { onCreated, user } = await renderDialog();
    const dialog = screen.getByRole("dialog");

    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.click(within(dialog).getByRole("button", { name: /save & make active/i }));

    await waitFor(() => expect(within(dialog).getByRole("alert")).toBeTruthy());
    expect(onCreated).not.toHaveBeenCalled();
  });
});

describe("AddModelEntryDialog — reuse toggle", () => {
  it("switches back to New key after opening Use existing key", async () => {
    const { user } = await renderDialog([ANTHROPIC_SECRET]);
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByRole("button", { name: /use existing key/i }));
    expect(screen.queryByLabelText(/^api key$/i)).toBeNull();

    await user.click(within(dialog).getByRole("button", { name: /^new key$/i }));
    expect(within(dialog).getByLabelText(/^api key$/i)).toBeTruthy();
  });
});
