import React from "react";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { LibraryModel } from "@/lib/api/model-library-types";
import type { Secret } from "@/lib/api/secrets";
import { SECRETS_LOAD, type SecretsLoad } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/secrets-load";

export const createModelEntryActionMock = vi.fn();
export const setProviderSelfManagedActionMock = vi.fn();
export const replaceSecretActionMock = vi.fn();
export const createSecretActionMock = vi.fn();

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
const { catalogueState: catalogueMock } = vi.hoisted(() => ({
  catalogueState: {
    models: [] as LibraryModel[],
    status: "ready" as const,
    preload: vi.fn(),
  },
}));
export const catalogueState = catalogueMock;
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider", () => ({
  useModelCatalogue: () => catalogueMock,
  // These suites exercise form shape, not prefetch policy — that has its own
  // suite in model-catalogue-provider.test.tsx. Hover speculation is off so a
  // stray pointer event cannot perturb the call counts asserted below.
  maySpeculateOnHover: () => false,
}));

export const ANTHROPIC_SECRET: Secret = {
  kind: "provider_key",
  name: "anthropic-prod",
  provider: "anthropic",
  created_at: "2026-04-30T00:00:00Z",
};
export const FORM_MODEL = "claude-sonnet-5-5";
export const ROTATED_API_KEY = "sk-ant-rotated-key";

export async function renderDialog(secrets: Secret[] = [], secretsLoad: SecretsLoad = SECRETS_LOAD.ready) {
  const { default: AddModelEntryDialog } = await import(
    "../../app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog"
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

/** Fill the form with an exact custom identifier when the catalogue is empty. */
export async function fillKnownForm(
  user: ReturnType<typeof userEvent.setup>,
  dialog: HTMLElement,
  { name, provider = "anthropic", key }: { name: string; provider?: string; key: string },
) {
  await user.type(within(dialog).getByLabelText(/^name$/i), name);
  await user.type(within(dialog).getByLabelText(/^provider$/i), provider);
  await user.type(within(dialog).getByLabelText(/^model$/i), FORM_MODEL);
  await user.type(within(dialog).getByLabelText(/^api key$/i), key);
}

beforeEach(() => {
  vi.clearAllMocks();
  catalogueState.models = [];
  createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
  replaceSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic-prod" } });
  createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: "1970-01-01T00:00:00.001Z" } });
  setProviderSelfManagedActionMock.mockResolvedValue({
    ok: true,
    data: { mode: "self_managed", provider: "anthropic", model: "claude-sonnet-5", context_cap_tokens: 200000, secret_ref: "anthropic", platform_default_available: true },
  });
});
afterEach(() => {
  cleanup();
});

