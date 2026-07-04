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

import { InstallEntry } from "../app/(dashboard)/fleets/new/InstallEntry";
import { FleetInstallGate } from "../app/(dashboard)/fleets/[id]/components/FleetInstallGate";
import { InstallSourceSelector } from "../app/(dashboard)/fleets/new/InstallSourceSelector";

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
  it("renders the template grid with a deep link", () => {
    const m = renderToStaticMarkup(React.createElement(InstallEntry, { templates: [TEMPLATE] }));
    expect(m).toContain('href="/fleets/new?template=github-pr-reviewer"');
    expect(m).toContain("GitHub PR reviewer");
  });

  it("falls back to an empty state with Learn-more + Create-a-template when library:write is available", () => {
    const m = renderToStaticMarkup(
      React.createElement(InstallEntry, { templates: [], canAddTemplate: true }),
    );
    expect(m).toContain("No fleet library yet");
    expect(m).toContain("Write your own template");
    expect(m).toContain("Create a template");
    expect(m).toContain("Learn more");
    expect(m).not.toContain("?template=");
  });

  it("omits Create-a-template (and its copy) when library:write is absent — matches InstallSourceSelector's own gate", () => {
    const m = renderToStaticMarkup(React.createElement(InstallEntry, { templates: [] }));
    expect(m).toContain("No fleet library yet");
    expect(m).toContain("Ask a workspace admin");
    expect(m).not.toContain("Create a template");
    expect(m).toContain("Learn more");
  });

  it("caps the gallery at maxTemplates", () => {
    const many = [TEMPLATE, { ...TEMPLATE, id: "second", name: "Second template" }];
    const m = renderToStaticMarkup(
      React.createElement(InstallEntry, { templates: many, maxTemplates: 1 }),
    );
    expect(m).toContain("GitHub PR reviewer");
    expect(m).not.toContain("Second template");
  });
});

// ── InstallSourceSelector — full install page template picker ───────────────

describe("InstallSourceSelector", () => {
  it("renders Create-a-template in the populated gallery when library:write is available", async () => {
    const onUseTemplate = vi.fn();
    const user = userEvent.setup({ delay: null });
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        templates: [TEMPLATE],
        onUseTemplate,
        canAddTemplate: true,
      }),
    );

    expect(screen.getByRole("button", { name: "Create a template" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Use template" }));
    expect(onUseTemplate).toHaveBeenCalledWith(TEMPLATE);
  });

  it("renders the empty selector without Create-a-template when library:write is absent", () => {
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        templates: [],
        onUseTemplate: vi.fn(),
        canAddTemplate: false,
      }),
    );

    expect(screen.getByText("No fleet library yet")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Create a template" })).toBeNull();
    expect(screen.getByRole("link", { name: "Learn more" })).toBeTruthy();
  });

  it("defaults the selector to no Create-a-template access", () => {
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        templates: [],
        onUseTemplate: vi.fn(),
      }),
    );

    expect(screen.getByText("No fleet library yet")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Create a template" })).toBeNull();
  });

  it("renders Create-a-template in the empty selector when library:write is available", () => {
    render(
      React.createElement(InstallSourceSelector, {
        workspaceId: "ws_1",
        templates: [],
        onUseTemplate: vi.fn(),
        canAddTemplate: true,
      }),
    );

    expect(screen.getByText("No fleet library yet")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Create a template" })).toBeTruthy();
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
    await user.click(screen.getByRole("button", { name: /back to templates/i }));
    expect(routerPush).toHaveBeenCalledWith("/fleets");
  });
});
