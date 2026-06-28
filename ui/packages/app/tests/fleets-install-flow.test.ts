import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetCommonMocks } from "./helpers/dashboard-mocks";

// The install flow's boundaries are the two server actions, analytics, and the
// SSE hook (post-create). Mock those; render the real source selector + states
// so the source → inline-states wiring (no review page) is exercised end to end.
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

import { InstallFleet } from "../app/(dashboard)/fleets/new/InstallFleet";
import { InstallSourceSelector } from "../app/(dashboard)/fleets/new/InstallSourceSelector";

const TEMPLATE_GH = {
  id: "github-pr-reviewer",
  name: "GitHub PR reviewer",
  description: "Reviews pull requests.",
  required_credentials: ["github"],
  required_credentials_reasons: { github: "review your pull requests" },
  required_tools: ["github_review_comment"],
  network_hosts: ["api.github.com"],
};
const TEMPLATE_BARE = {
  id: "hello",
  name: "Hello bot",
  description: "Says hi.",
  required_credentials: [],
  required_credentials_reasons: {},
  required_tools: [],
  network_hosts: [],
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

type FlowProps = {
  templates?: typeof TEMPLATE_GH[];
  presentCredentialNames?: string[] | null;
  initialTemplateId?: string;
};

function renderFlow(props: FlowProps = {}) {
  return render(
    React.createElement(InstallFleet, {
      workspaceId: "ws_1",
      templates: props.templates ?? [TEMPLATE_GH, TEMPLATE_BARE],
      presentCredentialNames:
        props.presentCredentialNames === undefined ? [] : props.presentCredentialNames,
      initialTemplateId: props.initialTemplateId,
    }),
  );
}

function useTemplateButton(index: number): HTMLElement {
  const button = screen.getAllByRole("button", { name: "Use template" })[index];
  if (!button) throw new Error(`no "Use template" button at index ${index}`);
  return button;
}

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks();
  stubStream(null);
});
afterEach(() => cleanup());

// ── 9.1: the three install paths render minimally ───────────────────────────

describe("test_install_three_paths_render", () => {
  it("renders the template grid, the owner/repo import, and the paste-SKILL.md link", () => {
    renderFlow();
    expect(screen.getByText("Start from a template")).toBeTruthy();
    expect(screen.getByText("GitHub PR reviewer")).toBeTruthy();
    expect(screen.getByText("needs: github")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Use template" }).length).toBe(2);
    // owner/repo import path
    expect(screen.getByLabelText("GitHub owner/repo")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Import from GitHub" })).toBeTruthy();
    // paste a quiet tertiary link
    expect(screen.getByRole("button", { name: "Paste SKILL.md instead" })).toBeTruthy();
  });

  it("shows an empty state when no templates are available", () => {
    renderFlow({ templates: [] });
    expect(screen.getByText("No templates available yet")).toBeTruthy();
  });
});

// ── 9.3: clicking a source proceeds INLINE to the states (no review page) ────

describe("test_install_inline_state_driven", () => {
  it("Use template proceeds inline to the states and fires create with the template source", async () => {
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_1" } });
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_new" } });
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: ["github"] });

    await user.click(useTemplateButton(0));

    // Inline states — NOT the retired review page.
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
    expect(screen.queryByText("Review what it needs")).toBeNull();
    await waitFor(() =>
      expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", { bundle_id: "bnd_1" }),
    );
    expect(importBundleActionMock).toHaveBeenCalledWith("ws_1", {
      source_kind: "template",
      source_ref: "github-pr-reviewer",
    });
  });

  it("Import from GitHub proceeds inline to the states and creates from the snapshot bundle_id", async () => {
    importBundleActionMock
      .mockResolvedValueOnce({
        ok: true,
        data: {
          bundle_id: "bnd_gh",
          name: "acme/pr-reviewer",
          requirements: { credentials: [], tools: [], network_hosts: [], support_files: [], trigger_present: true },
        },
      });
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_gh" } });
    const user = userEvent.setup({ delay: null });
    renderFlow();

    await user.type(screen.getByLabelText("GitHub owner/repo"), "acme/pr-reviewer");
    await user.click(screen.getByRole("button", { name: "Import from GitHub" }));
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
    expect(screen.queryByText("Review what it needs")).toBeNull();
    await waitFor(() =>
      expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", { bundle_id: "bnd_gh" }),
    );
  });

  it("Paste-create validates then proceeds inline, posting the pasted source markdown", async () => {
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_paste" } });
    const user = userEvent.setup({ delay: null });
    renderFlow();

    await user.click(screen.getByRole("button", { name: "Paste SKILL.md instead" }));
    await user.type(
      screen.getByLabelText(/SKILL\.md body/i),
      "---\nname: pasted\ndescription: d\nversion: 0.1.0\n---\n# Pasted\n",
    );
    await user.click(screen.getByRole("button", { name: /create fleet/i }));

    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
    await waitFor(() => expect(installFleetActionMock).toHaveBeenCalled());
    const body = installFleetActionMock.mock.calls[0]![1] as { source_markdown?: string };
    expect(body.source_markdown).toContain("Pasted");
    // Paste posts markdown directly — never a bundle_id.
    expect(importBundleActionMock).not.toHaveBeenCalled();
  });

  it("a non-owner/repo import ref is rejected without a server call (stays on the selector)", async () => {
    const user = userEvent.setup({ delay: null });
    renderFlow();
    await user.type(screen.getByLabelText("GitHub owner/repo"), "notaslug");
    await user.click(screen.getByRole("button", { name: "Import from GitHub" }));
    await waitFor(() =>
      expect(screen.getByText("Enter a GitHub repository as owner/repo.")).toBeTruthy(),
    );
    expect(importBundleActionMock).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Install states")).toBeNull();
  });

  it("a GitHub import failure stays on the selector with the error", async () => {
    importBundleActionMock.mockResolvedValue({
      ok: false,
      error: "repo not found",
      errorCode: "UZ-BUNDLE-004",
      status: 404,
    });
    const user = userEvent.setup({ delay: null });
    renderFlow();
    await user.type(screen.getByLabelText("GitHub owner/repo"), "acme/missing");
    await user.click(screen.getByRole("button", { name: "Import from GitHub" }));
    await waitFor(() => expect(importBundleActionMock).toHaveBeenCalled());
    expect(screen.getByText("Start from a template")).toBeTruthy();
    expect(screen.queryByLabelText("Install states")).toBeNull();
  });

  it("preselects a template from a ?template= deep link and proceeds to its states", async () => {
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_dl" } });
    installFleetActionMock.mockReturnValue(new Promise(() => {}));
    renderFlow({ initialTemplateId: "github-pr-reviewer", presentCredentialNames: ["github"] });
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
  });

  it("ignores a ?template= deep link that matches no template", () => {
    renderFlow({ initialTemplateId: "does-not-exist" });
    expect(screen.getByText("Start from a template")).toBeTruthy();
  });

  it("the import-pending arm disables the button with a label", () => {
    render(
      React.createElement(InstallSourceSelector, {
        templates: [],
        onUseTemplate: vi.fn(),
        onImport: vi.fn(),
        importPending: true,
        importError: null,
        onPaste: vi.fn(),
      }),
    );
    const button = screen.getByRole("button", { name: "Importing…" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });

  it("Back from the paste input returns to the selector", async () => {
    const user = userEvent.setup({ delay: null });
    renderFlow();
    await user.click(screen.getByRole("button", { name: "Paste SKILL.md instead" }));
    expect(screen.getByText("SKILL.md body")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /Back to templates/ }));
    expect(screen.getByText("Start from a template")).toBeTruthy();
  });

  it("Back from the states returns to the selector", async () => {
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_1" } });
    installFleetActionMock.mockReturnValue(new Promise(() => {}));
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: ["github"] });
    await user.click(useTemplateButton(0));
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /Back to templates/ }));
    expect(screen.getByText("Start from a template")).toBeTruthy();
  });
});
