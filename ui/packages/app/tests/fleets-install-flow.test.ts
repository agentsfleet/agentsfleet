import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { routerPush, resetCommonMocks } from "./helpers/dashboard-mocks";

// The install flow's only boundaries are the two server actions and analytics.
// Mock those; render the real design-system primitives + child components so the
// gallery → preview → create wiring is exercised end to end.
const { importBundleActionMock, installFleetActionMock, captureProductEventMock } = vi.hoisted(() => ({
  importBundleActionMock: vi.fn(),
  installFleetActionMock: vi.fn(),
  captureProductEventMock: vi.fn(),
}));

vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@/app/(dashboard)/fleets/actions", () => ({
  importBundleAction: importBundleActionMock,
  installFleetAction: installFleetActionMock,
}));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: captureProductEventMock }));

import { InstallFleet } from "../app/(dashboard)/fleets/new/InstallFleet";
import { InstallSourceSelector } from "../app/(dashboard)/fleets/new/InstallSourceSelector";

const TEMPLATE_GH = {
  id: "github-pr-reviewer",
  name: "GitHub PR reviewer",
  description: "Reviews pull requests.",
  required_credentials: ["github"],
  required_tools: ["github_review_comment"],
  network_hosts: ["api.github.com"],
};
const TEMPLATE_BARE = {
  id: "hello",
  name: "Hello bot",
  description: "Says hi.",
  required_credentials: [],
  required_tools: [],
  network_hosts: [],
};

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

function createButton() {
  return screen.getByRole("button", { name: /Create teammate/ }) as HTMLButtonElement;
}

function useTemplateButton(index: number): HTMLElement {
  const button = screen.getAllByRole("button", { name: "Use template" })[index];
  if (!button) throw new Error(`no "Use template" button at index ${index}`);
  return button;
}

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks();
});
afterEach(() => cleanup());

describe("install flow — source selector", () => {
  it("leads with the template gallery and offers GitHub + paste as secondary sources", () => {
    renderFlow();
    expect(screen.getByText("Start from a template")).toBeTruthy();
    expect(screen.getByText("GitHub PR reviewer")).toBeTruthy();
    expect(screen.getByText("needs: github")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Use template" }).length).toBe(2);
    expect(screen.getByLabelText("GitHub owner/repo")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Import from GitHub" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Paste SKILL.md instead" })).toBeTruthy();
  });

  it("shows an empty state when no templates are available", () => {
    renderFlow({ templates: [] });
    expect(screen.getByText("No templates available yet")).toBeTruthy();
  });
});

describe("install flow — credential preview routing", () => {
  it("routes a missing service credential to the workspace credentials flow, not the model provider", async () => {
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: [] });
    await user.click(useTemplateButton(0));

    expect(screen.getByText("Review what it needs")).toBeTruthy();
    expect(screen.getByText("github")).toBeTruthy();
    expect(screen.getByText("missing")).toBeTruthy();
    // tools + network facts render from template metadata
    expect(screen.getByText("github_review_comment")).toBeTruthy();
    expect(screen.getByText("api.github.com")).toBeTruthy();

    const connect = screen.getByRole("link", { name: "Connect" });
    expect(connect.getAttribute("href")).toBe("/credentials");
    expect(connect.getAttribute("href")).not.toContain("/settings/models");
    // create is gated until the credential is connected
    expect(createButton().disabled).toBe(true);
    expect(screen.getByText("Connect the required credentials, then create.")).toBeTruthy();
  });

  it("does not gate create when the credential vault could not be read", async () => {
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: null });
    await user.click(useTemplateButton(0));

    expect(screen.getByText("github")).toBeTruthy(); // requirement still listed
    // unknown vault → neither connected nor missing is claimed, create not gated
    expect(screen.queryByText("missing")).toBeNull();
    expect(screen.queryByText("connected")).toBeNull();
    expect(screen.queryByText("Connect the required credentials, then create.")).toBeNull();
    expect(createButton().disabled).toBe(false);
  });
});

describe("install flow — create from a template", () => {
  it("imports then creates once the credential is present, and navigates to the Fleet", async () => {
    const user = userEvent.setup({ delay: null });
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_1" } });
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_new" } });
    renderFlow({ presentCredentialNames: ["github"] });

    await user.click(useTemplateButton(0));
    expect(screen.getByText("connected")).toBeTruthy();
    expect(createButton().disabled).toBe(false);

    await user.click(createButton());
    await waitFor(() => expect(routerPush).toHaveBeenCalledWith("/fleets/zom_new"));
    expect(importBundleActionMock).toHaveBeenCalledWith("ws_1", {
      source_kind: "template",
      source_ref: "github-pr-reviewer",
    });
    expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", { bundle_id: "bnd_1", name: undefined });
    expect(captureProductEventMock).toHaveBeenCalled();
  });

  it("passes an operator-typed name override to create (multi-instance)", async () => {
    const user = userEvent.setup({ delay: null });
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_1" } });
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_named" } });
    renderFlow({ presentCredentialNames: ["github"] });

    await user.click(useTemplateButton(0));
    await user.type(screen.getByLabelText("Name"), "pr-reviewer-blog");
    await user.click(createButton());
    await waitFor(() => expect(routerPush).toHaveBeenCalledWith("/fleets/zom_named"));
    expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", {
      bundle_id: "bnd_1",
      name: "pr-reviewer-blog",
    });
  });

  it("surfaces the duplicate-name hint when create returns 409", async () => {
    const user = userEvent.setup({ delay: null });
    importBundleActionMock.mockResolvedValue({ ok: true, data: { bundle_id: "bnd_1" } });
    installFleetActionMock.mockResolvedValue({
      ok: false,
      error: "conflict",
      errorCode: "UZ-FLEET-409",
      status: 409,
    });
    renderFlow({ presentCredentialNames: ["github"] });

    await user.click(useTemplateButton(0));
    await user.click(createButton());
    await waitFor(() =>
      expect(
        screen.getByText("That teammate name already exists in this workspace."),
      ).toBeTruthy(),
    );
    expect(routerPush).not.toHaveBeenCalled();
  });

  it("surfaces the import error when the template repo is unpopulated", async () => {
    const user = userEvent.setup({ delay: null });
    importBundleActionMock.mockResolvedValue({
      ok: false,
      error: "no SKILL.md",
      errorCode: "UZ-BUNDLE-004",
      status: 502,
    });
    renderFlow({ presentCredentialNames: ["github"] });

    await user.click(useTemplateButton(0));
    await user.click(createButton());
    await waitFor(() => expect(importBundleActionMock).toHaveBeenCalled());
    expect(installFleetActionMock).not.toHaveBeenCalled();
    expect(routerPush).not.toHaveBeenCalled();
    expect(screen.getByText("Review what it needs")).toBeTruthy(); // stayed on preview
  });

  it("returns to the selector from a template preview with no required credentials", async () => {
    const user = userEvent.setup({ delay: null });
    renderFlow();
    await user.click(useTemplateButton(1)); // bare template
    expect(screen.getByText("No credentials required.")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByText("Start from a template")).toBeTruthy();
  });
});

describe("install flow — GitHub source", () => {
  it("imports a public repo, previews the snapshot, and creates from its bundle_id", async () => {
    const user = userEvent.setup({ delay: null });
    importBundleActionMock.mockResolvedValue({
      ok: true,
      data: {
        bundle_id: "bnd_gh",
        name: "acme/pr-reviewer",
        requirements: { credentials: [], tools: [], network_hosts: [], support_files: [], trigger_present: true },
      },
    });
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_gh" } });
    renderFlow();

    await user.type(screen.getByLabelText("GitHub owner/repo"), "acme/pr-reviewer");
    await user.click(screen.getByRole("button", { name: "Import from GitHub" }));
    await waitFor(() => expect(screen.getByText("Review what it needs")).toBeTruthy());
    expect(importBundleActionMock).toHaveBeenCalledWith("ws_1", {
      source_kind: "github",
      source_ref: "acme/pr-reviewer",
    });

    await user.click(createButton());
    await waitFor(() => expect(routerPush).toHaveBeenCalledWith("/fleets/zom_gh"));
    expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", { bundle_id: "bnd_gh", name: undefined });
  });

  it("disables the import button with a pending label while an import is in flight", () => {
    // Rendered directly: the pending arm is deterministic via the prop, without
    // racing a live useActionState transition.
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

  it("rejects a GitHub ref that is not owner/repo without calling the server", async () => {
    const user = userEvent.setup({ delay: null });
    renderFlow();
    await user.type(screen.getByLabelText("GitHub owner/repo"), "notaslug");
    await user.click(screen.getByRole("button", { name: "Import from GitHub" }));
    await waitFor(() =>
      expect(screen.getByText("Enter a GitHub repository as owner/repo.")).toBeTruthy(),
    );
    expect(importBundleActionMock).not.toHaveBeenCalled();
  });

  it("surfaces a GitHub import failure and stays on the selector", async () => {
    const user = userEvent.setup({ delay: null });
    importBundleActionMock.mockResolvedValue({
      ok: false,
      error: "repo not found",
      errorCode: "UZ-BUNDLE-004",
      status: 502,
    });
    renderFlow();
    await user.type(screen.getByLabelText("GitHub owner/repo"), "acme/missing");
    await user.click(screen.getByRole("button", { name: "Import from GitHub" }));
    await waitFor(() => expect(importBundleActionMock).toHaveBeenCalled());
    expect(screen.getByText("Start from a template")).toBeTruthy();
    expect(screen.queryByText("Review what it needs")).toBeNull();
  });

  it("surfaces a create failure after a successful import", async () => {
    const user = userEvent.setup({ delay: null });
    importBundleActionMock.mockResolvedValue({
      ok: true,
      data: {
        bundle_id: "bnd_x",
        name: "acme/repo",
        requirements: { credentials: [], tools: [], network_hosts: [], support_files: [], trigger_present: false },
      },
    });
    installFleetActionMock.mockResolvedValue({
      ok: false,
      error: "internal error",
      errorCode: "UZ-UNKNOWN",
      status: 500,
    });
    renderFlow();
    await user.type(screen.getByLabelText("GitHub owner/repo"), "acme/repo");
    await user.click(screen.getByRole("button", { name: "Import from GitHub" }));
    await waitFor(() => expect(screen.getByText("Review what it needs")).toBeTruthy());
    await user.click(createButton());
    await waitFor(() => expect(installFleetActionMock).toHaveBeenCalled());
    expect(routerPush).not.toHaveBeenCalled();
  });
});

describe("install flow — paste fallback + deep links", () => {
  it("switches to the paste form and back to the gallery", async () => {
    const user = userEvent.setup({ delay: null });
    renderFlow();
    await user.click(screen.getByRole("button", { name: "Paste SKILL.md instead" }));
    expect(screen.getByText("SKILL.md body")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /Back to templates/ }));
    expect(screen.getByText("Start from a template")).toBeTruthy();
  });

  it("preselects a template from a ?template= deep link", async () => {
    renderFlow({ initialTemplateId: "github-pr-reviewer" });
    await waitFor(() => expect(screen.getByText("Review what it needs")).toBeTruthy());
  });

  it("ignores a ?template= deep link that matches no template", () => {
    renderFlow({ initialTemplateId: "does-not-exist" });
    expect(screen.getByText("Start from a template")).toBeTruthy();
  });

  it("preselects at most once even when the template list changes", async () => {
    const user = userEvent.setup({ delay: null });
    const { rerender } = renderFlow({ initialTemplateId: "github-pr-reviewer" });
    await waitFor(() => expect(screen.getByText("Review what it needs")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByText("Start from a template")).toBeTruthy();
    // New template list (changed effect dep) must NOT re-open the preview.
    rerender(
      React.createElement(InstallFleet, {
        workspaceId: "ws_1",
        templates: [TEMPLATE_GH],
        presentCredentialNames: [],
        initialTemplateId: "github-pr-reviewer",
      }),
    );
    expect(screen.getByText("Start from a template")).toBeTruthy();
  });
});
