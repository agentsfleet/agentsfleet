import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { routerPush, routerRefresh, fetchMock, resetCommonMocks, authMock as auth } from "./helpers/dashboard-mocks";
import { EVENTS } from "../lib/analytics/events";

const captureProductEventMock = vi.fn();
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: captureProductEventMock,
}));

// Shared dashboard mock harness — see tests/helpers/dashboard-mocks.tsx.
vi.stubGlobal("fetch", fetchMock);
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());
vi.mock("@clerk/nextjs/server", async () => (await import("./helpers/dashboard-mocks")).clerkServerMock());
vi.mock("@clerk/nextjs", async () => (await import("./helpers/dashboard-mocks")).clerkMock());
vi.mock("next/link", async () => (await import("./helpers/dashboard-mocks")).nextLinkMock());
vi.mock("@/lib/workspace", async () => (await import("./helpers/dashboard-mocks")).workspaceMock());
vi.mock("lucide-react", async () => (await import("./helpers/dashboard-mocks")).lucideMock());
vi.mock("@agentsfleet/design-system", async (orig) => {
  const h = await import("./helpers/dashboard-mocks");
  return { ...h.designSystemCore(await orig<Record<string, unknown>>()), ...h.designSystemTabs() };
});

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks({ pathname: "/agents" });
});
afterEach(() => {
  cleanup();
  fetchMock.mockReset();
});

// These tests type a ~120-char multi-line TRIGGER.md fixture. `delay: null`
// fills the field in one synchronous pass instead of one keystroke per event
// loop tick, so the typing can't starve past testTimeout when the suite runs
// many shards in parallel — the byte content the assertions read is identical.
describe("InstallAgentForm interactions", () => {
  async function renderForm() {
    const { default: Form } = await import(
      "../app/(dashboard)/agents/new/InstallAgentForm"
    );
    return render(React.createElement(Form, { workspaceId: "ws_1" }));
  }

  const FIXTURE_TRIGGER =
    "---\nname: platform-ops\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools:\n    - agentmail\n  budget:\n    daily_dollars: 1.0\n---\n";
  const FIXTURE_SKILL =
    "---\nname: platform-ops\ndescription: Automates platform checks\nversion: 0.1.0\n---\n# Platform Ops\n";

  it("blank TRIGGER.md generates a manual-wake config", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => ({ agent_id: "zom_manual", status: "active" }),
    });
    const user = userEvent.setup({ delay: null });
    await renderForm();
    expect(screen.getByText(/What is SKILL\.md/i)).toBeTruthy();
    expect(screen.getAllByText(/manual wake/i).length).toBeGreaterThan(0);
    await user.type(screen.getByLabelText(/SKILL\.md body/i), FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /install teammate/i }));
    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        expect.stringContaining("/v1/workspaces/ws_1/agents"),
        expect.objectContaining({ method: "POST" }),
      ),
    );
    const callBody = JSON.parse(
      (fetchMock.mock.calls[0]![1] as RequestInit).body as string,
    ) as { trigger_markdown: string; source_markdown: string };
    expect(callBody.trigger_markdown).toContain('name: "platform-ops"');
    expect(callBody.trigger_markdown).toContain("type: api");
    expect(callBody.trigger_markdown).toContain("tools: []");
    expect(routerPush).toHaveBeenCalledWith("/agents/zom_manual");
  });

  it("empty SKILL.md blocks submit and shows the required-field error", async () => {
    const user = userEvent.setup({ delay: null });
    await renderForm();
    await user.type(screen.getByLabelText(/TRIGGER\.md body/i), FIXTURE_TRIGGER);
    await user.click(screen.getByRole("button", { name: /install teammate/i }));
    await waitFor(() =>
      expect(screen.getByText(/SKILL\.md body is required/i)).toBeTruthy(),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("invalid SKILL.md frontmatter blocks submit without a network call", async () => {
    const user = userEvent.setup({ delay: null });
    await renderForm();
    await user.type(screen.getByLabelText(/TRIGGER\.md body/i), FIXTURE_TRIGGER);
    await user.type(screen.getByLabelText(/SKILL\.md body/i), "# missing frontmatter");
    await user.click(screen.getByRole("button", { name: /install teammate/i }));
    await waitFor(() =>
      expect(screen.getByText(/SKILL\.md needs YAML frontmatter/i)).toBeTruthy(),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("invalid TRIGGER.md frontmatter blocks submit without a network call", async () => {
    const user = userEvent.setup({ delay: null });
    await renderForm();
    await user.type(screen.getByLabelText(/TRIGGER\.md body/i), "---\nname: platform-ops\n---\n");
    await user.type(screen.getByLabelText(/SKILL\.md body/i), FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /install teammate/i }));
    await waitFor(() =>
      expect(screen.getByText(/TRIGGER\.md frontmatter needs x-agentsfleet:/i)).toBeTruthy(),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("happy path: fills form, POSTs trigger+source markdown, redirects to detail", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => ({ agent_id: "zom_new", status: "active" }),
    });
    const user = userEvent.setup({ delay: null });
    await renderForm();
    const skillField = screen.getByLabelText(/SKILL\.md body/i);
    const triggerField = screen.getByLabelText(/TRIGGER\.md body/i);
    expect(skillField.compareDocumentPosition(triggerField) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await user.type(screen.getByLabelText(/TRIGGER\.md body/i), FIXTURE_TRIGGER);
    await user.type(screen.getByLabelText(/SKILL\.md body/i), FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /install teammate/i }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        expect.stringContaining("/v1/workspaces/ws_1/agents"),
        expect.objectContaining({ method: "POST" }),
      ),
    );
    const callBody = JSON.parse(
      (fetchMock.mock.calls[0]![1] as RequestInit).body as string,
    ) as { trigger_markdown: string; source_markdown: string };
    expect(Object.keys(callBody).sort()).toEqual(["source_markdown", "trigger_markdown"]);
    expect(callBody.trigger_markdown).toContain("name: platform-ops");
    expect(callBody.trigger_markdown).toContain("x-agentsfleet:");
    expect(callBody.source_markdown).toContain("Platform Ops");
    expect(routerPush).toHaveBeenCalledWith("/agents/zom_new");
    expect(routerRefresh).not.toHaveBeenCalled();
    expect(captureProductEventMock).toHaveBeenCalledTimes(1);
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.agent_created, { agent_id: "zom_new" });
  });

  it("409 conflict renders a name-collision hint", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 409,
      statusText: "Conflict",
      json: async () => ({ detail: "dup", error_code: "UZ-ZOM-002" }),
    });
    const user = userEvent.setup({ delay: null });
    await renderForm();
    await user.type(screen.getByLabelText(/TRIGGER\.md body/i), FIXTURE_TRIGGER);
    await user.type(screen.getByLabelText(/SKILL\.md body/i), FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /install teammate/i }));
    await waitFor(() =>
      expect(screen.getByText(/already exists in this workspace/i)).toBeTruthy(),
    );
    expect(routerPush).not.toHaveBeenCalled();
    expect(captureProductEventMock).not.toHaveBeenCalled();
  });

  it("non-409 errors render the raw error message", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 500,
      statusText: "Server Error",
      json: async () => ({ detail: "boom", error_code: "UZ-SRV" }),
    });
    const user = userEvent.setup({ delay: null });
    await renderForm();
    await user.type(screen.getByLabelText(/TRIGGER\.md body/i), FIXTURE_TRIGGER);
    await user.type(screen.getByLabelText(/SKILL\.md body/i), FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /install teammate/i }));
    await waitFor(() =>
      expect(screen.getByText(/boom/)).toBeTruthy(),
    );
  });

  it("missing token surfaces Not authenticated", async () => {
    // Server-side auth() returns no token → installAgentAction returns
    // { ok: false, status: 401 }; the form surfaces it as the api-error alert.
    auth.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const user = userEvent.setup({ delay: null });
    await renderForm();
    await user.type(screen.getByLabelText(/TRIGGER\.md body/i), FIXTURE_TRIGGER);
    await user.type(screen.getByLabelText(/SKILL\.md body/i), FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /install teammate/i }));
    // Same UZ-AUTH-401 mapping — "Your session expired" copy in the alert.
    await waitFor(() =>
      expect(screen.getByText(/Your session expired/i)).toBeTruthy(),
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("Cancel button navigates back to /agents", async () => {
    const user = userEvent.setup({ delay: null });
    await renderForm();
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(routerPush).toHaveBeenCalledWith("/agents");
  });
});
