import { SOURCE_URL, VERIFIED_AT } from "@/tests/helpers/catalogue-fixtures";
import React from "react";
import { afterEach, beforeEach, expect, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { TenantModelEntry, TenantModelEntryList, TenantPlatformDefault } from "@/lib/types";
import type { ModelLibrary } from "@/lib/api/model-library-types";

export const MODEL_REGISTRY_HEADER_ORDER = [
  "Provider",
  "Model",
  "Context",
  "Rates $/1M (in / cached / out)",
  "Status",
  "Actions",
] as const;

/** The details dialog renders a relative <Time> → a Radix Tooltip, which needs a
 *  TooltipProvider ancestor (the dashboard layout supplies it in production). */
export function withTooltipProvider(node: React.ReactElement): React.ReactElement {
  return React.createElement(TooltipProvider, null, node);
}

export const getModelLibraryActionMock = vi.fn();

export const listModelEntriesActionMock = vi.fn();
export const listSecretsActionMock = vi.fn();
export const setProviderSelfManagedActionMock = vi.fn();
export const resetProviderActionMock = vi.fn();
export const createModelEntryActionMock = vi.fn();
export const updateModelEntryActionMock = vi.fn();
export const deleteModelEntryActionMock = vi.fn();
export const replaceSecretActionMock = vi.fn();
export const createSecretActionMock = vi.fn();

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  getModelLibraryAction: getModelLibraryActionMock,
  listModelEntriesAction: listModelEntriesActionMock,
  listSecretsAction: listSecretsActionMock,
  setProviderSelfManagedAction: setProviderSelfManagedActionMock,
  resetProviderAction: resetProviderActionMock,
  createModelEntryAction: createModelEntryActionMock,
  updateModelEntryAction: updateModelEntryActionMock,
  deleteModelEntryAction: deleteModelEntryActionMock,
  replaceSecretAction: replaceSecretActionMock,
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({
  createSecretAction: createSecretActionMock,
  deleteSecretAction: vi.fn(),
}));
// ModelCatalogueProvider reads useRouter (401 → sign-in); a stable stub keeps
// its effect single-fire outside a real app-router mount.
export const routerMock = { push: vi.fn() };
vi.mock("next/navigation", () => ({ useRouter: () => routerMock }));

// A transparent DataTable wrapper that also exposes the live onSortChange
// prop — DataTable's own type accepts any string key (any column could opt
// in), but only "model"/"provider" are marked sortable below, so the real
// header never sends anything else. One test drives that boundary directly,
// the way a future column misconfiguration would.
export let capturedOnSortChange: ((key: string) => void) | undefined;
vi.mock("@agentsfleet/design-system", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@agentsfleet/design-system")>();
  return {
    ...actual,
    DataTable: (props: Record<string, unknown>) => {
      capturedOnSortChange = props.onSortChange as ((key: string) => void) | undefined;
      return React.createElement(actual.DataTable, props as never);
    },
  };
});

export function entry(overrides: Partial<TenantModelEntry>): TenantModelEntry {
  return {
    id: "0190aaaa-aaaa-7aaa-aaaa-aaaaaaaaaaaa",
    model_id: "claude-sonnet-5",
    display_name: "Sonnet 5",
    secret_ref: "anthropic-prod",
    provider: "anthropic",
    kind: "provider_key",
    has_key: true,
    active: false,
    created_at: "2026-04-30T00:00:00Z",
    ...overrides,
  };
}

export function registry(
  models: TenantModelEntry[],
  platformDefaultAvailable = true,
  platformDefault?: TenantPlatformDefault,
): TenantModelEntryList {
  return { models, platform_default_available: platformDefaultAvailable, platform_default: platformDefault };
}

export const LIBRARY: ModelLibrary = {
  total: null, next_cursor: null,
  version: "test",
  models: [
    {
      id: "claude-sonnet-5",
      display_name: "Sonnet 5",
      advanced: false,
      provider: "anthropic",
      context_cap_tokens: 200000,
      input_nanos_per_mtok: "3000000000",
      cached_input_nanos_per_mtok: "300000000",
      output_nanos_per_mtok: "15000000000", context_source_url: SOURCE_URL, context_verified_at: VERIFIED_AT, prices: [],
    },
  ],
};

export async function renderTable(initial: TenantModelEntryList) {
  const { default: ModelsRegistryTable } = await import(
    "../../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
  );
  render(withTooltipProvider(React.createElement(ModelsRegistryTable, { workspaceId: "ws_1", initialPage: { ...initial, next_cursor: null, total: null }, initialError: null } as never)));
}

/** Render with explicit paging fields (cursor/total) or a typed read failure. */
export async function renderTablePaged(props: {
  initialPage: (TenantModelEntryList & { next_cursor: string | null; total: number | null }) | null;
  initialError?: { kind: string; detail?: string } | null;
}) {
  const { default: ModelsRegistryTable } = await import(
    "../../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
  );
  render(
    withTooltipProvider(
      React.createElement(ModelsRegistryTable, {
        workspaceId: "ws_1",
        initialPage: props.initialPage,
        initialError: props.initialError ?? null,
      } as never),
    ),
  );
}

/** Renders inside a real ModelCatalogueProvider with the library action mocked,
 * so the Context column's rates join reads a deterministic library. */
export async function renderTableWithLibrary(initial: TenantModelEntryList, library: ModelLibrary = LIBRARY) {
  getModelLibraryActionMock.mockResolvedValue({ ok: true, data: library });
  const { ModelCatalogueProvider } = await import(
    "../../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider"
  );
  const { default: ModelsRegistryTable } = await import(
    "../../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
  );
  render(
    withTooltipProvider(
      React.createElement(
        ModelCatalogueProvider,
        null,
        React.createElement(ModelsRegistryTable, { workspaceId: "ws_1", initialPage: { ...initial, next_cursor: null, total: null }, initialError: null } as never),
      ),
    ),
  );

  // The catalogue is intent-loaded, not mount-loaded, so a bare render leaves
  // it idle and library-sourced rates would never arrive. Focusing an Edit
  // control is the same ungated signal a keyboard user produces, and it is
  // what these two cases need in order to assert the library FALLBACK at all.
  // Rows the server already priced do not depend on this — see the sibling
  // "without depending on the public catalogue" cases, which use renderTable.
  // A registry with no entries has no Edit control, so fall back to the
  // Create-model trigger — which is the only catalogue-consuming affordance
  // such a page has, and carries the same focus intent.
  const intentTarget =
    screen.queryAllByRole("button", { name: /^Edit / })[0] ??
    screen.queryByRole("button", { name: /create model/i });
  if (intentTarget) {
    intentTarget.focus();
    await waitFor(() => expect(getModelLibraryActionMock).toHaveBeenCalled());
  }
}

/** Retry renders as soon as the read fails but stays disabled until the
 *  transition settles. Clicking it while disabled is a silent no-op, so the
 *  wait is what makes the second read actually happen. */
export async function findEnabledRetry(): Promise<HTMLElement> {
  const retry = await screen.findByRole("button", { name: "Retry" });
  await waitFor(() => expect((retry as HTMLButtonElement).disabled).toBe(false));
  return retry;
}

beforeEach(() => {
  vi.clearAllMocks();
  listSecretsActionMock.mockResolvedValue({ ok: true, data: { secrets: [] } });
});
afterEach(() => cleanup());

// The platform-default row is marked by its lock, not by a word: the cell
// dropped "Default" because the lock beside it already said the same thing.
export const PLATFORM_LOCK = "Managed by a platform admin";

