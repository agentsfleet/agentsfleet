import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

const redirect = vi.fn((path: string) => {
  throw new Error(`redirect:${path}`);
});
const notFound = vi.fn(() => {
  throw new Error("notFound");
});
const usePathname = vi.fn(() => "/workspaces");
const auth = vi.fn();
const useUser = vi.fn(() => ({ isLoaded: false, user: null }));
const listWorkspaces = vi.fn();
const getWorkspace = vi.fn();
const listRuns = vi.fn();
const getRun = vi.fn();
const listRunTransitions = vi.fn();

vi.mock("next/navigation", () => ({
  redirect,
  notFound,
  usePathname,
}));

vi.mock("@clerk/nextjs/server", () => ({
  auth,
}));

vi.mock("@clerk/nextjs", () => ({
  ClerkProvider: ({ children }: { children: React.ReactNode }) => React.createElement(React.Fragment, null, children),
  UserButton: () => React.createElement("div", { "data-user-button": "1" }),
  SignIn: ({ appearance }: { appearance: unknown }) => React.createElement("div", { "data-sign-in": JSON.stringify(appearance) }),
  SignUp: ({ appearance }: { appearance: unknown }) => React.createElement("div", { "data-sign-up": JSON.stringify(appearance) }),
  useUser,
}));

vi.mock("@/lib/api", () => ({
  listWorkspaces,
  getWorkspace,
  listRuns,
  getRun,
  listRunTransitions,
}));

vi.mock("@/components/domain/WorkspaceCard", () => ({
  default: ({ workspace }: { workspace: { id: string } }) =>
    React.createElement("div", { "data-workspace-card": workspace.id }),
}));

vi.mock("@/components/domain/RunRow", () => ({
  default: ({ run }: { run: { id: string } }) =>
    React.createElement("div", { "data-run-row": run.id }),
}));

vi.mock("@/components/domain/PipelineStage", () => ({
  default: ({ stage, state }: { stage: string; state: string }) =>
    React.createElement("div", { "data-pipeline-stage": `${stage}:${state}` }),
}));

vi.mock("@/components/domain/RunStatus", () => ({
  default: ({ status }: { status: string }) =>
    React.createElement("div", { "data-run-status": status }),
}));

vi.mock("@/components/analytics/AnalyticsPageEvent", () => ({
  default: ({ event }: { event: string }) =>
    React.createElement("div", { "data-analytics-event": event }),
}));

vi.mock("@/components/analytics/TrackedAnchor", () => ({
  default: ({ href, children, event }: { href: string; children: React.ReactNode; event: string }) =>
    React.createElement("a", { href, "data-track-event": event }, children),
}));

vi.mock("lucide-react", () => ({
  PlusIcon: () => React.createElement("svg", { "data-icon": "PlusIcon" }),
  PauseIcon: () => React.createElement("svg", { "data-icon": "PauseIcon" }),
  PlayIcon: () => React.createElement("svg", { "data-icon": "PlayIcon" }),
  RefreshCwIcon: () => React.createElement("svg", { "data-icon": "RefreshCwIcon" }),
  ArrowLeftIcon: () => React.createElement("svg", { "data-icon": "ArrowLeftIcon" }),
  ExternalLinkIcon: () => React.createElement("svg", { "data-icon": "ExternalLinkIcon" }),
  LayoutDashboardIcon: () => React.createElement("svg", { "data-icon": "LayoutDashboardIcon" }),
  BoxIcon: () => React.createElement("svg", { "data-icon": "BoxIcon" }),
  ActivityIcon: () => React.createElement("svg", { "data-icon": "ActivityIcon" }),
  SettingsIcon: () => React.createElement("svg", { "data-icon": "SettingsIcon" }),
  BookOpenIcon: () => React.createElement("svg", { "data-icon": "BookOpenIcon" }),
  ZapIcon: () => React.createElement("svg", { "data-icon": "ZapIcon" }),
}));

beforeEach(() => {
  vi.clearAllMocks();
  auth.mockResolvedValue({
    getToken: vi.fn().mockResolvedValue("token_123"),
  });
});

describe("app layouts and pages", () => {
  it("wraps root and dashboard layouts and renders auth layout branding", async () => {
    const { default: RootLayout } = await import("../app/layout");
    const { default: DashboardLayout } = await import("../app/(dashboard)/layout");
    const { default: AuthLayout } = await import("../app/(auth)/layout");

    const rootMarkup = renderToStaticMarkup(React.createElement(RootLayout, null, React.createElement("div", null, "root child")));
    const dashboardMarkup = renderToStaticMarkup(
      React.createElement(DashboardLayout, null, React.createElement("div", null, "dash child")),
    );
    const authMarkup = renderToStaticMarkup(React.createElement(AuthLayout, null, React.createElement("div", null, "auth child")));

    expect(rootMarkup).toContain("root child");
    expect(dashboardMarkup).toContain("dash child");
    expect(authMarkup).toContain("UseZombie");
    expect(authMarkup).toContain("Mission Control");
  });

  it("redirects root and dashboard entry pages to workspaces", async () => {
    const { default: RootPage } = await import("../app/page");
    const { default: DashboardPage } = await import("../app/(dashboard)/page");

    expect(() => RootPage()).toThrow("redirect:/workspaces");
    expect(() => DashboardPage()).toThrow("redirect:/workspaces");
    expect(redirect).toHaveBeenCalledWith("/workspaces");
  });

  it("renders sign-in and sign-up pages with shared auth appearance", async () => {
    const { default: SignInPage } = await import("../app/(auth)/sign-in/[[...sign-in]]/page");
    const { default: SignUpPage } = await import("../app/(auth)/sign-up/[[...sign-up]]/page");

    const signInMarkup = renderToStaticMarkup(React.createElement(SignInPage));
    const signUpMarkup = renderToStaticMarkup(React.createElement(SignUpPage));

    expect(signInMarkup).toContain("data-sign-in=");
    expect(signUpMarkup).toContain("data-sign-up=");
    expect(signInMarkup).toContain("colorPrimary");
    expect(signUpMarkup).toContain("colorPrimary");
  });

  it("renders the workspaces page for anonymous users without workspace data", async () => {
    auth.mockResolvedValueOnce({
      getToken: vi.fn().mockResolvedValue(null),
    });

    const { default: WorkspacesPage } = await import("../app/(dashboard)/workspaces/page");
    const anonMarkup = renderToStaticMarkup(await WorkspacesPage());

    expect(anonMarkup).toContain("workspace_list_viewed");
    expect(anonMarkup).not.toContain("data-workspace-card=");
  });

  it("renders the workspaces page with empty, error, and populated states", async () => {
    listWorkspaces
      .mockResolvedValueOnce({ data: [] })
      .mockRejectedValueOnce(new Error("backend unavailable"))
      .mockResolvedValueOnce({
        data: [
          {
            id: "ws_1",
            name: "Alpha",
            repo_url: "https://github.com/usezombie/repo",
            paused: false,
            created_at: "2026-03-22T00:00:00Z",
            run_count: 1,
            last_run_at: null,
            plan: "hobby",
          },
        ],
      });

    const { default: WorkspacesPage } = await import("../app/(dashboard)/workspaces/page");
    const emptyMarkup = renderToStaticMarkup(await WorkspacesPage());
    const errorMarkup = renderToStaticMarkup(await WorkspacesPage());
    const populatedMarkup = renderToStaticMarkup(await WorkspacesPage());

    expect(emptyMarkup).toContain("workspace_list_viewed");
    expect(emptyMarkup).toContain("No workspaces yet");
    expect(errorMarkup).toContain("workspace_list_failed");
    expect(errorMarkup).toContain("backend unavailable");
    expect(populatedMarkup).toContain("data-workspace-card=\"ws_1\"");
    expect(populatedMarkup).toContain("workspace_add_docs_clicked");
  });

  it("renders workspace detail page with active run and empty recent runs branches", async () => {
    getWorkspace.mockResolvedValue({
      id: "ws_1",
      name: "Alpha",
      repo_url: "https://github.com/usezombie/repo",
      paused: false,
      created_at: "2026-03-22T00:00:00Z",
      run_count: 2,
      last_run_at: null,
      plan: "team",
    });
    listRuns
      .mockResolvedValueOnce({
        data: [
          { id: "run_active", status: "PATCH_IN_PROGRESS" },
          { id: "run_done", status: "DONE" },
        ],
      })
      .mockResolvedValueOnce({
        data: [
          { id: "run_failed", status: "FAILED" },
        ],
      })
      .mockResolvedValueOnce({ data: [] });

    const { default: WorkspacePage } = await import("../app/(dashboard)/workspaces/[id]/page");

    const populated = renderToStaticMarkup(await WorkspacePage({ params: Promise.resolve({ id: "ws_1" }) }));
    const noActive = renderToStaticMarkup(await WorkspacePage({ params: Promise.resolve({ id: "ws_1" }) }));
    const empty = renderToStaticMarkup(await WorkspacePage({ params: Promise.resolve({ id: "ws_1" }) }));

    expect(populated).toContain("workspace_detail_viewed");
    expect(populated).toContain("data-run-row=\"run_active\"");
    expect(populated).toContain("workspace_action_clicked");
    expect(noActive).not.toContain("Active run");
    expect(empty).toContain("No runs yet");
  });

  it("renders run detail page with retry, PR, artifacts, and error event branches", async () => {
    getRun
      .mockResolvedValueOnce({
        id: "run_1",
        workspace_id: "ws_1",
        spec_path: "specs/demo.md",
        status: "FAILED",
        attempts: 2,
        max_attempts: 3,
        created_at: "2026-03-22T00:00:00Z",
        updated_at: "2026-03-22T01:00:00Z",
        duration_seconds: 90,
        pr_url: "https://github.com/usezombie/repo/pull/1",
        artifacts: {
          plan: "plan.md",
          implementation: null,
          validation: "validation.md",
          summary: "summary.md",
          defect_report: null,
        },
        error: "build failed",
      })
      .mockResolvedValueOnce({
        id: "run_2",
        workspace_id: "ws_1",
        spec_path: "specs/clean.md",
        status: "DONE",
        attempts: 1,
        max_attempts: 1,
        created_at: "2026-03-22T00:00:00Z",
        updated_at: "2026-03-22T00:30:00Z",
        duration_seconds: null,
        pr_url: null,
        artifacts: null,
        error: null,
      });
    listRunTransitions
      .mockResolvedValueOnce([
        {
          id: "rt_1",
          run_id: "run_1",
          from_status: "PATCH_IN_PROGRESS",
          to_status: "FAILED",
          reason: "tests failed",
          actor: "worker",
          created_at: "2026-03-22T01:00:00Z",
        },
      ])
      .mockResolvedValueOnce([
        {
          id: "rt_2",
          run_id: "run_2",
          from_status: null,
          to_status: "DONE",
          reason: "completed",
          actor: "worker",
          created_at: "2026-03-22T00:30:00Z",
        },
      ]);

    const { default: RunDetailPage } = await import("../app/(dashboard)/workspaces/[id]/runs/[runId]/page");

    const markup = renderToStaticMarkup(
      await RunDetailPage({ params: Promise.resolve({ id: "ws_1", runId: "run_1" }) }),
    );
    const cleanMarkup = renderToStaticMarkup(
      await RunDetailPage({ params: Promise.resolve({ id: "ws_1", runId: "run_2" }) }),
    );

    expect(markup).toContain("run_detail_viewed");
    expect(markup).toContain("run_error_viewed");
    expect(markup).toContain("run_retry_clicked");
    expect(markup).toContain("run_pr_clicked");
    expect(markup).toContain("tests failed");
    expect(markup).toContain("plan.md");
    expect(cleanMarkup).not.toContain("run_error_viewed");
    expect(cleanMarkup).not.toContain("run_retry_clicked");
    expect(cleanMarkup).not.toContain("run_pr_clicked");
    expect(cleanMarkup).toContain("completed");
  });

  it("calls notFound when workspace and run pages lose auth or data lookup fails", async () => {
    auth.mockResolvedValueOnce({
      getToken: vi.fn().mockResolvedValue(null),
    });
    getWorkspace.mockRejectedValueOnce(new Error("boom"));
    getRun.mockRejectedValueOnce(new Error("boom"));

    const { default: WorkspacePage } = await import("../app/(dashboard)/workspaces/[id]/page");
    const { default: RunDetailPage } = await import("../app/(dashboard)/workspaces/[id]/runs/[runId]/page");

    await expect(WorkspacePage({ params: Promise.resolve({ id: "ws_1" }) })).rejects.toThrow("notFound");
    await expect(RunDetailPage({ params: Promise.resolve({ id: "ws_1", runId: "run_1" }) })).rejects.toThrow("notFound");
    expect(notFound).toHaveBeenCalled();
  });
});
