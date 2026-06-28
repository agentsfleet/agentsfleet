import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { routerPush, routerRefresh, resetCommonMocks } from "./helpers/dashboard-mocks";
import { INSTALL_STEP } from "@/lib/streaming/install-steps";

// InstallStates drives the inline flow; its only boundaries are the two server
// actions, analytics, and the Server-Sent Events (SSE) hook. Mock those; render
// the real flow + state-line components so importing → connect → creating →
// done/error and the SSE-driven step ladder are exercised end to end.
const {
  importBundleActionMock,
  installFleetActionMock,
  captureProductEventMock,
  useFleetEventStreamMock,
} = vi.hoisted(() => ({
  importBundleActionMock: vi.fn(),
  installFleetActionMock: vi.fn(),
  captureProductEventMock: vi.fn(),
  useFleetEventStreamMock: vi.fn(),
}));

vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@/app/(dashboard)/fleets/actions", () => ({
  importBundleAction: importBundleActionMock,
  installFleetAction: installFleetActionMock,
}));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: captureProductEventMock }));
vi.mock("@/components/domain/useFleetEventStream", () => ({
  useFleetEventStream: useFleetEventStreamMock,
}));

import { InstallStates } from "../app/(dashboard)/fleets/new/InstallStates";
import { InstallStreamSteps } from "../app/(dashboard)/fleets/new/InstallStreamSteps";
import type { InstallSource } from "../app/(dashboard)/fleets/new/install-flow";

const TEMPLATE_GH = {
  id: "github-pr-reviewer",
  name: "GitHub PR reviewer",
  description: "Reviews pull requests.",
  required_credentials: ["github"],
  required_tools: ["github_review_comment"],
  network_hosts: ["api.github.com"],
};

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
  it("a clean template auto-proceeds importing → creating with no confirm beat", async () => {
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_1" } });
    // Hold create open so the `creating` line is observable before a fleet exists.
    let resolveCreate: (v: unknown) => void = () => {};
    installFleetActionMock.mockReturnValue(new Promise((r) => { resolveCreate = r; }));

    renderStates({ kind: "template", template: TEMPLATE_GH }, ["github"]);

    // No review page — proceeds straight into the states; no "Use template"/"Create" button.
    await waitFor(() => expect(screen.getByText(/creating fleet/i)).toBeTruthy());
    expect(screen.queryByText(/Review what it needs/i)).toBeNull();
    expect(importBundleActionMock).toHaveBeenCalledWith("ws_1", {
      source_kind: "template",
      source_ref: "github-pr-reviewer",
    });
    resolveCreate({ ok: true, data: { fleet_id: "zom_x" } });
  });

  it("gates on connect-to-continue when a required credential is missing", async () => {
    renderStates({ kind: "template", template: TEMPLATE_GH }, []); // github not present
    await waitFor(() => expect(screen.getByText(/first run: connect github/i)).toBeTruthy());
    const link = screen.getByRole("link", { name: /connect github/i });
    expect(link.getAttribute("href")).toBe("/credentials");
    // Create is gated — no fleet created yet.
    expect(installFleetActionMock).not.toHaveBeenCalled();
    // No skip path: connecting is the only action (the "Continue" button is gone).
    expect(screen.queryByRole("button", { name: /continue/i })).toBeNull();
  });

  it("pluralises the connect-to-continue copy when multiple credentials are unmet", async () => {
    renderStates(
      { kind: "template", template: { ...TEMPLATE_GH, required_credentials: ["github", "zoho"] } },
      [],
    );
    await waitFor(() => expect(screen.getByText(/first run: connect github, zoho/i)).toBeTruthy());
    expect(screen.getByText(/Add them in Credentials/i)).toBeTruthy();
  });

  it("uses Add token when the missing credential is not GitHub", async () => {
    renderStates(
      { kind: "template", template: { ...TEMPLATE_GH, required_credentials: ["zoho"] } },
      [],
    );

    await waitFor(() => expect(screen.getByText(/first run: connect zoho/i)).toBeTruthy());
    expect(screen.getByRole("link", { name: /add token/i }).getAttribute("href")).toBe(
      "/credentials",
    );
  });

  it("a paste source carrying a TRIGGER.md posts both markdown bodies", async () => {
    let resolveCreate: (v: unknown) => void = () => {};
    installFleetActionMock.mockReturnValue(new Promise((r) => { resolveCreate = r; }));
    renderStates({ kind: "paste", sourceMarkdown: "skill-body", triggerMarkdown: "trigger-body" }, []);
    await waitFor(() => expect(installFleetActionMock).toHaveBeenCalled());
    expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", {
      source_markdown: "skill-body",
      trigger_markdown: "trigger-body",
    });
    resolveCreate({ ok: true, data: { fleet_id: "zom_p" } });
  });

  it("auto-creates when the required credential is already present (no gate)", async () => {
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_1" } });
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_after_gate" } });
    // The credential is present, so the gate never shows and create runs on mount.
    renderStates({ kind: "template", template: TEMPLATE_GH }, ["github"]);
    await waitFor(() => expect(installFleetActionMock).toHaveBeenCalled());
    expect(screen.queryByText(/first run: connect github/i)).toBeNull();
  });

  it("renders the skill-only line when the snapshot has no TRIGGER.md", async () => {
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_1" } });
    installFleetActionMock.mockReturnValue(new Promise(() => {}));
    renderStates(
      {
        kind: "github",
        snapshot: {
          bundle_id: "bnd_x",
          name: "acme/repo",
          source_kind: "github",
          source_ref: "acme/repo",
          validation_status: "ok",
          content_hash: "h",
          snapshot_key: "k",
          requirements: { credentials: [], tools: [], network_hosts: [], support_files: [], trigger_present: false },
          support_files: [],
        },
      },
    );
    await waitFor(() => expect(screen.getByText(/manual API wake will be generated/i)).toBeTruthy());
  });

  it("an import error (404 / no SKILL.md / rate-limited) renders an error line with Retry", async () => {
    importBundleActionMock.mockResolvedValue({
      ok: false,
      error: "no SKILL.md",
      errorCode: "UZ-BUNDLE-004",
      status: 404,
    });
    renderStates({ kind: "template", template: { ...TEMPLATE_GH, required_credentials: [] } }, []);
    await waitFor(() => expect(screen.getByRole("button", { name: /retry/i })).toBeTruthy());
    expect(installFleetActionMock).not.toHaveBeenCalled();
  });

  it("a create 409 surfaces the duplicate-name hint with Retry", async () => {
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_1" } });
    installFleetActionMock.mockResolvedValue({
      ok: false,
      error: "conflict",
      errorCode: "UZ-FLEET-409",
      status: 409,
    });
    renderStates({ kind: "template", template: { ...TEMPLATE_GH, required_credentials: [] } }, []);
    await waitFor(() =>
      expect(screen.getByText(/already exists in this workspace/i)).toBeTruthy(),
    );
    expect(screen.getByRole("button", { name: /retry/i })).toBeTruthy();
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("a non-409 create failure renders the error and Retry re-runs create", async () => {
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_1" } });
    installFleetActionMock
      .mockResolvedValueOnce({ ok: false, error: "boom", errorCode: "UZ-SRV", status: 500 })
      .mockResolvedValueOnce({ ok: true, data: { fleet_id: "zom_retry" } });
    stubStream(INSTALL_STEP.READY);
    const user = userEvent.setup({ delay: null });
    renderStates({ kind: "template", template: { ...TEMPLATE_GH, required_credentials: [] } }, []);

    await waitFor(() => expect(screen.getByRole("button", { name: /retry/i })).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /retry/i }));
    // The retry re-creates and, with the stream reporting ready, lands Open fleet.
    await waitFor(() => expect(screen.getByRole("button", { name: /open fleet/i })).toBeTruthy());
    expect(installFleetActionMock).toHaveBeenCalledTimes(2);
  });

  it("a template whose lazy import fails at create-time surfaces the error with Retry", async () => {
    // No unmet credential → auto-proceeds to create, where the template imports
    // lazily; that import fails, exercising the resolveCreateBody error arm.
    importBundleActionMock.mockResolvedValue({
      ok: false,
      error: "repo empty",
      errorCode: "UZ-BUNDLE-004",
      status: 404,
    });
    renderStates({ kind: "template", template: { ...TEMPLATE_GH, required_credentials: [] } }, []);
    await waitFor(() => expect(screen.getByRole("button", { name: /retry/i })).toBeTruthy());
    expect(installFleetActionMock).not.toHaveBeenCalled();
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
  it("create → ready → Open fleet pushes to /fleets/{id} (the full-height steer/chat)", async () => {
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_1" } });
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_steer" } });
    // The fleet exists and the stream reports ready → Open fleet is offered.
    stubStream(INSTALL_STEP.READY);
    const user = userEvent.setup({ delay: null });
    renderStates({ kind: "template", template: { ...TEMPLATE_GH, required_credentials: [] } }, []);

    await waitFor(() => expect(screen.getByRole("button", { name: /open fleet/i })).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /open fleet/i }));
    expect(routerPush).toHaveBeenCalledWith("/fleets/zom_steer");
    expect(captureProductEventMock).toHaveBeenCalled();
    expect(routerRefresh).not.toHaveBeenCalled();
  });
});
