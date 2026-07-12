import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { routerPush, routerRefresh, resetCommonMocks } from "./helpers/dashboard-mocks";
import { INSTALL_STEP } from "@/lib/streaming/install-steps";
import type { FleetLibraryGalleryEntry } from "@/lib/types";

// InstallStates drives the inline template-only flow; its only boundaries are
// the install server action, analytics, and the Server-Sent Events (SSE) hook.
// Mock those; render the real flow + state-line components so selected → connect
// → creating → done/error and the SSE-driven step ladder are exercised end to
// end. M103 removed the import step — the template is already onboarded, so the
// flow opens on the selected template and creates with a visibility-keyed body.
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

import { InstallStates } from "../app/(dashboard)/w/[workspaceId]/fleets/new/InstallStates";
import { InstallStreamSteps } from "../app/(dashboard)/w/[workspaceId]/fleets/new/InstallStreamSteps";
import type { InstallSource } from "../app/(dashboard)/w/[workspaceId]/fleets/new/install-flow";

// A platform gallery entry — installs by slug (`platform_library_id`).
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
  support_files: [],
};

// A tenant gallery entry — installs by UUID (`tenant_library_id`), carries no
// per-credential reasons, and ships no TRIGGER.md (skill-only fallback).
const TEMPLATE_TENANT: FleetLibraryGalleryEntry = {
  id: "01932d4e-7c10-7a3a-9f00-000000000001",
  name: "Internal ops",
  description: "Tenant-authored ops fleet.",
  visibility: "tenant",
  source_ref: "tenant/01932d4e",
  requirements: { credentials: [], tools: [], network_hosts: [], trigger_present: false },
  required_credentials_reasons: {},
  support_files: [],
};

// Build a gallery entry with the given requirements overrides on the platform base.
function entry(overrides: Partial<FleetLibraryGalleryEntry> = {}): FleetLibraryGalleryEntry {
  return {
    ...TEMPLATE_GH,
    ...overrides,
    requirements: { ...TEMPLATE_GH.requirements, ...(overrides.requirements ?? {}) },
  };
}

// Default the SSE hook to "no install frame yet" so the stream-steps render
// `creating`; individual tests override the installStep they want to assert.
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

function renderStates(source: InstallSource, present: string[] | null = []) {
  return render(
    React.createElement(InstallStates, {
      workspaceId: "ws_1",
      source,
      presentCredentialNames: present,
      onBack: vi.fn(),
    }),
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks();
  stubStream(null);
});
afterEach(() => cleanup());

// ── 9.4: states render in order; error shows retry ──────────────────────────

describe("test_install_states_render", () => {
  it("a clean template auto-proceeds selected → creating with no confirm beat", async () => {
    // Hold create open so the `creating` line is observable before a fleet exists.
    let resolveCreate: (v: unknown) => void = () => {};
    installFleetActionMock.mockReturnValue(new Promise((r) => { resolveCreate = r; }));

    renderStates(TEMPLATE_GH, ["github"]);

    // No review page — proceeds straight into the states; the first line is the
    // already-onboarded template, then create runs with the platform body.
    await waitFor(() => expect(screen.getByText(/creating fleet/i)).toBeTruthy());
    expect(screen.getByText(/template · GitHub PR reviewer/i)).toBeTruthy();
    expect(screen.queryByText(/Review what it needs/i)).toBeNull();
    expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", {
      platform_library_id: "github-pr-reviewer",
    });
    resolveCreate({ ok: true, data: { fleet_id: "zom_x" } });
  });

  it("a tenant template installs by tenant_library_id", async () => {
    let resolveCreate: (v: unknown) => void = () => {};
    installFleetActionMock.mockReturnValue(new Promise((r) => { resolveCreate = r; }));
    renderStates(TEMPLATE_TENANT, []);
    await waitFor(() => expect(installFleetActionMock).toHaveBeenCalled());
    expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", {
      tenant_library_id: "01932d4e-7c10-7a3a-9f00-000000000001",
    });
    resolveCreate({ ok: true, data: { fleet_id: "zom_t" } });
  });

  it("holds at the connect gate when a required secret is missing", async () => {
    renderStates(TEMPLATE_GH, []); // github not present
    await waitFor(() => expect(screen.getByText(/first run: connect github/i)).toBeTruthy());
    const link = screen.getByRole("link", { name: /connect github/i });
    expect(link.getAttribute("href")).toBe("/w/ws_1/secrets");
    // Purpose-driven copy from the template's per-credential reason (data-driven).
    expect(screen.getByText(/review your pull requests/i)).toBeTruthy();
    // Create is gated — no fleet created yet.
    expect(installFleetActionMock).not.toHaveBeenCalled();
    // No skip path: connecting is the only action (the "Continue" button is gone).
    expect(screen.queryByRole("button", { name: /continue/i })).toBeNull();
  });

  it("pluralises the connect gate copy when multiple credentials are unmet", async () => {
    renderStates(
      entry({ requirements: { ...TEMPLATE_GH.requirements, credentials: ["github", "zoho"] } }),
      [],
    );
    await waitFor(() => expect(screen.getByText(/first run: connect github, zoho/i)).toBeTruthy());
    // github has a reason but zoho does not → not every credential has one, so
    // the gate falls back to the generic copy rather than a half-listed purpose.
    expect(screen.getByText(/Add them in Secrets/i)).toBeTruthy();
  });

  it("joins per-credential reasons with \"and\" when every unmet credential has one", async () => {
    renderStates(
      entry({
        requirements: { ...TEMPLATE_GH.requirements, credentials: ["github", "zoho"] },
        required_credentials_reasons: {
          github: "review your pull requests",
          zoho: "read your zoho activity",
        },
      }),
      [],
    );
    // Every unmet credential carries a reason → the purpose-driven sentence
    // joins them with "and"; the generic "Add them in Secrets" copy is gone.
    await waitFor(() =>
      expect(
        screen.getByText(/review your pull requests and read your zoho activity/i),
      ).toBeTruthy(),
    );
    expect(screen.queryByText(/Add them in Secrets/i)).toBeNull();
  });

  it("uses Add token when the missing secret is not GitHub", async () => {
    renderStates(
      entry({ requirements: { ...TEMPLATE_GH.requirements, credentials: ["zoho"] } }),
      [],
    );

    await waitFor(() => expect(screen.getByText(/first run: connect zoho/i)).toBeTruthy());
    expect(screen.getByRole("link", { name: /add token/i }).getAttribute("href")).toBe(
      "/w/ws_1/secrets",
    );
  });

  it("auto-creates when the required credential is already present (no gate)", async () => {
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_after_gate" } });
    // The credential is present, so the gate never shows and create runs on mount.
    renderStates(TEMPLATE_GH, ["github"]);
    await waitFor(() => expect(installFleetActionMock).toHaveBeenCalled());
    expect(screen.queryByText(/first run: connect github/i)).toBeNull();
  });

  it("renders the skill-only line when the template has no TRIGGER.md", async () => {
    installFleetActionMock.mockReturnValue(new Promise(() => {}));
    renderStates(TEMPLATE_TENANT, []);
    await waitFor(() => expect(screen.getByText(/manual API wake will be generated/i)).toBeTruthy());
  });

  it("a create 409 surfaces the duplicate-name hint with Retry", async () => {
    installFleetActionMock.mockResolvedValue({
      ok: false,
      error: "conflict",
      errorCode: "UZ-FLEET-409",
      status: 409,
    });
    renderStates(entry({ requirements: { ...TEMPLATE_GH.requirements, credentials: [] } }), []);
    await waitFor(() =>
      expect(screen.getByText(/already exists in this workspace/i)).toBeTruthy(),
    );
    expect(screen.getByRole("button", { name: /retry/i })).toBeTruthy();
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("the mount guard blocks a second create when the effect re-runs", async () => {
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_once" } });
    const source = entry({ requirements: { ...TEMPLATE_GH.requirements, credentials: [] } });
    const { rerender } = render(
      React.createElement(InstallStates, {
        workspaceId: "ws_1",
        source,
        presentCredentialNames: [],
        onBack: vi.fn(),
      }),
    );
    await waitFor(() => expect(installFleetActionMock).toHaveBeenCalledTimes(1));
    // A fresh presentCredentialNames reference re-fires the mount effect; the
    // `started` guard (`if (started.current) return;`) holds, so create runs once.
    rerender(
      React.createElement(InstallStates, {
        workspaceId: "ws_1",
        source,
        presentCredentialNames: [],
        onBack: vi.fn(),
      }),
    );
    await waitFor(() => expect(installFleetActionMock).toHaveBeenCalledTimes(1));
  });

  it("a non-409 create failure renders the error and Retry re-runs create", async () => {
    installFleetActionMock
      .mockResolvedValueOnce({ ok: false, error: "boom", errorCode: "UZ-SRV", status: 500 })
      .mockResolvedValueOnce({ ok: true, data: { fleet_id: "zom_retry" } });
    stubStream(INSTALL_STEP.READY);
    const user = userEvent.setup({ delay: null });
    renderStates(entry({ requirements: { ...TEMPLATE_GH.requirements, credentials: [] } }), []);

    await waitFor(() => expect(screen.getByRole("button", { name: /retry/i })).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /retry/i }));
    // The retry re-creates and, with the stream reporting ready, lands Open fleet.
    await waitFor(() => expect(screen.getByRole("button", { name: /open fleet/i })).toBeTruthy());
    expect(installFleetActionMock).toHaveBeenCalledTimes(2);
  });
});

// ── 9.7 (component tier): SSE steps advance + ready lands "Open fleet" ───────

describe("test_install_status_stream — InstallStreamSteps consumes the SSE stream", () => {
  function renderSteps(onOpen = vi.fn()) {
    return render(
      React.createElement(InstallStreamSteps, {
        workspaceId: "ws_1",
        fleetId: "zom_1",
        fleetName: "pr-reviewer",
        onOpen,
      }),
    );
  }

  it("renders the creating step before any install frame, no Open fleet yet", () => {
    stubStream(null);
    renderSteps();
    expect(screen.getByText(/creating fleet/i)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /open fleet/i })).toBeNull();
  });

  it("advances to provisioning when the stream reports it", () => {
    stubStream(INSTALL_STEP.PROVISIONING);
    renderSteps();
    expect(screen.getByText(/provisioning/i)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /open fleet/i })).toBeNull();
  });

  it("on install:ready surfaces Open fleet, which routes to the steer/chat", async () => {
    stubStream(INSTALL_STEP.READY);
    const onOpen = vi.fn();
    const user = userEvent.setup({ delay: null });
    renderSteps(onOpen);
    expect(screen.getByText(/is ready/i)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /open fleet/i }));
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("an error step renders the failure line (spinner never hangs)", () => {
    stubStream(INSTALL_STEP.ERROR);
    renderSteps();
    expect(screen.getByText(/install failed/i)).toBeTruthy();
  });
});

// ── 9.6: install done routes into the fleet (the steer/chat) ─────────────────

describe("test_install_lands_in_steer", () => {
  it("create → ready → Open fleet pushes to /w/ws_1/fleets/{id} (the full-height steer/chat)", async () => {
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_steer" } });
    // The fleet exists and the stream reports ready → Open fleet is offered.
    stubStream(INSTALL_STEP.READY);
    const user = userEvent.setup({ delay: null });
    renderStates(entry({ requirements: { ...TEMPLATE_GH.requirements, credentials: [] } }), []);

    await waitFor(() => expect(screen.getByRole("button", { name: /open fleet/i })).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /open fleet/i }));
    expect(routerPush).toHaveBeenCalledWith("/w/ws_1/fleets/zom_steer");
    expect(captureProductEventMock).toHaveBeenCalled();
    expect(routerRefresh).not.toHaveBeenCalled();
  });
});
