import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

const mocks = vi.hoisted(() => ({
  trackAppEvent: vi.fn(),
  trackNavigationClicked: vi.fn(),
  identifyAnalyticsUser: vi.fn(),
  useUser: vi.fn(),
  usePathname: vi.fn(),
  useEffectMock: vi.fn((fn: () => void) => fn()),
}));

vi.mock("react", async () => {
  const actual = await vi.importActual<typeof import("react")>("react");
  return { ...actual, useEffect: mocks.useEffectMock };
});

vi.mock("@/lib/analytics/posthog", () => ({
  trackAppEvent: mocks.trackAppEvent,
  trackNavigationClicked: mocks.trackNavigationClicked,
  identifyAnalyticsUser: mocks.identifyAnalyticsUser,
}));

vi.mock("@clerk/nextjs", () => ({
  UserButton: () => React.createElement("div", { "data-user-button": "1" }),
  useUser: mocks.useUser,
}));

vi.mock("next/navigation", () => ({
  usePathname: mocks.usePathname,
}));

vi.mock("next/link", () => ({
  default: ({ children, ...props }: React.PropsWithChildren<React.AnchorHTMLAttributes<HTMLAnchorElement>>) =>
    React.createElement("a", props, children),
}));

vi.mock("lucide-react", () => ({
  GitBranchIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "GitBranchIcon" }),
  ActivityIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ActivityIcon" }),
  PauseIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "PauseIcon" }),
  ExternalLinkIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ExternalLinkIcon" }),
  LayoutDashboardIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "LayoutDashboardIcon" }),
  BoxIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "BoxIcon" }),
  SettingsIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "SettingsIcon" }),
  BookOpenIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "BookOpenIcon" }),
  ZapIcon: (props: Record<string, unknown>) => React.createElement("svg", { ...props, "data-icon": "ZapIcon" }),
}));

type ClickableElement = React.ReactElement<{ children?: React.ReactNode; onClick?: (...args: unknown[]) => unknown }>;

function findElements(node: React.ReactNode, matcher: (element: ClickableElement) => boolean): ClickableElement[] {
  const results: ClickableElement[] = [];
  function walk(value: React.ReactNode) {
    if (!React.isValidElement(value)) return;
    const element = value as ClickableElement;
    if (matcher(element)) results.push(element);
    const children = element.props?.children;
    if (Array.isArray(children)) {
      children.forEach(walk);
      return;
    }
    if (children) walk(children);
  }
  walk(node);
  return results;
}

beforeEach(() => {
  mocks.useUser.mockReset();
  mocks.usePathname.mockReset();
  mocks.trackAppEvent.mockReset();
  mocks.trackNavigationClicked.mockReset();
  mocks.identifyAnalyticsUser.mockReset();
  mocks.useEffectMock.mockClear();
  mocks.usePathname.mockReturnValue("/workspaces");
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("app components", () => {
  it("renders pipeline stages with state icon and optional connector", async () => {
    const { default: PipelineStage } = await import("../components/domain/PipelineStage");
    const activeMarkup = renderToStaticMarkup(
      React.createElement(PipelineStage, {
        stage: "PATCH_IN_PROGRESS",
        state: "active",
        showConnector: true,
      }),
    );
    const pendingMarkup = renderToStaticMarkup(
      React.createElement(PipelineStage, {
        stage: "CUSTOM_STAGE" as never,
        state: "pending",
        showConnector: false,
      }),
    );

    expect(activeMarkup).toContain("Patch");
    expect(activeMarkup).toContain("pipeline-connector");
    expect(pendingMarkup).toContain("CUSTOM_STAGE");
    expect(pendingMarkup).toContain("○");
  });

  it("renders run statuses with configured and fallback labels", async () => {
    const { default: RunStatus } = await import("../components/domain/RunStatus");
    const known = renderToStaticMarkup(React.createElement(RunStatus, { status: "FAILED", size: "sm" }));
    const fallback = renderToStaticMarkup(React.createElement(RunStatus, { status: "WAITING" }));

    expect(known).toContain("Failed");
    expect(known).toContain("status-failed");
    expect(fallback).toContain("WAITING");
    expect(fallback).toContain("status-pending");
  });

  it("tracks workspace and run interactions from domain rows", async () => {
    const { default: WorkspaceCard } = await import("../components/domain/WorkspaceCard");
    const { default: RunRow } = await import("../components/domain/RunRow");

    const workspaceTree = WorkspaceCard({
      workspace: {
        id: "ws_1",
        name: "Alpha",
        repo_url: "https://github.com/usezombie/repo",
        paused: true,
        created_at: "2026-03-22T00:00:00Z",
        run_count: 7,
        last_run_at: "2026-03-22T00:00:00Z",
        plan: "pro",
      },
    });
    const runTree = RunRow({
      workspaceId: "ws_1",
      run: {
        id: "run_1",
        workspace_id: "ws_1",
        spec_path: "specs/demo.md",
        status: "PR_OPENED",
        attempts: 1,
        max_attempts: 3,
        created_at: "2026-03-22T00:00:00Z",
        updated_at: "2026-03-22T00:00:00Z",
        duration_seconds: 125,
        pr_url: "https://github.com/usezombie/repo/pull/1",
        artifacts: null,
        error: null,
      },
    });

    findElements(workspaceTree, (el) => typeof el.props?.onClick === "function")[0]?.props.onClick?.();
    const runLinks = findElements(runTree, (el) => typeof el.props?.onClick === "function");
    runLinks[0]?.props.onClick?.();
    runLinks[1]?.props.onClick?.({ stopPropagation: vi.fn() });

    expect(mocks.trackAppEvent).toHaveBeenCalledWith("workspace_opened", expect.objectContaining({
      workspace_id: "ws_1",
      workspace_plan: "pro",
      paused: true,
    }));
    expect(mocks.trackAppEvent).toHaveBeenCalledWith("run_opened", expect.objectContaining({
      workspace_id: "ws_1",
      run_id: "run_1",
      run_status: "PR_OPENED",
    }));
    expect(mocks.trackAppEvent).toHaveBeenCalledWith("run_pr_clicked", expect.objectContaining({
      target: "https://github.com/usezombie/repo/pull/1",
    }));

    const nonPausedMarkup = renderToStaticMarkup(WorkspaceCard({
      workspace: {
        id: "ws_2",
        name: "Beta",
        repo_url: "https://github.com/usezombie/repo",
        paused: false,
        created_at: "2026-03-22T00:00:00Z",
        run_count: 0,
        last_run_at: null,
        plan: "team",
      },
    }));
    const noPrMarkup = renderToStaticMarkup(RunRow({
      workspaceId: "ws_2",
      run: {
        id: "run_2",
        workspace_id: "ws_2",
        spec_path: "specs/short.md",
        status: "DONE",
        attempts: 1,
        max_attempts: 1,
        created_at: "2026-03-22T00:00:00Z",
        updated_at: "2026-03-22T00:00:00Z",
        duration_seconds: null,
        pr_url: null,
        artifacts: null,
        error: null,
      },
    }));

    expect(nonPausedMarkup).not.toContain("Paused");
    expect(noPrMarkup).toContain("—");
  });

  it("tracks analytics wrapper components and shell navigation", async () => {
    const { default: AnalyticsPageEvent } = await import("../components/analytics/AnalyticsPageEvent");
    const { default: TrackedAnchor } = await import("../components/analytics/TrackedAnchor");
    const { default: Shell } = await import("../components/layout/Shell");

    AnalyticsPageEvent({ event: "workspace_list_viewed", properties: { surface: "workspace_list" } });
    const trackedAnchor = TrackedAnchor({
      event: "workspace_action_clicked",
      properties: { target: "pause" },
      onClick: vi.fn(),
      href: "/workspaces/ws_1/pause",
      children: "Pause",
    });
    trackedAnchor.props.onClick?.({ type: "click" } as unknown as React.MouseEvent<HTMLAnchorElement>);

    const shellTree = Shell({ children: React.createElement("div", null, "content") });
    const clickable = findElements(shellTree, (el) => typeof el.props?.onClick === "function");
    clickable.forEach((el) => el.props.onClick?.());

    mocks.usePathname.mockReturnValue("/");
    renderToStaticMarkup(React.createElement(Shell, null, React.createElement("div", null, "root")));

    expect(mocks.trackAppEvent).toHaveBeenCalledWith("workspace_list_viewed", { surface: "workspace_list" });
    expect(mocks.trackAppEvent).toHaveBeenCalledWith("workspace_action_clicked", { target: "pause" });
    expect(mocks.trackNavigationClicked).toHaveBeenCalled();
    expect(renderToStaticMarkup(React.createElement(React.Fragment, null, shellTree))).toContain("Mission Control");
  });

  it("identifies the current clerk user once loaded", async () => {
    mocks.useUser.mockReturnValue({
      isLoaded: true,
      user: {
        id: "user_123",
        primaryEmailAddress: { emailAddress: "kishore@example.com" },
      },
    });

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    const tree = AnalyticsBootstrap();

    expect(tree).toBeNull();
    expect(mocks.identifyAnalyticsUser).toHaveBeenCalledWith({
      id: "user_123",
      email: "kishore@example.com",
    });
  });

  it("does not identify until clerk user data is ready", async () => {
    mocks.useUser.mockReturnValue({
      isLoaded: false,
      user: null,
    });

    const { default: AnalyticsBootstrap } = await import("../components/analytics/AnalyticsBootstrap");

    AnalyticsBootstrap();

    expect(mocks.identifyAnalyticsUser).not.toHaveBeenCalled();
  });

  it("exports stable auth appearance tokens", async () => {
    const { AUTH_APPEARANCE } = await import("../lib/clerkAppearance");

    expect(AUTH_APPEARANCE.variables.colorPrimary).toBe("var(--z-orange)");
    expect(AUTH_APPEARANCE.elements.formButtonPrimary.color).toBe("var(--z-text-inverse)");
    expect(AUTH_APPEARANCE.elements.footer.background).toContain("linear-gradient");
  });
});
