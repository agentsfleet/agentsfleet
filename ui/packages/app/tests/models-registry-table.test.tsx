import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { TenantModelEntry, TenantModelEntryList, TenantPlatformDefault } from "@/lib/types";
import type { ModelLibrary } from "@/lib/api/model_library";

const MODEL_REGISTRY_HEADER_ORDER = [
  "Provider",
  "Model",
  "Context · $/1M (in / cached / out)",
  "Status",
  "Actions",
] as const;

/** The details dialog renders a relative <Time> → a Radix Tooltip, which needs a
 *  TooltipProvider ancestor (the dashboard layout supplies it in production). */
function withTooltipProvider(node: React.ReactElement): React.ReactElement {
  return React.createElement(TooltipProvider, null, node);
}

const getModelLibraryActionMock = vi.fn();

const listModelEntriesActionMock = vi.fn();
const listSecretsActionMock = vi.fn();
const setProviderSelfManagedActionMock = vi.fn();
const resetProviderActionMock = vi.fn();
const createModelEntryActionMock = vi.fn();
const updateModelEntryActionMock = vi.fn();
const deleteModelEntryActionMock = vi.fn();
const rotateSecretActionMock = vi.fn();
const createSecretActionMock = vi.fn();

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  getModelLibraryAction: getModelLibraryActionMock,
  listModelEntriesAction: listModelEntriesActionMock,
  listSecretsAction: listSecretsActionMock,
  setProviderSelfManagedAction: setProviderSelfManagedActionMock,
  resetProviderAction: resetProviderActionMock,
  createModelEntryAction: createModelEntryActionMock,
  updateModelEntryAction: updateModelEntryActionMock,
  deleteModelEntryAction: deleteModelEntryActionMock,
  rotateSecretAction: rotateSecretActionMock,
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({
  createSecretAction: createSecretActionMock,
  deleteSecretAction: vi.fn(),
}));
// ModelCatalogueProvider reads useRouter (401 → sign-in); a stable stub keeps
// its effect single-fire outside a real app-router mount.
const routerMock = { push: vi.fn() };
vi.mock("next/navigation", () => ({ useRouter: () => routerMock }));

// A transparent DataTable wrapper that also exposes the live onSortChange
// prop — DataTable's own type accepts any string key (any column could opt
// in), but only "model"/"provider" are marked sortable below, so the real
// header never sends anything else. One test drives that boundary directly,
// the way a future column misconfiguration would.
let capturedOnSortChange: ((key: string) => void) | undefined;
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

function entry(overrides: Partial<TenantModelEntry>): TenantModelEntry {
  return {
    id: "0190aaaa-aaaa-7aaa-aaaa-aaaaaaaaaaaa",
    model_id: "claude-sonnet-5",
    secret_ref: "anthropic-prod",
    provider: "anthropic",
    kind: "provider_key",
    has_key: true,
    active: false,
    created_at: 1_777_507_200_000,
    ...overrides,
  };
}

function registry(
  models: TenantModelEntry[],
  platformDefaultAvailable = true,
  platformDefault?: TenantPlatformDefault,
): TenantModelEntryList {
  return { models, platform_default_available: platformDefaultAvailable, platform_default: platformDefault };
}

const LIBRARY: ModelLibrary = {
  version: "test",
  models: [
    {
      id: "claude-sonnet-5",
      provider: "anthropic",
      context_cap_tokens: 200000,
      input_nanos_per_mtok: 3_000_000_000,
      cached_input_nanos_per_mtok: 300_000_000,
      output_nanos_per_mtok: 15_000_000_000,
    },
  ],
};

async function renderTable(initial: TenantModelEntryList) {
  const { default: ModelsRegistryTable } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
  );
  render(withTooltipProvider(React.createElement(ModelsRegistryTable, { workspaceId: "ws_1", initialPage: { ...initial, next_cursor: null, total: null }, initialError: null } as never)));
}

/** Render with explicit paging fields (cursor/total) or a typed read failure. */
async function renderTablePaged(props: {
  initialPage: (TenantModelEntryList & { next_cursor: string | null; total: number | null }) | null;
  initialError?: { kind: string; detail?: string } | null;
}) {
  const { default: ModelsRegistryTable } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
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
async function renderTableWithLibrary(initial: TenantModelEntryList, library: ModelLibrary = LIBRARY) {
  getModelLibraryActionMock.mockResolvedValue({ ok: true, data: library });
  const { ModelCatalogueProvider } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider"
  );
  const { default: ModelsRegistryTable } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
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

beforeEach(() => {
  vi.clearAllMocks();
  listSecretsActionMock.mockResolvedValue({ ok: true, data: { secrets: [] } });
});
afterEach(() => cleanup());

describe("ModelsRegistryTable", () => {
  it("renders Provider before Model in the registry table", async () => {
    await renderTable(registry([entry({})]));
    const headers = screen.getAllByRole("columnheader").map((h) => h.textContent);
    expect(headers).toEqual([...MODEL_REGISTRY_HEADER_ORDER]);
  });

  it("renders N entries plus the pinned Default row first; sorting never unpins Default", async () => {
    const entries = Array.from({ length: 9 }, (_, i) =>
      entry({ id: `e${i}`, model_id: `model-${i}`, provider: i % 2 === 0 ? "anthropic" : "openai" }),
    );
    await renderTable(registry(entries));

    const rows = screen.getAllByRole("row");
    // 1 header row + 1 Default row + 9 entry rows.
    expect(rows).toHaveLength(11);
    expect(within(rows[1]!).getByText("Default")).toBeTruthy();

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /^model$/i }));
    const afterSort = screen.getAllByRole("row");
    expect(within(afterSort[1]!).getByText("Default")).toBeTruthy();
  });

  it("Switch on an inactive row activates it with (secret_ref, model_id); no key input renders", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({
      ok: true,
      data: { mode: "self_managed", provider: "anthropic", model: "claude-sonnet-5", context_cap_tokens: 200000, secret_ref: "anthropic-prod", platform_default_available: true },
    });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ active: true })]) });
    await renderTable(registry([entry({})]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /switch to claude-sonnet-5/i }));
    expect(setProviderSelfManagedActionMock).toHaveBeenCalledWith({ secret_ref: "anthropic-prod", model: "claude-sonnet-5" });
    expect(screen.queryByLabelText(/api key/i)).toBeNull();
  });

  it("hides the platform row entirely for a self-managed tenant with no platform default", async () => {
    // Nothing to act on and nothing in effect: the tenant's own entry wins
    // resolution and no default exists to switch to. Showing a locked row there
    // read as a broken setting.
    await renderTable(registry([entry({ active: true })], false));
    expect(screen.queryByText("Default")).toBeNull();
    expect(screen.queryByText("No default is configured.")).toBeNull();
    expect(screen.queryByRole("button", { name: /use default/i })).toBeNull();
  });

  it("keeps the platform row, disabled with explanatory copy, when a default exists but is not in effect", async () => {
    await renderTable(registry([entry({ active: true })], true));
    expect(screen.getByText("Default")).toBeTruthy();
    expect(screen.getByRole("button", { name: /use default/i })).toBeTruthy();
  });

  it("warns instead of claiming Active when nothing is active and no platform default exists", async () => {
    // The regression this pins: isDefaultLive once tested only "no active
    // entry", so a fresh tenant on a fresh install (core.model_library ships
    // empty, so no default can exist) saw a green Active badge on a default
    // that did not exist — and the badge suppressed the warning. That tenant's
    // first fleet run fails PlatformKeyMissing.
    await renderTable(registry([], false));
    const rows = screen.getAllByRole("row");
    const defaultRow = within(rows[1]!);
    expect(defaultRow.queryByText("Active")).toBeNull();
    expect(screen.getByText("No default is configured.")).toBeTruthy();
  });

  it("shows Active on the Default row and no action button when nothing else is active", async () => {
    await renderTable(registry([]));
    const rows = screen.getAllByRole("row");
    const defaultRow = within(rows[1]!);
    expect(defaultRow.getByText("Active")).toBeTruthy();
    expect(defaultRow.queryByRole("button", { name: /use default/i })).toBeNull();
  });

  it("sorting by Provider toggles ascending/descending without unpinning Default", async () => {
    const entries = [
      entry({ id: "e1", model_id: "m1", provider: "openai" }),
      entry({ id: "e2", model_id: "m2", provider: "anthropic" }),
      // No provider — the sort comparator's `?? ""` fallback, both entries.
      entry({ id: "e3", model_id: "m3", provider: undefined }),
    ];
    await renderTable(registry(entries));
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /^provider$/i }));
    let rows = screen.getAllByRole("row");
    expect(within(rows[1]!).getByText("Default")).toBeTruthy();
    // Ascending: "" (no provider) sorts before named providers.
    expect(within(rows[2]!).getByText("Unknown")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: /^provider$/i }));
    rows = screen.getAllByRole("row");
    expect(within(rows[2]!).getByText("OpenAI")).toBeTruthy();
  });

  it("Switch surfaces a friendly error and still refreshes (Failure Modes: stale activation)", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({ ok: false, error: "rejected", errorCode: "UZ-PROVIDER-003" });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({})]) });
    await renderTable(registry([entry({})]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /switch to claude-sonnet-5/i }));
    await screen.findByText(/rejected/i);
    await waitFor(() => expect(listModelEntriesActionMock).toHaveBeenCalled());
  });

  it("Use default activates the platform default and refreshes the list", async () => {
    resetProviderActionMock.mockResolvedValue({ ok: true, data: { mode: "platform" } });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([]) });
    // provider: undefined on the active entry — exercises the `?? ""`
    // fallback that names the outgoing provider for the reset-analytics call.
    await renderTable(registry([entry({ active: true, provider: undefined })]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /use default/i }));
    expect(resetProviderActionMock).toHaveBeenCalled();
    await screen.findByText("Active");
  });

  it("Use default surfaces a friendly error and still refreshes", async () => {
    resetProviderActionMock.mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-INTERNAL-003" });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ active: true })]) });
    await renderTable(registry([entry({ active: true })]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /use default/i }));
    await screen.findByText(/boom/i);
    await waitFor(() => expect(listModelEntriesActionMock).toHaveBeenCalled());
  });

  it("a failed refresh after Switch leaves the existing rows in place", async () => {
    setProviderSelfManagedActionMock.mockResolvedValue({
      ok: true,
      data: { mode: "self_managed", provider: "anthropic", model: "claude-sonnet-5", context_cap_tokens: 200000, secret_ref: "anthropic-prod", platform_default_available: true },
    });
    listModelEntriesActionMock.mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-INTERNAL-003" });
    await renderTable(registry([entry({})]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /switch to claude-sonnet-5/i }));
    await waitFor(() => expect(listModelEntriesActionMock).toHaveBeenCalled());
    expect(screen.getByText("claude-sonnet-5")).toBeTruthy();
  });

  it("View details opens the read-only dialog straight from the inline icon button", async () => {
    await renderTable(registry([entry({})]));
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /view details for claude-sonnet-5/i }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("heading", { name: "claude-sonnet-5" })).toBeTruthy();
  });

  it("row actions are inline icon buttons — view/switch/edit/remove, no dropdown menu; switch absent and remove disabled on the active row", async () => {
    await renderTable(registry([entry({ id: "e1", model_id: "claude-sonnet-5", active: false }), entry({ id: "e2", model_id: "claude-opus-4-8", active: true })]));

    // Inactive row: all four actions.
    expect(screen.getByRole("button", { name: /view details for claude-sonnet-5/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /switch to claude-sonnet-5/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /edit claude-sonnet-5/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^remove claude-sonnet-5$/i })).toBeTruthy();

    // Active row: no switch; remove disabled with the explanatory label.
    expect(screen.queryByRole("button", { name: /switch to claude-opus-4-8/i })).toBeNull();
    const disabledRemove = screen.getByRole("button", { name: /cannot remove claude-opus-4-8 while it is active/i });
    expect(disabledRemove.hasAttribute("disabled")).toBe(true);

    // The overflow dropdown is gone entirely.
    expect(screen.queryByRole("button", { name: /row actions/i })).toBeNull();
  });

  it("default row renders the platform default's model, provider, context, and library rates", async () => {
    await renderTableWithLibrary(
      registry([], true, { provider: "anthropic", model: "claude-sonnet-5", context_cap_tokens: 200000 }),
    );

    const rows = screen.getAllByRole("row");
    const defaultRow = within(rows[1]!);
    expect(defaultRow.getByText("Default")).toBeTruthy();
    expect(defaultRow.getByText("claude-sonnet-5")).toBeTruthy();
    expect(defaultRow.getByText("Anthropic")).toBeTruthy();
    expect(defaultRow.getByText("200k")).toBeTruthy();
    await waitFor(() => expect(defaultRow.getByText("3.00 / 0.30 / 15.00")).toBeTruthy());
  });

  it("default row renders server-provided rates when the public catalogue is unavailable", async () => {
    await renderTable(
      registry([], true, {
        provider: "anthropic",
        model: "claude-sonnet-5",
        context_cap_tokens: 200000,
        input_nanos_per_mtok: 3_000_000_000,
        cached_input_nanos_per_mtok: 300_000_000,
        output_nanos_per_mtok: 15_000_000_000,
      }),
    );

    const rows = screen.getAllByRole("row");
    const defaultRow = within(rows[1]!);
    expect(defaultRow.getByText("200k")).toBeTruthy();
    expect(defaultRow.getByText("3.00 / 0.30 / 15.00")).toBeTruthy();
  });

  it("default row degrades to '—' when no platform default identity rides the list", async () => {
    // No active entry, so the platform row still renders (it is what would run)
    // — but with no identity to show it must degrade rather than invent one.
    await renderTable(registry([], false));
    const rows = screen.getAllByRole("row");
    const defaultRow = within(rows[1]!);
    expect(defaultRow.getByText("—")).toBeTruthy();
    expect(screen.getByText("No default is configured.")).toBeTruthy();
  });

  it("entry rows price from the library when known and say who bills otherwise", async () => {
    await renderTableWithLibrary(
      registry([
        entry({ id: "e1", model_id: "claude-sonnet-5", provider: "anthropic", context_cap_tokens: 200000 }),
        entry({ id: "e2", model_id: "local-model", provider: "openai-compatible", base_url: "https://vllm.corp/v1", context_cap_tokens: 32000 }),
      ]),
    );

    await waitFor(() => expect(screen.getByText("3.00 / 0.30 / 15.00")).toBeTruthy());
    const rows = screen.getAllByRole("row");
    // Row order: header, Default, sonnet (priced), local (unpriced).
    const localRow = within(rows[3]!);
    expect(localRow.getByText("32k")).toBeTruthy();
    // A tenant entry is self-managed by definition, so an unpriced row is "not
    // applicable", not a lookup miss — and a price here would imply agentsfleet
    // is charging it when the tenant's own provider bills them directly.
    expect(localRow.getByText("Billed by provider")).toBeTruthy();
    expect(localRow.queryByText("Rates unavailable")).toBeNull();
  });

  it("entry rows render server-provided rates without depending on the public catalogue", async () => {
    await renderTable(
      registry([
        entry({
          id: "e1",
          model_id: "claude-sonnet-5",
          provider: "anthropic",
          context_cap_tokens: 200000,
          input_nanos_per_mtok: 3_000_000_000,
          cached_input_nanos_per_mtok: 300_000_000,
          output_nanos_per_mtok: 15_000_000_000,
        }),
      ]),
    );

    expect(screen.getByText("200k")).toBeTruthy();
    expect(screen.getByText("3.00 / 0.30 / 15.00")).toBeTruthy();
  });

  it("shows the 'no key · local' badge on an entry with no key, and the endpoint host in the Provider cell", async () => {
    await renderTable(
      registry([entry({ provider: "openai-compatible", base_url: "https://vllm.corp/v1", has_key: false })]),
    );
    expect(screen.getByText("no key · local")).toBeTruthy();
    expect(screen.getByText("https://vllm.corp/v1")).toBeTruthy();
  });

  it("shows Unknown in the Provider cell when the entry has no provider", async () => {
    await renderTable(registry([entry({ provider: undefined })]));
    expect(screen.getByText("Unknown")).toBeTruthy();
  });

  it("ignores an onSortChange call for a key outside the sortable column set", async () => {
    await renderTable(registry([entry({})]));
    const rowsBefore = screen.getAllByRole("row").length;

    capturedOnSortChange?.("status");

    expect(screen.getAllByRole("row").length).toBe(rowsBefore);
    expect(screen.getByRole("columnheader", { name: "Model" }).getAttribute("aria-sort")).toBe("none");
  });

  it("Load more appends the next page, retains prior rows, and discloses the unnamed remainder", async () => {
    // The registry's own Invariant-5 surface, mirroring the gallery suite: the
    // walk this replaced guaranteed every row was present; paging must retain
    // what is loaded and say what is not.
    listModelEntriesActionMock.mockResolvedValue({
      ok: true,
      data: { ...registry([entry({ id: "e2", model_id: "gpt-5", secret_ref: "openai" })]), next_cursor: null, total: null },
    });
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    // total is null on every daemon endpoint today, so this wording is the
    // one production users actually see.
    expect(screen.getByText("Showing 1 models — more available")).toBeTruthy();

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Load more" }));

    expect(listModelEntriesActionMock).toHaveBeenCalledWith("cur-2");
    expect(await screen.findByText("gpt-5")).toBeTruthy();
    // Prior rows RETAINED, not replaced.
    expect(screen.getByText("claude-sonnet-5")).toBeTruthy();
    // Last page reached — the affordance and its disclosure both retire.
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
  });

  it("a failed load-more keeps the rows on screen and offers retry — never an empty registry", async () => {
    listModelEntriesActionMock.mockResolvedValue({ ok: false, error: "upstream 503", status: 503 });
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Load more" }));

    expect(screen.getByText("claude-sonnet-5")).toBeTruthy();
    // The preserved 503 keeps its specific instruction.
    expect(await screen.findByText("Models are temporarily unavailable. Your entries are safe.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
  });

  it("a failed server read renders the typed failure on the real table, distinct from empty", async () => {
    await renderTablePaged({
      initialPage: null,
      initialError: { kind: "unavailable" },
    });
    expect(screen.getByText("Models are temporarily unavailable. Your entries are safe.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
  });

  it("treats a cursor that does not advance as the last page instead of appending forever", async () => {
    listModelEntriesActionMock.mockResolvedValue({
      ok: true,
      data: { ...registry([entry({ id: "e2", model_id: "gpt-5", secret_ref: "openai" })]), next_cursor: "cur-2", total: null },
    });
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Load more" }));
    expect(await screen.findByText("gpt-5")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
  });

  it("a rejected action round-trip surfaces as a read failure instead of escaping the transition", async () => {
    listModelEntriesActionMock.mockRejectedValue(new Error("network down"));
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Load more" }));
    expect(await screen.findByText("Could not load your models. They have not been changed.")).toBeTruthy();
    expect(screen.getByText("claude-sonnet-5")).toBeTruthy();
  });

  it("a thrown non-Error still reports the failure, with no fabricated detail", async () => {
    // `detail` is the thrown message, and only an Error carries one. A rejected
    // string (or anything else) must still surface the failure rather than
    // render `undefined` at the operator as if it were a reason.
    listModelEntriesActionMock.mockRejectedValue("bare string, not an Error");
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Load more" }));
    expect(await screen.findByText("Could not load your models. They have not been changed.")).toBeTruthy();
    expect(screen.queryByText(/bare string, not an Error/)).toBeNull();
  });

  it("readErrorFrom maps a preserved transport status onto the typed vocabulary", async () => {
    const { readErrorFrom } = await import("@/lib/api/library-types");
    expect(readErrorFrom({ error: "no session", status: 401 }).kind).toBe("unauthenticated");
    expect(readErrorFrom({ error: "nope", status: 403 }).kind).toBe("forbidden");
    expect(readErrorFrom({ error: "down", status: 503 }).kind).toBe("unavailable");
    // No status (thrown before transport, or a non-Api failure) → unknown,
    // never a guessed specific instruction.
    expect(readErrorFrom({ error: "boom" }).kind).toBe("unknown");
    expect(readErrorFrom({ error: "boom" }).detail).toBe("boom");
  });

  it("libraryErrorFromCause reads a status off the cause and a detail only off an Error", async () => {
    const { libraryErrorFromCause } = await import("@/lib/api/library-types");
    // An ApiError-shaped throw keeps its specific kind.
    expect(libraryErrorFromCause({ status: 503 }).kind).toBe("unavailable");
    // A thrown Error contributes its message as the detail...
    expect(libraryErrorFromCause(new Error("threw")).detail).toBe("threw");
    expect(libraryErrorFromCause(new Error("threw")).kind).toBe("unknown");
    // ...and a thrown non-Error has no message to contribute, so it must not
    // invent one rather than render `undefined` at the operator as a reason.
    expect(libraryErrorFromCause("bare string").detail).toBeUndefined();
  });

  it("readErrorCopy gives each failure kind its own next step", async () => {
    const { readErrorCopy } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/registry-view"
    );
    const { LIBRARY_ERROR_KIND } = await import("@/lib/api/library-types");
    // Five kinds, five distinct instructions — collapsing any two loses the
    // reason these are typed at all.
    const copies = Object.values(LIBRARY_ERROR_KIND).map((kind) => readErrorCopy({ kind }));
    expect(new Set(copies).size).toBe(copies.length);
    expect(readErrorCopy({ kind: LIBRARY_ERROR_KIND.unauthenticated })).toMatch(/sign in/i);
    expect(readErrorCopy({ kind: LIBRARY_ERROR_KIND.unavailable })).toMatch(/temporarily unavailable/i);
  });

  it("computeNextSort ignores a key outside the sortable column set, and toggles both directions", async () => {
    const { computeNextSort } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/registry-view"
    );
    expect(computeNextSort(null, "status")).toBeNull();
    expect(computeNextSort({ key: "model", dir: "ascending" }, "actions")).toBeNull();
    expect(computeNextSort({ key: "model", dir: "ascending" }, "model")).toEqual({ key: "model", dir: "descending" });
    expect(computeNextSort({ key: "model", dir: "descending" }, "model")).toEqual({ key: "model", dir: "ascending" });
  });

  it("sortValueFor reads model_id for the model column and provider (or '') for the provider column", async () => {
    const { sortValueFor } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/registry-view"
    );
    const e = entry({ model_id: "claude-sonnet-5", provider: "anthropic" });
    expect(sortValueFor(e, "model")).toBe("claude-sonnet-5");
    expect(sortValueFor(e, "provider")).toBe("anthropic");
    expect(sortValueFor(entry({ provider: undefined }), "provider")).toBe("");
  });

  it("formats the context column at and below the 'k' abbreviation threshold", async () => {
    await renderTable(
      registry([
        entry({ id: "e1", model_id: "m1", context_cap_tokens: 200000 }),
        entry({ id: "e2", model_id: "m2", context_cap_tokens: 500 }),
      ]),
    );
    expect(screen.getByText("200k")).toBeTruthy();
    expect(screen.getByText("500")).toBeTruthy();
  });

  it("renders an explicit 0-token cap as '0', not '—' (nullish guard, not falsy)", async () => {
    await renderTable(registry([entry({ id: "e1", model_id: "m1", context_cap_tokens: 0 })]));
    expect(screen.getByText("0")).toBeTruthy();
  });

  it("formatRates still names unavailable rates for its remaining direct callers", async () => {
    // The entry-row path now routes a missing rate to "Billed by provider", so
    // the null arm survives only for callers outside that path (the admin
    // catalogue presentation). Pin it directly so the guard cannot rot unseen.
    const { formatRates } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells"
    );
    expect(formatRates(null)).toBe("Rates unavailable");
  });

  it("renders a dash for absent context and names who bills", async () => {
    await renderTable(registry([entry({ id: "e1", model_id: "m1", context_cap_tokens: undefined })]));
    const rows = screen.getAllByRole("row");
    const contextCell = within(rows[2]!);
    expect(contextCell.getByText("—")).toBeTruthy();
    expect(contextCell.getByText("Billed by provider")).toBeTruthy();
  });

  it("creating a model entry refreshes the secrets list — a repeat add on the same key name rotates instead of re-creating", async () => {
    createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    rotateSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: 1 } });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ id: "e1", secret_ref: "anthropic" })]) });
    // The dialog now loads the stored-secret list on OPEN as well as after a
    // secret changes, so the sequence has to mirror reality: nothing stored
    // when the dialog first opens, "anthropic" present from the create onward.
    // A single mockResolvedValueOnce left later calls resolving `undefined`,
    // which the real action never does.
    listSecretsActionMock
      .mockResolvedValueOnce({ ok: true, data: { secrets: [] } })
      .mockResolvedValue({
        ok: true,
        data: { secrets: [{ kind: "provider_key", name: "anthropic", provider: "anthropic", created_at: 1 }] },
      });
    await renderTable(registry([]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(listSecretsActionMock).toHaveBeenCalledWith("ws_1"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // Second add reusing the same name ("anthropic") — the refreshed secrets
    // state now carries that name, so the dialog rotates the stored key in
    // place instead of re-creating it. This is the observable proof the
    // refreshSecrets round-trip landed in state.
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const reopened = await screen.findByRole("dialog");
    await user.type(within(reopened).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(reopened).getByLabelText(/^provider$/i), "anthropic");
    await user.click(within(reopened).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.type(within(reopened).getByLabelText(/^api key$/i), "sk-ant-second-key");
    await user.click(within(reopened).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(rotateSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic", "sk-ant-second-key"));
    expect(createSecretActionMock).toHaveBeenCalledTimes(1);
  });

  it("a failed refresh after a good load keeps the stored-key state live — rotate still resolves", async () => {
    // `ready` is sticky: once a real list has arrived, a failed background
    // refresh must neither blank it (locking a form that has usable data) nor
    // lose the rotate-vs-create resolution it feeds. Only a list that NEVER
    // loaded fails closed — that case is the test below.
    rotateSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: 1 } });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ id: "e1", secret_ref: "anthropic" })]) });
    listSecretsActionMock
      .mockResolvedValueOnce({
        ok: true,
        data: { secrets: [{ kind: "provider_key", name: "anthropic", provider: "anthropic", created_at: 1 }] },
      })
      .mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-INTERNAL-003" });
    await renderTable(registry([]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(rotateSecretActionMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // Reopen: the on-open refresh now FAILS, but the earlier list is retained —
    // no fail-closed alert, and the same name still resolves to rotate.
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const reopened = await screen.findByRole("dialog");
    expect(within(reopened).queryByText(/couldn't load your stored secrets/i)).toBeNull();
    await user.type(within(reopened).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(reopened).getByLabelText(/^provider$/i), "anthropic");
    await user.click(within(reopened).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.type(within(reopened).getByLabelText(/^api key$/i), "sk-ant-second-key");
    await user.click(within(reopened).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(rotateSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic", "sk-ant-second-key"));
    expect(createSecretActionMock).not.toHaveBeenCalled();
  });

  it("a failed first secret-list load fails closed — Save stays disabled until a retry lands", async () => {
    // The dialog's rotate-vs-create decision and its name-ownership guard both
    // read the stored-secret list, and the secrets POST upserts server-side.
    // Submitting against a list that never arrived would therefore stomp
    // whatever already holds the typed name — so an unloaded list must BLOCK,
    // not silently take the create path.
    createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: 1 } });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ id: "e1", secret_ref: "anthropic" })]) });
    listSecretsActionMock
      .mockResolvedValueOnce({ ok: false, error: "boom", errorCode: "UZ-INTERNAL-003" })
      .mockResolvedValue({ ok: true, data: { secrets: [] } });
    await renderTable(registry([]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");

    // Complete form, unknown secret list: both Save buttons are inert and the
    // dialog says why.
    await screen.findByText(/couldn't load your stored secrets/i);
    expect((within(dialog).getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(true);
    expect(createSecretActionMock).not.toHaveBeenCalled();

    // Retry loads the list; with it present, the same submit goes through.
    await user.click(within(dialog).getByRole("button", { name: /retry/i }));
    await waitFor(() =>
      expect((within(dialog).getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(false),
    );
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(rotateSecretActionMock).not.toHaveBeenCalled();
  });

  it("fails closed when the secrets round-trip rejects, not just when it returns an error", async () => {
    // A rejected action is a different failure from `{ ok: false }`: the call
    // never came back at all (network drop, deploy skew). Without the catch the
    // dialog strands at "Checking your stored secrets…" — Save disabled forever
    // and no Retry, because Retry only renders in the error state.
    createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: 1 } });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ id: "e1", secret_ref: "anthropic" })]) });
    listSecretsActionMock
      .mockRejectedValueOnce(new Error("network down"))
      .mockResolvedValue({ ok: true, data: { secrets: [] } });
    await renderTable(registry([]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");

    // Complete form, rejected list: same fail-closed surface as `ok: false`.
    await screen.findByText(/couldn't load your stored secrets/i);
    expect((within(dialog).getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(true);
    expect(createSecretActionMock).not.toHaveBeenCalled();

    // And the same Retry recovers it.
    await user.click(within(dialog).getByRole("button", { name: /retry/i }));
    await waitFor(() =>
      expect((within(dialog).getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(false),
    );
  });
});
