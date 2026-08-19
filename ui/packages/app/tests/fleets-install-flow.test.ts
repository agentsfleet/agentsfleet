import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetCommonMocks } from "./helpers/dashboard-mocks";
import type { FleetLibraryGalleryEntry } from "@/lib/types";

// The template-only install flow's boundaries are the install server action,
// analytics, and the SSE hook (post-create). Mock those; render the real source
// selector + states so picking a template proceeds inline to the live states
// (no review page) and creates with the visibility-keyed body. M103 removed the
// github-import and paste sources — templates are the only install surface.
const {
  installFleetActionMock,
  captureProductEventMock,
  useFleetEventStreamMock,
} = vi.hoisted(() => ({
  installFleetActionMock: vi.fn(),
  captureProductEventMock: vi.fn(),
  useFleetEventStreamMock: vi.fn(),
}));

vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({
  installFleetAction: installFleetActionMock,
}));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: captureProductEventMock }));
vi.mock("@/components/domain/useFleetEventStream", () => ({
  useFleetEventStream: useFleetEventStreamMock,
}));

import { InstallFleet } from "../app/(dashboard)/w/[workspaceId]/fleets/new/InstallFleet";

// A platform gallery entry (installs by slug) and a tenant one (installs by
// UUID). Mirrors GET /v1/workspaces/{ws}/fleet-libraries.
const TEMPLATE_GH: FleetLibraryGalleryEntry = {
  id: "github-pr-reviewer",
  name: "GitHub PR reviewer",
  description: "Reviews pull requests.",
  visibility: "platform",
  source_ref: "platform/github-pr-reviewer",
  requirements: {
    credentials: ["github"],
    tools: ["http_request"],
    network_hosts: ["api.github.com"],
    trigger_present: true,
  },
  required_credentials_reasons: { github: "review your pull requests" },
};
const TEMPLATE_TENANT: FleetLibraryGalleryEntry = {
  id: "01932d4e-7c10-7a3a-9f00-000000000001",
  name: "Internal ops",
  description: "Tenant-authored ops fleet.",
  visibility: "tenant",
  source_ref: "tenant/01932d4e",
  requirements: { credentials: [], tools: [], network_hosts: [], trigger_present: true },
  required_credentials_reasons: {},
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

type FlowProps = {
  entries?: FleetLibraryGalleryEntry[];
  presentCredentialNames?: string[] | null;
  initialLibraryId?: string;
};

function renderFlow(props: FlowProps = {}) {
  return render(
    React.createElement(InstallFleet, {
      workspaceId: "ws_1",
      initialPage: {
        items: props.entries ?? [TEMPLATE_GH, TEMPLATE_TENANT],
        next_cursor: null,
        total: null,
      },
      initialError: null,
      // Deep-link selection is resolved on the SERVER now, so the flow test
      // hands over the already-matched entry instead of an id to match.
      initialSelection:
        (props.entries ?? [TEMPLATE_GH, TEMPLATE_TENANT]).find(
          (e: { id: string }) => e.id === props.initialLibraryId,
        ) ?? null,
      presentCredentialNames:
        props.presentCredentialNames === undefined ? [] : props.presentCredentialNames,
    }),
  );
}

function useEntryButton(index: number): HTMLElement {
  const button = screen.getAllByRole("button", { name: "Install" })[index];
  if (!button) throw new Error(`no "Install" button at index ${index}`);
  return button;
}

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks();
  stubStream(null);
});
afterEach(() => cleanup());

// ── 9.1: the template gallery renders ───────────────────────────────────────

describe("test_install_template_gallery_render", () => {
  it("renders the template grid with one Use template button per template", () => {
    renderFlow();
    expect(screen.getByText("Fleet library")).toBeTruthy();
    expect(screen.getByText("GitHub PR reviewer")).toBeTruthy();
    expect(screen.getByText("needs: github")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Install" }).length).toBe(2);
  });

  it("shows an empty state when no library entries are available", () => {
    renderFlow({ entries: [] });
    expect(screen.getByText("No prebuilt fleet library found")).toBeTruthy();
  });
});

// ── 9.3: one click installs — the states follow INLINE (no confirm page) ─────

describe("test_install_inline_state_driven", () => {
  it("Install fires create with the platform body — one step, no confirm", async () => {
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_new", name: "github-pr-reviewer" } });
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: ["github"] });

    await user.click(useEntryButton(0));

    // Inline states — no confirm step, no retired review page, no name field.
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
    expect(screen.queryByText("Review what it needs")).toBeNull();
    expect(screen.queryByLabelText("Fleet name")).toBeNull();
    await waitFor(() =>
      expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", {
        platform_library_id: "github-pr-reviewer",
      }),
    );
  });

  it("a tenant template installs with the tenant body", async () => {
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_tenant", name: "platform-ops" } });
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: [] });

    await user.click(useEntryButton(1)); // TEMPLATE_TENANT
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
    await waitFor(() =>
      expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", {
        tenant_library_id: "01932d4e-7c10-7a3a-9f00-000000000001",
      }),
    );
  });

  it("renders the server-chosen name, so an auto-suffixed install reads honestly", async () => {
    // Two installs of one template: the server suffixes the second
    // (`{template}-NNN`) rather than 409ing. The UI must show the name the
    // server actually persisted, never the template's own.
    installFleetActionMock.mockResolvedValue({
      ok: true,
      data: { fleet_id: "zom_suffixed", name: "github-pr-reviewer-042" },
    });
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: ["github"] });

    await user.click(useEntryButton(0));
    await waitFor(() => expect(screen.getByText(/github-pr-reviewer-042/)).toBeTruthy());
  });

  it("renders a DownloadIcon on the gallery card Install button (test_install_renders_icon)", () => {
    renderFlow({ presentCredentialNames: ["github"] });
    expect(useEntryButton(0).querySelector("svg.lucide-download")).toBeTruthy();
  });

  it("preselects a library entry from a ?library= deep link and lands straight in the states", async () => {
    installFleetActionMock.mockReturnValue(new Promise(() => {}));
    renderFlow({ initialLibraryId: "github-pr-reviewer", presentCredentialNames: ["github"] });
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
  });

  it("ignores a ?library= deep link that matches no library entry", () => {
    renderFlow({ initialLibraryId: "does-not-exist" });
    expect(screen.getByText("Fleet library")).toBeTruthy();
  });

  it("Back from the states returns to the selector", async () => {
    installFleetActionMock.mockReturnValue(new Promise(() => {}));
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: ["github"] });
    await user.click(useEntryButton(0));
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /Back to library/ }));
    expect(screen.getByText("Fleet library")).toBeTruthy();
  });
});
