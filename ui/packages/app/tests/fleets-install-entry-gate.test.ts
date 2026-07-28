import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
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

import { InstallEntry } from "../app/(dashboard)/w/[workspaceId]/fleets/new/InstallEntry";
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
  support_files: [],
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

// ── InstallEntry — the shared entry surface (both empty states compose it) ───

describe("InstallEntry", () => {
  it("renders the library-entry grid with a deep link", () => {
    const m = renderToStaticMarkup(React.createElement(InstallEntry, { workspaceId: "ws_1", entries: [TEMPLATE] }));
    // Tier-qualified: a bare ?library=<id> could not tell a platform entry
    // from a tenant entry sharing the id, and the create body keys off tier.
    expect(m).toContain("library_visibility=platform");
    expect(m).toContain("library_id=github-pr-reviewer");
    expect(m).toContain("GitHub PR reviewer");
  });

  it("renders compactly and drops the credential badges for a no-credential entry", () => {
    // The compact grid + a credential-less entry cover LibraryCard's compact and
    // no-badge branches (formerly exercised by the removed dashboard FirstInstall).
    const noCreds = {
      ...TEMPLATE,
      id: "no-creds",
      name: "No-credential fleet",
      requirements: { ...TEMPLATE.requirements, credentials: [] as string[] },
    };
    const m = renderToStaticMarkup(
      React.createElement(InstallEntry, { workspaceId: "ws_1", entries: [noCreds], compact: true }),
    );
    expect(m).toContain("No-credential fleet");
    // No credential requirement → no "needs" badge rendered.
    expect(m).not.toContain("github");
  });

  it("falls back to an empty state with Learn-more + Create-fleet-library when library:write is available", () => {
    const m = renderToStaticMarkup(
      React.createElement(InstallEntry, { workspaceId: "ws_1", entries: [], canAddLibraryEntry: true }),
    );
    expect(m).toContain("No prebuilt fleet library found");
    expect(m).toContain("Write your own fleet library");
    expect(m).toContain("Create fleet library");
    expect(m).toContain("Learn more");
    expect(m).not.toContain("library_id=");
  });

  it("omits Create-fleet-library (and its copy) when library:write is absent — matches InstallSourceSelector's own gate", () => {
    const m = renderToStaticMarkup(React.createElement(InstallEntry, { workspaceId: "ws_1", entries: [] }));
    expect(m).toContain("No prebuilt fleet library found");
    expect(m).toContain("Ask a workspace admin");
    expect(m).not.toContain("Create fleet library");
    expect(m).toContain("Learn more");
  });

  it("caps the gallery at maxEntries", () => {
    const many = [TEMPLATE, { ...TEMPLATE, id: "second", name: "Second template" }];
    const m = renderToStaticMarkup(
      React.createElement(InstallEntry, { workspaceId: "ws_1", entries: many, maxEntries: 1 }),
    );
    expect(m).toContain("GitHub PR reviewer");
    expect(m).not.toContain("Second template");
  });
});

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

  it("discloses what it has not loaded rather than implying it with a button", () => {
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

  it("appends the next page, retaining prior cards, and mirrors position into the URL", async () => {
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

  it("keeps loaded cards on screen when load-more fails, and offers retry", async () => {
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
});
