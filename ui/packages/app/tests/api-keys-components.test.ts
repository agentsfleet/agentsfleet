import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { ApiKeyListResponse, ApiKeyRow } from "@/lib/api/api_keys";

// The relative-Time cells in ApiKeyList render Radix tooltips, which require a
// TooltipProvider ancestor (mounted at the dashboard layout in production).
function withTooltipProvider(node: React.ReactElement): React.ReactElement {
  return React.createElement(TooltipProvider, null, node);
}

// ── Shared mocks ───────────────────────────────────────────────────────────

const listApiKeysActionMock = vi.fn();
const createApiKeyActionMock = vi.fn();
const revokeApiKeyActionMock = vi.fn();
const deleteApiKeyActionMock = vi.fn();

vi.mock("@/app/(dashboard)/settings/api-keys/actions", () => ({
  listApiKeysAction: listApiKeysActionMock,
  createApiKeyAction: createApiKeyActionMock,
  revokeApiKeyAction: revokeApiKeyActionMock,
  deleteApiKeyAction: deleteApiKeyActionMock,
}));

// The "Create key" trigger ships behind a next/dynamic shim (M101 §5). For
// these interaction tests, alias the shim back to the real dialog so the
// trigger + form mount synchronously instead of behind the loading skeleton.
vi.mock("@/components/domain/island-dynamic/CreateApiKeyDialogDynamic", async () => ({
  default: (
    await vi.importActual<{ default: unknown }>(
      "@/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog",
    )
  ).default,
}));

const ACTIVE: ApiKeyRow = {
  id: "0190aaaa-aaaa-7aaa-aaaa-aaaaaaaaaaaa",
  key_name: "ci-runner",
  active: true,
  created_at: 1_716_000_000_000,
  revoked_at: null,
};
const REVOKED: ApiKeyRow = {
  id: "0190bbbb-bbbb-7bbb-bbbb-bbbbbbbbbbbb",
  key_name: "old-zapier",
  active: false,
  created_at: 1_715_000_000_000,
  revoked_at: 1_715_900_000_000,
};

function listResponse(items: ApiKeyRow[], total = items.length): ApiKeyListResponse {
  return { items, total, next_cursor: null };
}

beforeEach(() => {
  vi.clearAllMocks();
  listApiKeysActionMock.mockResolvedValue({ ok: true, data: listResponse([ACTIVE, REVOKED]) });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

// ── ApiKeyList ───────────────────────────────────────────────────────────

describe("ApiKeyList component", () => {
  async function renderList(initial: ApiKeyListResponse) {
    const { default: ApiKeyList } = await import(
      "../app/(dashboard)/settings/api-keys/components/ApiKeyList"
    );
    render(withTooltipProvider(React.createElement(ApiKeyList, { initial } as never)));
  }

  it("renders the empty-state message when there are no keys", async () => {
    await renderList(listResponse([]));
    expect(screen.getByText(/Connect your first tool/i)).toBeTruthy();
  });

  it("hides the sortable table while the list is empty (only the empty state shows)", async () => {
    await renderList(listResponse([]));
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.getByText(/Connect your first tool/i)).toBeTruthy();
  });

  it("renders Name and Created as clickable, sortable column headers once keys exist", async () => {
    await renderList(listResponse([ACTIVE, REVOKED]));
    expect(screen.getByRole("button", { name: /^name$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^created$/i })).toBeTruthy();
    expect(screen.getByRole("columnheader", { name: "Name" }).getAttribute("aria-sort")).toBe("none");
    expect(screen.getByRole("columnheader", { name: "Created" }).getAttribute("aria-sort")).toBe("none");
    expect(screen.getByRole("button", { name: /^created$/i }).querySelector(".lucide-chevrons-up-down")).toBeTruthy();
  });

  it("renders active and revoked rows with status badges", async () => {
    await renderList(listResponse([ACTIVE, REVOKED]));
    expect(screen.getByText("ci-runner")).toBeTruthy();
    expect(screen.getByText("old-zapier")).toBeTruthy();
    expect(screen.getByText("Active")).toBeTruthy();
    expect(screen.getByText("Revoked")).toBeTruthy();
    expect(screen.queryByText(/never used/i)).toBeNull();
  });

  it("active rows expose Revoke; revoked rows expose Delete", async () => {
    await renderList(listResponse([ACTIVE, REVOKED]));
    expect(screen.getByLabelText(/Revoke API key ci-runner/i)).toBeTruthy();
    expect(screen.getByLabelText(/Delete API key old-zapier/i)).toBeTruthy();
  });

  it("the icon action buttons disable while a mutation is in flight (disabled-while-pending preserved)", async () => {
    // Hold the mutation open so the transition stays pending; the row's icon
    // buttons must be disabled for the duration, then re-enable on settle.
    let resolveRevoke!: (v: unknown) => void;
    revokeApiKeyActionMock.mockReturnValue(new Promise((r) => { resolveRevoke = r; }));
    listApiKeysActionMock.mockResolvedValue({ ok: true, data: listResponse([REVOKED]) });
    const user = userEvent.setup();
    await renderList(listResponse([ACTIVE]));

    await user.click(screen.getByLabelText(/Revoke API key ci-runner/i));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^revoke$/i }));

    await waitFor(() => expect(revokeApiKeyActionMock).toHaveBeenCalled());
    // Mid-flight: the row action is disabled.
    await waitFor(() =>
      expect(screen.getByLabelText(/Revoke API key ci-runner/i).hasAttribute("disabled")).toBe(true),
    );

    resolveRevoke({ ok: true, data: { id: ACTIVE.id, active: false, revoked_at: 1 } });
    // After settle the list re-fetched (row now revoked → Delete affordance).
    await waitFor(() => expect(screen.getByLabelText(/Delete API key old-zapier/i)).toBeTruthy());
  });

  it("renders rows through the standard DataTable primitive, not a hand-rolled list", async () => {
    // Pins the DataTable migration — a regression back to a bespoke row-list
    // (no `role=table`, no column headers) must fail this test.
    await renderList(listResponse([ACTIVE, REVOKED]));
    expect(screen.getByRole("table")).toBeTruthy();
    expect(screen.getByRole("columnheader", { name: "Name" })).toBeTruthy();
    expect(screen.getByRole("columnheader", { name: "Created" })).toBeTruthy();
    expect(screen.getByRole("columnheader", { name: "Actions" })).toBeTruthy();
  });

  it("revoke happy path: confirm → revokeApiKeyAction(id) → list re-fetched", async () => {
    revokeApiKeyActionMock.mockResolvedValue({ ok: true, data: { id: ACTIVE.id, active: false, revoked_at: 1 } });
    const user = userEvent.setup();
    await renderList(listResponse([ACTIVE]));
    await user.click(screen.getByLabelText(/Revoke API key ci-runner/i));
    await waitFor(() => expect(screen.getByRole("alertdialog")).toBeTruthy());
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^revoke$/i }));
    await waitFor(() => expect(revokeApiKeyActionMock).toHaveBeenCalledWith(ACTIVE.id));
    await waitFor(() => expect(listApiKeysActionMock).toHaveBeenCalled());
  });

  it("revoke race surfaces the already-revoked toast and still re-fetches", async () => {
    revokeApiKeyActionMock.mockResolvedValue({ ok: false, error: "already revoked", errorCode: "UZ-APIKEY-006" });
    const user = userEvent.setup();
    await renderList(listResponse([ACTIVE]));
    await user.click(screen.getByLabelText(/Revoke API key ci-runner/i));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^revoke$/i }));
    await waitFor(() => expect(screen.getByText(/already revoked/i)).toBeTruthy());
    await waitFor(() => expect(listApiKeysActionMock).toHaveBeenCalled());
  });

  it("a successful revoke whose re-fetch fails leaves the existing rows in place", async () => {
    revokeApiKeyActionMock.mockResolvedValue({ ok: true, data: { id: ACTIVE.id, active: false, revoked_at: 1 } });
    // The post-mutation refresh() re-fetch fails → its `if (r.ok)` guard skips
    // apply(), so the rows the user already sees stay put.
    listApiKeysActionMock.mockResolvedValueOnce({ ok: false, error: "refetch failed", errorCode: "UZ-INTERNAL-003" });
    const user = userEvent.setup();
    await renderList(listResponse([ACTIVE]));
    await user.click(screen.getByLabelText(/Revoke API key ci-runner/i));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^revoke$/i }));
    await waitFor(() => expect(revokeApiKeyActionMock).toHaveBeenCalledWith(ACTIVE.id));
    await waitFor(() => expect(screen.getByText("ci-runner")).toBeTruthy());
  });

  it("delete happy path on a revoked row: deleteApiKeyAction(id) → list re-fetched", async () => {
    deleteApiKeyActionMock.mockResolvedValue({ ok: true, data: undefined });
    const user = userEvent.setup();
    await renderList(listResponse([REVOKED]));
    await user.click(screen.getByLabelText(/Delete API key old-zapier/i));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^delete$/i }));
    await waitFor(() => expect(deleteApiKeyActionMock).toHaveBeenCalledWith(REVOKED.id));
    await waitFor(() => expect(listApiKeysActionMock).toHaveBeenCalled());
  });

  it("delete race (key still active) surfaces must-revoke-first and re-fetches", async () => {
    // ApiError.message is user_message ?? detail (client.ts) —
    // UZ-APIKEY-008's friendly copy is its `user_message` in
    // rustd/crates/afd_core/src/problem/auth.rs.
    deleteApiKeyActionMock.mockResolvedValue({
      ok: false,
      error: "Revoke this key before deleting it. Revoke it first, then delete the revoked key.",
      errorCode: "UZ-APIKEY-008",
    });
    const user = userEvent.setup();
    await renderList(listResponse([REVOKED]));
    await user.click(screen.getByLabelText(/Delete API key old-zapier/i));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^delete$/i }));
    await waitFor(() => expect(screen.getByText(/Revoke this key before deleting it/i)).toBeTruthy());
    await waitFor(() => expect(listApiKeysActionMock).toHaveBeenCalled());
  });

  it("uses the shared pagination footer for the complete key list", async () => {
    await renderList(listResponse([ACTIVE, REVOKED]));
    expect(screen.getByRole("button", { name: /next page/i })).toHaveProperty("disabled", true);
    expect(screen.getByRole("button", { name: /previous page/i })).toHaveProperty("disabled", true);
    expect(screen.getByRole("combobox", { name: /rows per page/i })).toBeTruthy();
    expect(screen.getByText("ci-runner")).toBeTruthy();
    expect(screen.getByText("old-zapier")).toBeTruthy();
  });

  /** The key names in the order the table shows them. */
  function shownNames(): string[] {
    return screen
      .getAllByRole("row")
      .slice(1)
      .map((row) => row.querySelector("td")?.textContent ?? "")
      .map((cell) => ["ci-runner", "old-zapier", "twin"].find((name) => cell.includes(name)) ?? cell);
  }

  it("clicking Name sorts the loaded keys at once, with no fetch", async () => {
    await renderList(listResponse([REVOKED, ACTIVE]));
    fireEvent.click(screen.getByRole("button", { name: /^name$/i }));
    expect(shownNames()).toEqual(["ci-runner", "old-zapier"]);
    expect(screen.getByRole("columnheader", { name: "Name" }).getAttribute("aria-sort")).toBe("ascending");

    fireEvent.click(screen.getByRole("button", { name: /^name$/i }));
    expect(shownNames()).toEqual(["old-zapier", "ci-runner"]);
    expect(listApiKeysActionMock).not.toHaveBeenCalled();
  });

  it("clicking Created selects ascending order and toggles in place", async () => {
    await renderList(listResponse([ACTIVE, REVOKED]));
    expect(shownNames()).toEqual(["ci-runner", "old-zapier"]);
    fireEvent.click(screen.getByRole("button", { name: /^created$/i }));
    expect(shownNames()).toEqual(["old-zapier", "ci-runner"]);
    expect(screen.getByRole("columnheader", { name: "Created" }).getAttribute("aria-sort")).toBe("ascending");
    fireEvent.click(screen.getByRole("button", { name: /^created$/i }));
    expect(shownNames()).toEqual(["ci-runner", "old-zapier"]);
    expect(screen.getByRole("columnheader", { name: "Created" }).getAttribute("aria-sort")).toBe("descending");
    expect(listApiKeysActionMock).not.toHaveBeenCalled();
  });

  it("keys created in the same millisecond keep one order in both directions", async () => {
    const twin = { ...ACTIVE, id: "0190cccc-cccc-7ccc-cccc-cccccccccccc", key_name: "twin" };
    await renderList(listResponse([twin, ACTIVE]));
    expect(shownNames()).toEqual(["ci-runner", "twin"]);
    fireEvent.click(screen.getByRole("button", { name: /^created$/i }));
    expect(shownNames()).toEqual(["ci-runner", "twin"]);
  });

  it("a failed reload after a create says so and keeps the list", async () => {
    const { default: ApiKeyList } = await import(
      "../app/(dashboard)/settings/api-keys/components/ApiKeyList"
    );
    const handle = React.createRef<{ refresh: () => void }>();
    render(withTooltipProvider(React.createElement(ApiKeyList, { initial: listResponse([ACTIVE]), ref: handle } as never)));
    listApiKeysActionMock.mockResolvedValueOnce({ ok: false, error: "boom", errorCode: "UZ-INTERNAL-003" });

    React.startTransition(() => handle.current?.refresh());
    await screen.findByText(/couldn't load api keys/i);
    expect(listApiKeysActionMock).toHaveBeenCalledWith("-created_at");
    expect(screen.getByText("ci-runner")).toBeTruthy();
  });

  it("re-fetches the list after a key is minted and the reveal is closed", async () => {
    const user = userEvent.setup();
    const { default: ApiKeysView } = await import(
      "../app/(dashboard)/settings/api-keys/components/ApiKeysView"
    );
    render(
      withTooltipProvider(
        React.createElement(ApiKeysView, {
          initial: listResponse([ACTIVE]),
          operatorOnly: false,
        } as never),
      ),
    );
    createApiKeyActionMock.mockResolvedValue({
      ok: true,
      data: { id: "k", key_name: "ci-runner", key: "agt_tnew", created_at: 1 },
    });
        // The dialog submit is just "Create"; the trigger keeps the noun.
    await user.click(screen.getByRole("button", { name: /create key/i }));
    await user.type(screen.getByLabelText(/^name$/i), "ci-runner");
    // Once open, scope to the dialog to disambiguate the submit from the
    // still-mounted trigger.
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /^create$/i }));
    await screen.findByLabelText(/API key value/i);
    const before = listApiKeysActionMock.mock.calls.length;
    await user.click(screen.getByRole("button", { name: /done/i }));
    await waitFor(() => expect(listApiKeysActionMock).toHaveBeenCalledWith("-created_at"));
    expect(listApiKeysActionMock.mock.calls.length).toBeGreaterThan(before);
  });

  it("dismissing the revoke confirm without confirming clears the pending target", async () => {
    const user = userEvent.setup();
    await renderList(listResponse([ACTIVE, REVOKED]));
    await user.click(screen.getByRole("button", { name: /revoke api key ci-runner/i }));
    await screen.findByRole("alertdialog");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(revokeApiKeyActionMock).not.toHaveBeenCalled();
  });
});
