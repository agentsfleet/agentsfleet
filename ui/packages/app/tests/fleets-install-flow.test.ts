import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetCommonMocks } from "./helpers/dashboard-mocks";
import type { FleetTemplateGalleryEntry } from "@/lib/types";

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
vi.mock("@/app/(dashboard)/fleets/actions", () => ({
  installFleetAction: installFleetActionMock,
}));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: captureProductEventMock }));
vi.mock("@/components/domain/useFleetEventStream", () => ({
  useFleetEventStream: useFleetEventStreamMock,
}));

import { InstallFleet } from "../app/(dashboard)/fleets/new/InstallFleet";

// A platform gallery entry (installs by slug) and a tenant one (installs by
// UUID). Mirrors GET /v1/workspaces/{ws}/fleet-templates.
const TEMPLATE_GH: FleetTemplateGalleryEntry = {
  id: "github-pr-reviewer",
  name: "GitHub PR reviewer",
  description: "Reviews pull requests.",
  visibility: "platform",
  source_ref: "platform/github-pr-reviewer",
  requirements: {
    credentials: ["github"],
    tools: ["github_review_comment"],
    network_hosts: ["api.github.com"],
    trigger_present: true,
  },
  required_credentials_reasons: { github: "review your pull requests" },
  support_files: [],
};
const TEMPLATE_TENANT: FleetTemplateGalleryEntry = {
  id: "01932d4e-7c10-7a3a-9f00-000000000001",
  name: "Internal ops",
  description: "Tenant-authored ops fleet.",
  visibility: "tenant",
  source_ref: "tenant/01932d4e",
  requirements: { credentials: [], tools: [], network_hosts: [], trigger_present: true },
  required_credentials_reasons: {},
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

type FlowProps = {
  templates?: FleetTemplateGalleryEntry[];
  presentCredentialNames?: string[] | null;
  initialTemplateId?: string;
};

function renderFlow(props: FlowProps = {}) {
  return render(
    React.createElement(InstallFleet, {
      workspaceId: "ws_1",
      templates: props.templates ?? [TEMPLATE_GH, TEMPLATE_TENANT],
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

// Drive the confirm step that sits between picking a template and the live
// states: optionally type a fleet-name override, then click Install.
async function confirmInstall(
  user: ReturnType<typeof userEvent.setup>,
  name?: string,
): Promise<void> {
  await waitFor(() => expect(screen.getByRole("button", { name: "Install" })).toBeTruthy());
  if (name !== undefined) {
    await user.type(screen.getByLabelText("Fleet name"), name);
  }
  await user.click(screen.getByRole("button", { name: "Install" }));
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
    expect(screen.getByText("Fleet Library")).toBeTruthy();
    expect(screen.getByText("GitHub PR reviewer")).toBeTruthy();
    expect(screen.getByText("needs: github")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Use template" }).length).toBe(2);
  });

  it("shows an empty state when no templates are available", () => {
    renderFlow({ templates: [] });
    expect(screen.getByText("No fleet library yet")).toBeTruthy();
  });
});

// ── 9.3: picking a template proceeds INLINE to the states (no review page) ───

describe("test_install_inline_state_driven", () => {
  it("Use template → confirm → fires create with the platform body", async () => {
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_new" } });
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: ["github"] });

    await user.click(useTemplateButton(0));
    await confirmInstall(user);

    // Inline states — NOT the retired review page.
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
    expect(screen.queryByText("Review what it needs")).toBeNull();
    await waitFor(() =>
      expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", {
        platform_template_id: "github-pr-reviewer",
      }),
    );
  });

  it("a tenant template installs with the tenant body", async () => {
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_tenant" } });
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: [] });

    await user.click(useTemplateButton(1)); // TEMPLATE_TENANT
    await confirmInstall(user);
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
    await waitFor(() =>
      expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", {
        tenant_template_id: "01932d4e-7c10-7a3a-9f00-000000000001",
      }),
    );
  });

  it("an operator-supplied name overrides the SKILL.md name in the create body", async () => {
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_named" } });
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: ["github"] });

    await user.click(useTemplateButton(0));
    await confirmInstall(user, "pr-reviewer-frontend");
    await waitFor(() =>
      expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", {
        platform_template_id: "github-pr-reviewer",
        name: "pr-reviewer-frontend",
      }),
    );
  });

  it("an operator-supplied name overrides the SKILL.md name for a tenant template too", async () => {
    installFleetActionMock.mockResolvedValue({ ok: true, data: { fleet_id: "zom_tenant_named" } });
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: [] });

    await user.click(useTemplateButton(1)); // TEMPLATE_TENANT — installs by UUID
    await confirmInstall(user, "ops-frontend");
    await waitFor(() =>
      expect(installFleetActionMock).toHaveBeenCalledWith("ws_1", {
        tenant_template_id: "01932d4e-7c10-7a3a-9f00-000000000001",
        name: "ops-frontend",
      }),
    );
  });

  it("the confirm step renders no description paragraph when the template has none", async () => {
    installFleetActionMock.mockReturnValue(new Promise(() => {}));
    const user = userEvent.setup({ delay: null });
    // A template whose SKILL.md carried no `description:` → the confirm panel
    // shows the name but skips the description line (the `: null` branch).
    const noDesc = { ...TEMPLATE_GH, id: "no-desc", name: "No description template", description: "" };
    renderFlow({ templates: [noDesc], presentCredentialNames: ["github"] });

    await user.click(useTemplateButton(0));
    // Reaching the confirm step (Install button) renders InstallConfirm with a
    // falsy description; the panel still surfaces the template name.
    await waitFor(() => expect(screen.getByRole("button", { name: "Install" })).toBeTruthy());
    expect(screen.getByText("No description template")).toBeTruthy();
  });

  it("preselects a template from a ?template= deep link and lands on the confirm step", async () => {
    installFleetActionMock.mockReturnValue(new Promise(() => {}));
    const user = userEvent.setup({ delay: null });
    renderFlow({ initialTemplateId: "github-pr-reviewer", presentCredentialNames: ["github"] });
    await confirmInstall(user);
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
  });

  it("ignores a ?template= deep link that matches no template", () => {
    renderFlow({ initialTemplateId: "does-not-exist" });
    expect(screen.getByText("Fleet Library")).toBeTruthy();
  });

  it("Back from the states returns to the selector", async () => {
    installFleetActionMock.mockReturnValue(new Promise(() => {}));
    const user = userEvent.setup({ delay: null });
    renderFlow({ presentCredentialNames: ["github"] });
    await user.click(useTemplateButton(0));
    await confirmInstall(user);
    await waitFor(() => expect(screen.getByLabelText("Install states")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: /Back to templates/ }));
    expect(screen.getByText("Fleet Library")).toBeTruthy();
  });
});
