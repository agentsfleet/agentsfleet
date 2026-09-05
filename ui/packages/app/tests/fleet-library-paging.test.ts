import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetCommonMocks } from "./helpers/dashboard-mocks";
import { LIBRARY_ERROR_KIND } from "@/lib/api/library-types";
import { galleryErrorCopy, InstallSourceSelector, mirrorCursorIntoUrl } from "../app/(dashboard)/w/[workspaceId]/fleets/new/InstallSourceSelector";

vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
const { readFleetLibraryPageActionMock } = vi.hoisted(() => ({ readFleetLibraryPageActionMock: vi.fn() }));
vi.mock("../app/(dashboard)/w/[workspaceId]/fleets/new/actions", () => ({ readFleetLibraryPageAction: readFleetLibraryPageActionMock }));

beforeEach(() => { vi.clearAllMocks(); resetCommonMocks(); });
afterEach(() => cleanup());

const TEMPLATE = {
  id: "github-pr-reviewer",
  name: "GitHub PR reviewer",
  description: "Reviews pull requests.",
  visibility: "platform" as const,
  source_ref: "platform/github-pr-reviewer",
  requirements: {
    credentials: ["github"],
    tools: [],
    network_hosts: [],
    trigger_present: true,
  },
  required_credentials_reasons: { github: "review your pull requests" },
};

describe("InstallSourceSelector — paging and list position", () => {
  const SECOND = { ...TEMPLATE, id: "second-entry", name: "Second entry" };

  beforeEach(() => {
    readFleetLibraryPageActionMock.mockReset();
    window.history.replaceState({}, "", "/w/ws_1/fleets/new");
  });

  it("test_fleet_gallery_paging_discloses_remaining — states what it has not loaded", () => {
    // Invariant 5. The exhaustive walk this replaced guaranteed every entry was
    // present; paging cannot, so the remainder is stated outright.
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [TEMPLATE], next_cursor: "cur-2", total: 7 },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
      }),
    );
    expect(screen.getByText("Showing 1 of 7 entries")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Load more" })).toBeTruthy();
  });

  it("discloses an unnamed remainder when the server sends no total — the branch production actually hits", () => {
    // Every daemon endpoint currently emits total: null (counting a keyset
    // page costs the scan it avoids), so THIS wording is the one users see;
    // the named-total case above pins the wire shape OpenAPI permits.
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [TEMPLATE], next_cursor: "cur-2", total: null },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
      }),
    );
    expect(screen.getByText("Showing 1 entries — more available")).toBeTruthy();
  });

  it("treats a cursor that does not advance as the last page instead of appending forever", async () => {
    // The exhaustive walk this replaced THREW when the server stopped
    // advancing its cursor ("one page repeated is worse than an error");
    // load-more must not reintroduce that as infinite duplicate appends.
    const user = userEvent.setup({ delay: null });
    readFleetLibraryPageActionMock.mockResolvedValue({
      ok: true,
      data: { items: [SECOND], next_cursor: "cur-2", total: null },
    });

    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [TEMPLATE], next_cursor: "cur-2", total: null },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
      }),
    );

    await user.click(screen.getByRole("button", { name: "Load more" }));
    expect(await screen.findByText("Second entry")).toBeTruthy();
    // Terminal: the affordance retires rather than offering the same page again.
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
  });

  it("a rejected action round-trip surfaces the failure instead of escaping the transition", async () => {
    // `withToken` catches server-side; the POST to the action endpoint itself
    // can still reject (network failure, deploy skew). Uncaught, that escapes
    // into a route with no error boundary.
    const user = userEvent.setup({ delay: null });
    readFleetLibraryPageActionMock.mockRejectedValue(new Error("network down"));

    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [TEMPLATE], next_cursor: "cur-2", total: null },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
      }),
    );

    await user.click(screen.getByRole("button", { name: "Load more" }));
    expect(await screen.findByRole("alert")).toBeTruthy();
    // Cards retained; the failure is a failure, not an empty library.
    expect(screen.getByText("GitHub PR reviewer")).toBeTruthy();
  });

  it("a thrown non-Error still surfaces the failure, with no fabricated detail", async () => {
    // `detail` is the thrown message, and only an Error carries one. A rejected
    // string — a deploy-skewed action, a framework-level throw — must still
    // reach the typed failure state rather than render `undefined` at the user
    // as though it were the reason.
    const user = userEvent.setup({ delay: null });
    readFleetLibraryPageActionMock.mockRejectedValue("bare string, not an Error");

    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [TEMPLATE], next_cursor: "cur-2", total: null },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
      }),
    );

    await user.click(screen.getByRole("button", { name: "Load more" }));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.queryByText(/bare string, not an Error/)).toBeNull();
    expect(screen.getByText("GitHub PR reviewer")).toBeTruthy();
  });

  it("mirroring the cursor is a no-op where there is no window to mirror into", () => {
    // The module is evaluated during the server render even though the click
    // that calls this cannot happen there. Touching `window.location` under
    // those conditions is a ReferenceError that takes the whole route down, so
    // the guard returns before the read rather than after it.
    //
    // Assigning `undefined` is what makes `typeof window` report "undefined":
    // happy-dom always defines the binding, so removing the VALUE is the only
    // way to reproduce a server render inside a browser environment.
    const before = window.location.href;
    vi.stubGlobal("window", undefined);
    expect(() => mirrorCursorIntoUrl("cur-2")).not.toThrow();
    vi.unstubAllGlobals();
    expect(window.location.href).toBe(before);
  });

  it("galleryErrorCopy gives each failure kind its own next step", async () => {
    const { LIBRARY_ERROR_KIND: kinds } = await import("@/lib/api/library-types");
    const copies = Object.values(kinds).map((kind) => galleryErrorCopy({ kind }));
    expect(new Set(copies).size).toBe(copies.length);
    expect(galleryErrorCopy({ kind: kinds.unauthenticated })).toMatch(/sign in/i);
    expect(galleryErrorCopy({ kind: kinds.unavailable })).toMatch(/temporarily unavailable/i);
  });

  it("test_fleet_load_more_then_selected_summary — appends and retains prior cards, selection needs no request", async () => {
    const user = userEvent.setup({ delay: null });
    readFleetLibraryPageActionMock.mockResolvedValue({
      ok: true,
      data: { items: [SECOND], next_cursor: null, total: 2 },
    });

    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [TEMPLATE], next_cursor: "cur-2", total: 2 },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
      }),
    );

    await user.click(screen.getByRole("button", { name: "Load more" }));

    // Exactly one request, carrying the cursor the first page returned.
    expect(readFleetLibraryPageActionMock).toHaveBeenCalledTimes(1);
    expect(readFleetLibraryPageActionMock).toHaveBeenCalledWith("ws_1", "cur-2");
    // Prior cards RETAINED, not replaced.
    expect(screen.getByText("GitHub PR reviewer")).toBeTruthy();
    expect(screen.getByText("Second entry")).toBeTruthy();
    // Position mirrored so a reload lands here, and REPLACED so Back still
    // leaves the screen rather than walking back one page-load at a time.
    expect(window.location.search).toContain("library_after=cur-2");
    // Last page reached — the affordance and its disclosure both retire.
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
  });

  it("test_refresh_retains_authorized_content — a failed page keeps prior cards and offers retry", async () => {
    const user = userEvent.setup({ delay: null });
    readFleetLibraryPageActionMock.mockResolvedValue({ ok: false, error: "upstream 503" });

    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [TEMPLATE], next_cursor: "cur-2", total: 7 },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
      }),
    );

    await user.click(screen.getByRole("button", { name: "Load more" }));

    // A failed page never blanks the gallery.
    expect(screen.getByText("GitHub PR reviewer")).toBeTruthy();
    // `findByRole`, not `getByRole`: the banner appears only after the failed
    // page settles and React re-renders, which is a tick later than the click
    // promise resolves. The synchronous query asserted on the instant before
    // that render and won the race whenever the file ran alone — and lost it
    // under the full suite, which is what made this test flaky rather than
    // wrong. Awaiting the FIRST element of the banner is enough; `Retry`
    // arrives in the same render, so it stays a synchronous sibling assertion.
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
    // And it is a failure, not an empty library.
    expect(screen.queryByText("No prebuilt fleet library found")).toBeNull();
  });

  it("shows the not-found selection state without erroring the page", () => {
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [TEMPLATE], next_cursor: null, total: 1 },
        initialError: null,
        selectionNotFound: true,
        onUseLibraryEntry: vi.fn(),
      }),
    );
    expect(screen.getByText(/not on this page/)).toBeTruthy();
    // The gallery still works — a bad link lands somewhere useful.
    expect(screen.getByText("GitHub PR reviewer")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("a failed initial read leaves Retry live, and it re-reads the FIRST page", async () => {
    // The failure Retry exists for: the server-render read failed, so there is
    // no page and no cursor. Retry must not depend on either — a load-more
    // bound retry is disabled in exactly this state, with no way back short of
    // a browser reload.
    const user = userEvent.setup({ delay: null });
    readFleetLibraryPageActionMock.mockResolvedValue({
      ok: true,
      data: { items: [TEMPLATE], next_cursor: null, total: 1 },
    });

    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: null,
        initialError: { kind: LIBRARY_ERROR_KIND.unavailable },
        onUseLibraryEntry: vi.fn(),
      }),
    );

    // The typed failure renders its own copy, and is not the empty state.
    expect(await screen.findByText("The fleet library is temporarily unavailable.")).toBeTruthy();
    expect(screen.queryByText("No prebuilt fleet library found")).toBeNull();

    const retry = screen.getByRole("button", { name: "Retry" }) as HTMLButtonElement;
    expect(retry.disabled).toBe(false);
    await user.click(retry);

    // First page, no cursor — the read that actually failed.
    expect(readFleetLibraryPageActionMock).toHaveBeenCalledWith("ws_1", null);
    expect(await screen.findByText("GitHub PR reviewer")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("a failed page with a transport status keeps its specific copy", async () => {
    // The action layer preserves `status`; collapsing every client-side
    // failure to "unknown" would show "Could not load" where "temporarily
    // unavailable" (or "sign in") is the actionable instruction.
    const user = userEvent.setup({ delay: null });
    readFleetLibraryPageActionMock.mockResolvedValue({ ok: false, error: "boom", status: 503 });

    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [TEMPLATE], next_cursor: "cur-2", total: 2 },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
      }),
    );

    await user.click(screen.getByRole("button", { name: "Load more" }));
    expect(await screen.findByText("The fleet library is temporarily unavailable.")).toBeTruthy();
  });

  it("Retry after a failed load-more re-requests the SAME page and appends", async () => {
    const user = userEvent.setup({ delay: null });
    readFleetLibraryPageActionMock
      .mockResolvedValueOnce({ ok: false, error: "upstream 503" })
      .mockResolvedValueOnce({ ok: true, data: { items: [SECOND], next_cursor: null, total: 2 } });

    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [TEMPLATE], next_cursor: "cur-2", total: 2 },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
      }),
    );

    await user.click(screen.getByRole("button", { name: "Load more" }));
    expect(screen.getByRole("alert")).toBeTruthy();

    // The failed transition's `pending` can settle a commit after the alert
    // appears; the click must wait for the button to be live again.
    const retry = screen.getByRole("button", { name: "Retry" }) as HTMLButtonElement;
    await waitFor(() => expect(retry.disabled).toBe(false));
    await user.click(retry);

    // Same cursor both times: the retry re-reads the page that failed rather
    // than restarting the walk, so nothing already on screen is lost.
    expect(readFleetLibraryPageActionMock).toHaveBeenNthCalledWith(1, "ws_1", "cur-2");
    expect(readFleetLibraryPageActionMock).toHaveBeenNthCalledWith(2, "ws_1", "cur-2");
    expect(await screen.findByText("Second entry")).toBeTruthy();
    expect(screen.getByText("GitHub PR reviewer")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
