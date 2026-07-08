import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { TenantModelEntry, TenantModelEntryList } from "@/lib/types";

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

function registry(models: TenantModelEntry[], platformDefaultAvailable = true): TenantModelEntryList {
  return { models, platform_default_available: platformDefaultAvailable };
}

async function renderTable(initial: TenantModelEntryList) {
  const { default: ModelsRegistryTable } = await import(
    "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
  );
  render(React.createElement(ModelsRegistryTable, { workspaceId: "ws_1", initial, initialSecrets: [] } as never));
}

beforeEach(() => {
  vi.clearAllMocks();
  listSecretsActionMock.mockResolvedValue({ ok: true, data: { secrets: [] } });
});
afterEach(() => cleanup());

describe("ModelsRegistryTable", () => {
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
    await user.click(screen.getByRole("button", { name: /^switch$/i }));
    expect(setProviderSelfManagedActionMock).toHaveBeenCalledWith({ secret_ref: "anthropic-prod", model: "claude-sonnet-5" });
    expect(screen.queryByLabelText(/api key/i)).toBeNull();
  });

  it("Default row's Use-default is disabled with explanatory copy when no platform default exists", async () => {
    await renderTable(registry([entry({ active: true })], false));
    const useDefault = screen.getByRole("button", { name: /use default/i });
    expect(useDefault.hasAttribute("disabled")).toBe(true);
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
    await user.click(screen.getByRole("button", { name: /^switch$/i }));
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
    await user.click(screen.getByRole("button", { name: /^switch$/i }));
    await waitFor(() => expect(listModelEntriesActionMock).toHaveBeenCalled());
    expect(screen.getByText("claude-sonnet-5")).toBeTruthy();
  });

  it("View details opens the read-only dialog for the clicked row", async () => {
    await renderTable(registry([entry({})]));
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /row actions for claude-sonnet-5/i }));
    await user.click(screen.getByRole("menuitem", { name: /view details/i }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("heading", { name: "claude-sonnet-5" })).toBeTruthy();
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

  it("computeNextSort ignores a key outside the sortable column set, and toggles both directions", async () => {
    const { computeNextSort } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
    );
    expect(computeNextSort(null, "status")).toBeNull();
    expect(computeNextSort({ key: "model", dir: "ascending" }, "actions")).toBeNull();
    expect(computeNextSort({ key: "model", dir: "ascending" }, "model")).toEqual({ key: "model", dir: "descending" });
    expect(computeNextSort({ key: "model", dir: "descending" }, "model")).toEqual({ key: "model", dir: "ascending" });
  });

  it("sortValueFor reads model_id for the model column and provider (or '') for the provider column", async () => {
    const { sortValueFor } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
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

  it("renders '—' when the context cap is absent (undefined)", async () => {
    await renderTable(registry([entry({ id: "e1", model_id: "m1", context_cap_tokens: undefined })]));
    const rows = screen.getAllByRole("row");
    expect(within(rows[2]!).getByText("—")).toBeTruthy();
  });

  it("creating a model entry refreshes the secrets list — a repeat add on the same key name rotates instead of re-creating", async () => {
    createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    rotateSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: 1 } });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ id: "e1", secret_ref: "anthropic" })]) });
    listSecretsActionMock.mockResolvedValueOnce({
      ok: true,
      data: { secrets: [{ kind: "provider_key", name: "anthropic", provider: "anthropic", created_at: 1 }] },
    });
    await renderTable(registry([]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /add model/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(listSecretsActionMock).toHaveBeenCalledWith("ws_1"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // Second add with the same auto-detected key name ("anthropic") — the
    // refreshed secrets state now carries that name, so the dialog rotates
    // the stored key in place instead of re-creating it. This is the
    // observable proof the refreshSecrets round-trip landed in state.
    await user.click(screen.getByRole("button", { name: /add model/i }));
    const reopened = await screen.findByRole("dialog");
    await user.type(within(reopened).getByLabelText(/^api key$/i), "sk-ant-second-key");
    await user.click(within(reopened).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.click(within(reopened).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(rotateSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic", "sk-ant-second-key"));
    expect(createSecretActionMock).toHaveBeenCalledTimes(1);
  });

  it("a failed secrets refresh leaves the stored-key state as-is — a repeat add re-creates rather than rotating", async () => {
    createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: 1 } });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ id: "e1", secret_ref: "anthropic" })]) });
    listSecretsActionMock.mockResolvedValueOnce({ ok: false, error: "boom", errorCode: "UZ-INTERNAL-003" });
    await renderTable(registry([]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /add model/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    await user.click(within(dialog).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(listSecretsActionMock).toHaveBeenCalledWith("ws_1"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // The `!r.ok` early return kept the secrets state empty, so the same key
    // name is unknown to the dialog and the second add takes the create path
    // again (the backend would 409 in reality; mocked ok here — the branch
    // under test is refreshSecrets' silent no-op, matching refresh()).
    await user.click(screen.getByRole("button", { name: /add model/i }));
    const reopened = await screen.findByRole("dialog");
    await user.type(within(reopened).getByLabelText(/^api key$/i), "sk-ant-second-key");
    await user.click(within(reopened).getByLabelText(/^model$/i));
    await user.click((await screen.findAllByRole("option"))[0]!);
    await user.click(within(reopened).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalledTimes(2));
    expect(rotateSecretActionMock).not.toHaveBeenCalled();
  });
});
