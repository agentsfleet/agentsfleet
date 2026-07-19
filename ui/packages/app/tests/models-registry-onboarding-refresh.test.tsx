import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import { subscribeOnboardingRefresh } from "@/lib/onboarding-refresh";
import type { TenantModelEntry, TenantModelEntryList } from "@/lib/types";

const WORKSPACE_ID = "ws_1";
const MODEL_ID = "claude-sonnet-5";
const PROVIDER = "anthropic";
const SECRET_REF = "anthropic-prod";
const CONTEXT_CAP_TOKENS = 200000;
const CREATED_AT = 1_777_507_200_000;
const MODEL_KIND = { providerKey: "provider_key" } as const;
const PROVIDER_MODE = { selfManaged: "self_managed" } as const;
const listModelEntriesActionMock = vi.fn();
const listSecretsActionMock = vi.fn();
const setProviderSelfManagedActionMock = vi.fn();
let unsubscribeRefresh: (() => void) | null = null;

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  getModelLibraryAction: vi.fn(),
  listModelEntriesAction: listModelEntriesActionMock,
  listSecretsAction: listSecretsActionMock,
  setProviderSelfManagedAction: setProviderSelfManagedActionMock,
  resetProviderAction: vi.fn(),
  createModelEntryAction: vi.fn(),
  updateModelEntryAction: vi.fn(),
  deleteModelEntryAction: vi.fn(),
  rotateSecretAction: vi.fn(),
}));

vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({
  createSecretAction: vi.fn(),
  deleteSecretAction: vi.fn(),
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn() }),
}));

function entry(active = false): TenantModelEntry {
  return {
    id: "0190aaaa-aaaa-7aaa-aaaa-aaaaaaaaaaaa",
    model_id: MODEL_ID,
    secret_ref: SECRET_REF,
    provider: PROVIDER,
    kind: MODEL_KIND.providerKey,
    has_key: true,
    active,
    created_at: CREATED_AT,
  };
}

function registry(active = false): TenantModelEntryList {
  return {
    models: [entry(active)],
    platform_default_available: true,
  };
}

async function renderTable() {
  const { default: ModelsRegistryTable } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
  );
  render(
    React.createElement(
      TooltipProvider,
      null,
      React.createElement(ModelsRegistryTable, {
        workspaceId: WORKSPACE_ID,
        initial: registry(),
        initialSecrets: [],
      }),
    ),
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  listSecretsActionMock.mockResolvedValue({ ok: true, data: { secrets: [] } });
});

afterEach(() => {
  unsubscribeRefresh?.();
  unsubscribeRefresh = null;
  cleanup();
});

describe("ModelsRegistryTable onboarding refresh", () => {
  it("should refresh onboarding after activating a saved model entry", async () => {
    const onboardingRefresh = vi.fn();
    unsubscribeRefresh = subscribeOnboardingRefresh(WORKSPACE_ID, onboardingRefresh);
    setProviderSelfManagedActionMock.mockResolvedValue({
      ok: true,
      data: {
        mode: PROVIDER_MODE.selfManaged,
        provider: PROVIDER,
        model: MODEL_ID,
        context_cap_tokens: CONTEXT_CAP_TOKENS,
        secret_ref: SECRET_REF,
        platform_default_available: true,
      },
    });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry(true) });
    await renderTable();

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /switch to claude-sonnet-5/i }));

    await waitFor(() => expect(onboardingRefresh).toHaveBeenCalledTimes(1));
  });

  it("should not refresh onboarding when saved model activation fails", async () => {
    const onboardingRefresh = vi.fn();
    unsubscribeRefresh = subscribeOnboardingRefresh(WORKSPACE_ID, onboardingRefresh);
    setProviderSelfManagedActionMock.mockResolvedValue({
      ok: false,
      error: "rejected",
      errorCode: "UZ-PROVIDER-003",
    });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry() });
    await renderTable();

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /switch to claude-sonnet-5/i }));

    expect(await screen.findByText(/rejected/i)).toBeTruthy();
    expect(onboardingRefresh).not.toHaveBeenCalled();
  });
});
