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

import { FleetInstallGate } from "../app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetInstallGate";
import {
  InstallSourceSelector,
} from "../app/(dashboard)/w/[workspaceId]/fleets/new/InstallSourceSelector";

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
    await user.click(screen.getByRole("button", { name: "Install" }));
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
