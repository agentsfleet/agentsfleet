import { describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MODEL_REGISTRY_HEADER_ORDER, listModelEntriesActionMock, setProviderSelfManagedActionMock, resetProviderActionMock, entry, registry, renderTable, PLATFORM_LOCK } from "./helpers/models-registry-fixtures";

describe("Models registry actions", () => {
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
    expect(within(rows[1]!).getByLabelText(PLATFORM_LOCK)).toBeTruthy();

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /^model$/i }));
    const afterSort = screen.getAllByRole("row");
    expect(within(afterSort[1]!).getByLabelText(PLATFORM_LOCK)).toBeTruthy();
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
    expect(screen.queryByLabelText(PLATFORM_LOCK)).toBeNull();
    expect(screen.queryByText("No default is configured.")).toBeNull();
    expect(screen.queryByRole("button", { name: /use default/i })).toBeNull();
  });

  it("keeps the platform row, disabled with explanatory copy, when a default exists but is not in effect", async () => {
    await renderTable(registry([entry({ active: true })], true));
    expect(screen.getByLabelText(PLATFORM_LOCK)).toBeTruthy();
    expect(screen.getByRole("button", { name: /use default/i })).toBeTruthy();
  });

  it("warns instead of claiming Active when nothing is active and no platform default exists", async () => {
    // The regression this pins: isDefaultLive once tested only "no active
    // entry", so a fresh tenant on a fresh install (model.platform_models ships
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
    expect(within(rows[1]!).getByLabelText(PLATFORM_LOCK)).toBeTruthy();
    // Ascending: "" (no provider) sorts before named providers.
    expect(within(within(rows[2]!).getAllByRole("cell")[0]!).getByText("Unknown")).toBeTruthy();

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
    expect(screen.getByTitle("claude-sonnet-5")).toBeTruthy();
  });

  it("View details opens the read-only dialog straight from the inline icon button", async () => {
    await renderTable(registry([entry({})]));
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /view details for claude-sonnet-5/i }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("heading", { name: "Sonnet 5" })).toBeTruthy();
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

});
