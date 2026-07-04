import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ApiKeyListResponse, ApiKeyRow } from "@/lib/api/api_keys";

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

// SettingsTabs pulls usePathname/Link; the ApiKeysView wrapper only needs the
// dialog + list for this flow, so stub the tab chrome out.
vi.mock("@/components/layout/SettingsTabs", () => ({ default: () => null }));

// The "New API key" trigger ships behind a next/dynamic shim (M101 §5). For
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
  last_used_at: null,
  revoked_at: null,
};
const REVOKED: ApiKeyRow = {
  id: "0190bbbb-bbbb-7bbb-bbbb-bbbbbbbbbbbb",
  key_name: "old-zapier",
  active: false,
  created_at: 1_715_000_000_000,
  last_used_at: 1_715_500_000_000,
  revoked_at: 1_715_900_000_000,
};

function listResponse(items: ApiKeyRow[], total = items.length): ApiKeyListResponse {
  return { items, total, page: 1, page_size: 25 };
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
    render(React.createElement(ApiKeyList, { initial } as never));
  }

  it("renders the empty-state message when there are no keys", async () => {
    await renderList(listResponse([]));
    expect(screen.getByText(/No API keys yet/i)).toBeTruthy();
  });

  it("hides the sort toolbar while the list is empty (only the empty state shows)", async () => {
    await renderList(listResponse([]));
    expect(screen.queryByLabelText(/sort api keys/i)).toBeNull();
    expect(screen.getByText(/No API keys yet/i)).toBeTruthy();
  });

  it("shows the sort toolbar once keys exist", async () => {
    await renderList(listResponse([ACTIVE, REVOKED]));
    expect(screen.getByLabelText(/sort api keys/i)).toBeTruthy();
  });

  it("renders active + revoked rows with status badges and 'never used'", async () => {
    await renderList(listResponse([ACTIVE, REVOKED]));
    expect(screen.getByText("ci-runner")).toBeTruthy();
    expect(screen.getByText("old-zapier")).toBeTruthy();
    expect(screen.getByText("active")).toBeTruthy();
    expect(screen.getByText("revoked")).toBeTruthy();
    expect(screen.getByText(/never used/i)).toBeTruthy();
  });

  it("active rows expose Revoke; revoked rows expose Delete", async () => {
    await renderList(listResponse([ACTIVE, REVOKED]));
    expect(screen.getByLabelText(/Revoke API key ci-runner/i)).toBeTruthy();
    expect(screen.getByLabelText(/Delete API key old-zapier/i)).toBeTruthy();
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
    deleteApiKeyActionMock.mockResolvedValue({ ok: false, error: "revoke first", errorCode: "UZ-APIKEY-008" });
    const user = userEvent.setup();
    await renderList(listResponse([REVOKED]));
    await user.click(screen.getByLabelText(/Delete API key old-zapier/i));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: /^delete$/i }));
    await waitFor(() => expect(screen.getByText(/Revoke this key before deleting it/i)).toBeTruthy());
    await waitFor(() => expect(listApiKeysActionMock).toHaveBeenCalled());
  });

  it("pagination shows when total exceeds the page size and Next re-fetches page 2", async () => {
    const user = userEvent.setup();
    await renderList(listResponse([ACTIVE], 30));
    const next = screen.getByRole("button", { name: /^next$/i });
    await user.click(next);
    await waitFor(() =>
      expect(listApiKeysActionMock).toHaveBeenCalledWith(expect.objectContaining({ page: 2, page_size: 25 })),
    );
  });

  it("Previous re-fetches the prior page", async () => {
    const user = userEvent.setup();
    // Render already on page 2 (Previous enabled, no in-flight transition) so
    // the click can't race a pending-disabled button — deterministic under the
    // slower coverage instrumentation.
    await renderList({ ...listResponse([ACTIVE], 30), page: 2 });
    await user.click(screen.getByRole("button", { name: /^previous$/i }));
    await waitFor(() =>
      expect(listApiKeysActionMock).toHaveBeenCalledWith(expect.objectContaining({ page: 1 })),
    );
  });

  it("picking a different sort re-fetches page 1 with that sort", async () => {
    await renderList(listResponse([ACTIVE], 30));
    const trigger = screen.getByLabelText(/sort api keys/i);
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    fireEvent.click(screen.getByText("Name A–Z"));
    await waitFor(() =>
      expect(listApiKeysActionMock).toHaveBeenCalledWith(expect.objectContaining({ sort: "key_name", page: 1 })),
    );
  });

  it("resets to defaults at most once on UZ-REQ-001 (no infinite retry loop)", async () => {
    // Backend rejects every request, including the defaults the reset falls back
    // to — the `retried` guard must stop after one reset, not loop forever.
    listApiKeysActionMock.mockResolvedValue({ ok: false, error: "invalid sort", errorCode: "UZ-REQ-001" });
    await renderList(listResponse([ACTIVE], 30));
    const trigger = screen.getByLabelText(/sort api keys/i);
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    fireEvent.click(screen.getByText("Name A–Z"));
    // Original load + exactly one defaults-reset = 2 calls; never a third.
    await waitFor(() => expect(listApiKeysActionMock.mock.calls.length).toBe(2));
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(listApiKeysActionMock.mock.calls.length).toBe(2);
  });

  it("re-fetches the first page after a key is minted and the reveal is closed", async () => {
    const user = userEvent.setup();
    const { default: ApiKeysView } = await import(
      "../app/(dashboard)/settings/api-keys/components/ApiKeysView"
    );
    render(React.createElement(ApiKeysView, { initial: listResponse([ACTIVE]) } as never));
    createApiKeyActionMock.mockResolvedValue({
      ok: true,
      data: { id: "k", key_name: "ci-runner", key: "agt_tnew", created_at: 1 },
    });
    await user.click(screen.getByRole("button", { name: /new api key/i }));
    await user.type(screen.getByLabelText(/^name$/i), "ci-runner");
    await user.click(screen.getByRole("button", { name: /create key/i }));
    await screen.findByLabelText(/API key value/i);
    const before = listApiKeysActionMock.mock.calls.length;
    await user.click(screen.getByRole("button", { name: /done/i }));
    await waitFor(() =>
      expect(listApiKeysActionMock).toHaveBeenCalledWith(expect.objectContaining({ page: 1, sort: "-created_at" })),
    );
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

  it("surfaces a non-validation load error inline without resetting", async () => {
    const user = userEvent.setup();
    await renderList(listResponse([ACTIVE], 30));
    listApiKeysActionMock.mockResolvedValueOnce({ ok: false, error: "boom", errorCode: "UZ-INTERNAL-003" });
    await user.click(screen.getByRole("button", { name: /^next$/i }));
    await screen.findByText(/couldn't load api keys/i);
    // No UZ-REQ-001 reset loop: exactly one load fired by the click.
    expect(listApiKeysActionMock).toHaveBeenCalledWith(expect.objectContaining({ page: 2 }));
  });

  it("an invalid sort/page response (UZ-REQ-001) resets to the default sort + page 1", async () => {
    listApiKeysActionMock.mockResolvedValueOnce({ ok: false, error: "bad sort", errorCode: "UZ-REQ-001" });
    const user = userEvent.setup();
    await renderList(listResponse([ACTIVE], 30));
    await user.click(screen.getByRole("button", { name: /^next$/i }));
    await waitFor(() =>
      expect(listApiKeysActionMock).toHaveBeenCalledWith(
        expect.objectContaining({ page: 1, sort: "-created_at" }),
      ),
    );
  });

  it("never sends a page_size above the backend max (always the fixed default 25)", async () => {
    const user = userEvent.setup();
    await renderList(listResponse([ACTIVE], 30));
    await user.click(screen.getByRole("button", { name: /^next$/i }));
    await waitFor(() => expect(listApiKeysActionMock).toHaveBeenCalled());
    for (const call of listApiKeysActionMock.mock.calls) {
      expect(call[0].page_size).toBeLessThanOrEqual(100);
    }
  });
});
