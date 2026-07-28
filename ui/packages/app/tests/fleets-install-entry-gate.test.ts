import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderToStaticMarkup } from "react-dom/server";
import { routerPush, routerRefresh, resetCommonMocks } from "./helpers/dashboard-mocks";
import { INSTALL_STEP } from "@/lib/streaming/install-steps";

const { useFleetEventStreamMock } = vi.hoisted(() => ({ useFleetEventStreamMock: vi.fn() }));

vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@/components/domain/useFleetEventStream", () => ({
  useFleetEventStream: useFleetEventStreamMock,
}));
const { readFleetLibraryPageActionMock } = vi.hoisted(() => ({
  readFleetLibraryPageActionMock: vi.fn(),
}));
vi.mock("../app/(dashboard)/w/[workspaceId]/fleets/new/actions", () => ({
  readFleetLibraryPageAction: readFleetLibraryPageActionMock,
}));

import { LIBRARY_ERROR_KIND } from "@/lib/api/library-types";
import { FleetInstallGate } from "../app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetInstallGate";
import { InstallSourceSelector } from "../app/(dashboard)/w/[workspaceId]/fleets/new/InstallSourceSelector";

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

function stubStream(installStep: string | null) {
  useFleetEventStreamMock.mockReturnValue({
    events: [],
    connectionStatus: "live",
    isRunning: false,
    installStep,
    appendOptimistic: vi.fn(),
    reconcileOptimistic: vi.fn(),
    markOptimisticFailed: vi.fn(),
    discardOptimistic: vi.fn(),
    convertEvent: vi.fn(),
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks();
  stubStream(null);
});
afterEach(() => cleanup());

// ── InstallSourceSelector — full install page library-entry picker ──────────

describe("InstallSourceSelector", () => {
  it("renders Create-fleet-library in the populated gallery when library:write is available", async () => {
    const onUseLibraryEntry = vi.fn();
    const user = userEvent.setup({ delay: null });
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [TEMPLATE], next_cursor: null, total: 1 },
        initialError: null,
        onUseLibraryEntry,
        canAddLibraryEntry: true,
      }),
    );

    expect(screen.getByRole("button", { name: "Create fleet library" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Use entry" }));
    expect(onUseLibraryEntry).toHaveBeenCalledWith(TEMPLATE);
  });

  it("renders the empty selector without Create-fleet-library when library:write is absent", () => {
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [], next_cursor: null, total: 0 },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
        canAddLibraryEntry: false,
      }),
    );

    expect(screen.getByText("No prebuilt fleet library found")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Create fleet library" })).toBeNull();
    expect(screen.getByRole("link", { name: "Learn more" })).toBeTruthy();
  });

  it("defaults the selector to no Create-fleet-library access", () => {
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [], next_cursor: null, total: 0 },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
      }),
    );

    expect(screen.getByText("No prebuilt fleet library found")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Create fleet library" })).toBeNull();
  });

  it("renders Create-fleet-library in the empty selector when library:write is available", () => {
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [], next_cursor: null, total: 0 },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
        canAddLibraryEntry: true,
      }),
    );

    expect(screen.getByText("No prebuilt fleet library found")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Create fleet library" })).toBeTruthy();
  });

  it("drops the needs badges for a credential-less entry", () => {
    // LibraryCard's no-badge branch, exercised through its one consumer.
    const noCreds = {
      ...TEMPLATE,
      id: "no-creds",
      name: "No-credential fleet",
      requirements: { ...TEMPLATE.requirements, credentials: [] as string[] },
    };
    const m = renderToStaticMarkup(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        initialPage: { items: [noCreds], next_cursor: null, total: 1 },
        initialError: null,
        onUseLibraryEntry: vi.fn(),
      }),
    );
    expect(m).toContain("No-credential fleet");
    expect(m).not.toContain("needs:");
  });
});

// ── FleetInstallGate — installing fleets show states first, then the page ────

describe("FleetInstallGate", () => {
  // Children passed positionally to createElement (the canonical form — no
  // `children` prop key). The cast loosens the required-children overload so a
  // `.ts` test file (the lane glob is `tests/*.test.ts`) needs no JSX.
  const Gate = FleetInstallGate as unknown as React.FunctionComponent<{
    workspaceId: string;
    fleetId: string;
    fleetName: string;
    status: string;
  }>;
  function renderGate(status: string) {
    return render(
      React.createElement(
        Gate,
        { workspaceId: "ws_1", fleetId: "zom_1", fleetName: "fresh-bot", status },
        React.createElement("div", { "data-testid": "fleet-body" }, "the full fleet surface"),
      ),
    );
  }

  it("renders the children directly when the fleet is not installing", () => {
    renderGate("active");
    expect(screen.getByTestId("fleet-body")).toBeTruthy();
    expect(screen.queryByLabelText("Install states")).toBeNull();
  });

  it("shows the install states (gating the body) while the fleet is installing", () => {
    stubStream(INSTALL_STEP.PROVISIONING);
    renderGate("installing");
    expect(screen.getByLabelText("Install states")).toBeTruthy();
    expect(screen.queryByTestId("fleet-body")).toBeNull();
  });

  it("on install:ready the gate surfaces Open fleet, which refreshes server data in place", async () => {
    stubStream(INSTALL_STEP.READY);
    const user = userEvent.setup({ delay: null });
    renderGate("installing");
    // The gate stays on the install surface; InstallStreamSteps shows Open fleet
    // on ready, which refreshes (resolves the now-active fleet in place).
    await user.click(screen.getByRole("button", { name: /open fleet/i }));
    expect(routerRefresh).toHaveBeenCalledTimes(1);
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("Back from the installing gate routes to the fleets list", async () => {
    stubStream(INSTALL_STEP.CREATING);
    const user = userEvent.setup({ delay: null });
    renderGate("installing");
    await user.click(screen.getByRole("button", { name: /back to library/i }));
    expect(routerPush).toHaveBeenCalledWith("/w/ws_1/fleets");
  });
});

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
    expect(screen.getByRole("alert")).toBeTruthy();
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
    expect(screen.getByText("The fleet library is temporarily unavailable.")).toBeTruthy();
    expect(screen.queryByText("No prebuilt fleet library found")).toBeNull();

    const retry = screen.getByRole("button", { name: "Retry" }) as HTMLButtonElement;
    expect(retry.disabled).toBe(false);
    await user.click(retry);

    // First page, no cursor — the read that actually failed.
    expect(readFleetLibraryPageActionMock).toHaveBeenCalledWith("ws_1", null);
    expect(await screen.findByText("GitHub PR reviewer")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
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
