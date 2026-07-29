import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

type Snapshot = {
  error: unknown;
  module: { default: React.ComponentType<Record<string, unknown>> } | null;
  status: "idle" | "loading" | "ready" | "error";
};

type TestLoader = {
  importModule: () => Promise<unknown>;
  preload: ReturnType<typeof vi.fn>;
  retry: ReturnType<typeof vi.fn>;
  snapshot: Snapshot;
};

const state = vi.hoisted(() => ({
  loaders: [] as TestLoader[],
  maySpeculate: true,
  pathname: "/",
  renderedProps: null as Record<string, unknown> | null,
  setAnalyticsContext: vi.fn(),
}));

vi.mock("./intent-module-loader", () => ({
  INTENT_MODULE_STATUS: {
    idle: "idle",
    loading: "loading",
    ready: "ready",
    error: "error",
  },
  createIntentModuleLoader: (importModule: () => Promise<unknown>) => {
    const loader: TestLoader = {
      importModule,
      preload: vi.fn(() => Promise.resolve(loader.snapshot.module)),
      retry: vi.fn(() => Promise.resolve(loader.snapshot.module)),
      snapshot: { error: null, module: null, status: "idle" },
    };
    state.loaders.push(loader);
    return loader;
  },
  maySpeculateOnHover: () => state.maySpeculate,
  useIntentModule: (loader: TestLoader) => loader.snapshot,
}));

vi.mock("@agentsfleet/design-system", async () => {
  const ReactModule = await import("react");
  function Button({
    children,
    size: _size,
    variant: _variant,
    ...props
  }: React.ButtonHTMLAttributes<HTMLButtonElement> & {
    size?: string;
    variant?: string;
  }) {
    return ReactModule.createElement("button", props, children);
  }
  function WakePulse(props: React.HTMLAttributes<HTMLSpanElement>) {
    return ReactModule.createElement("span", props);
  }
  function Spinner({ srLabel }: { srLabel?: string }) {
    return ReactModule.createElement("span", { role: "status" }, srLabel);
  }
  return { Button, Spinner, WakePulse };
});

vi.mock("next/navigation", () => ({
  usePathname: () => state.pathname,
}));

vi.mock("next/link", async () => {
  const ReactModule = await import("react");
  return {
    default: ({
      children,
      ...props
    }: React.PropsWithChildren<React.AnchorHTMLAttributes<HTMLAnchorElement>>) =>
      ReactModule.createElement("a", props, children),
  };
});

vi.mock("@/lib/analytics/posthog", () => ({
  setAnalyticsContext: state.setAnalyticsContext,
}));

function loadedProbe(props: Record<string, unknown>) {
  state.renderedProps = props;
  return React.createElement("div", null, "loaded controls");
}

function latestLoader(): TestLoader {
  const loader = state.loaders.at(-1);
  if (!loader) throw new Error("expected wrapper to create an intent loader");
  return loader;
}

function showLoaded(loader: TestLoader) {
  loader.snapshot = {
    error: null,
    module: { default: loadedProbe },
    status: "ready",
  };
}

function showError(loader: TestLoader) {
  loader.snapshot = {
    error: new Error("chunk unavailable"),
    module: null,
    status: "error",
  };
}

beforeEach(() => {
  state.loaders = [];
  state.maySpeculate = true;
  state.pathname = "/";
  state.renderedProps = null;
  state.setAnalyticsContext.mockReset();
  vi.resetModules();
});

afterEach(cleanup);

describe("shell intent controls", () => {
  it("renders the workspace trigger defaults without loading its menu", async () => {
    const { WorkspaceSwitcherTrigger } = await import(
      "@/components/layout/WorkspaceSwitcherTrigger"
    );
    render(<WorkspaceSwitcherTrigger activeLabel="Default workspace" />);
    const trigger = screen.getByRole("button", {
      name: "Default workspace",
    });
    expect(trigger.getAttribute("aria-busy")).toBeNull();
    expect(trigger.textContent).toContain("Default workspace");
  });

  it("preserves mobile navigation through capability checks and retry", async () => {
    const { ShellControls } = await import("@/components/layout/ShellControls");
    const loader = latestLoader();
    expect(await loader.importModule()).toHaveProperty("default");
    const view = render(
      <ShellControls
        workspaces={[]}
        operatorScopes={["runner:read"]}
        sidebarNavId="test-sidebar"
      />,
    );
    const trigger = screen.getByRole("button", { name: "Open navigation" });
    expect(
      screen.getByRole("link", { name: "agentsfleet home" }).getAttribute("href"),
    ).toBe("/");
    expect(state.setAnalyticsContext).toHaveBeenCalledWith({
      workspaceCount: 0,
      workspaceId: null,
    });

    trigger.focus();
    await userEvent.hover(trigger);
    expect(loader.preload).toHaveBeenCalledTimes(2);
    state.maySpeculate = false;
    await userEvent.unhover(trigger);
    await userEvent.hover(trigger);
    expect(loader.preload).toHaveBeenCalledTimes(2);

    await userEvent.click(trigger);
    loader.snapshot = { error: null, module: null, status: "loading" };
    view.rerender(
      <ShellControls
        workspaces={[]}
        operatorScopes={["runner:read"]}
        sidebarNavId="test-sidebar"
      />,
    );
    expect(trigger.getAttribute("aria-busy")).toBe("true");

    showError(loader);
    view.rerender(
      <ShellControls
        workspaces={[]}
        operatorScopes={["runner:read"]}
        sidebarNavId="test-sidebar"
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Retry navigation" }),
    );
    expect(loader.retry).toHaveBeenCalledOnce();

    state.pathname = "/w/ws_active/events";
    showLoaded(loader);
    view.rerender(
      <ShellControls
        workspaces={[{ id: "ws_fallback", name: "Fallback", created_at: 1 }]}
        operatorScopes={["runner:read"]}
        sidebarNavId="test-sidebar"
      />,
    );
    await waitFor(() =>
      expect(state.renderedProps).toMatchObject({
        open: false,
        operatorScopes: ["runner:read"],
        pathname: "/w/ws_active/events",
        workspaceId: "ws_active",
      }),
    );
    const restoreFocus = state.renderedProps?.restoreFocus;
    expect(typeof restoreFocus).toBe("function");
    if (typeof restoreFocus === "function") restoreFocus();
    expect(document.activeElement).toBe(trigger);
  });

  it("keeps the workspace trigger stable through preload, failure, and load", async () => {
    const { default: WorkspaceSwitcher } = await import(
      "@/components/layout/WorkspaceSwitcher"
    );
    const loader = latestLoader();
    expect(await loader.importModule()).toHaveProperty("default");
    const view = render(
      <WorkspaceSwitcher
        workspaces={[{ id: "ws_fallback", name: "Fallback", created_at: 1 }]}
      />,
    );
    const trigger = screen.getByTestId("workspace-switcher");
    expect(trigger.textContent).toContain("Fallback");

    trigger.focus();
    await userEvent.hover(trigger);
    expect(loader.preload).toHaveBeenCalledTimes(2);
    state.maySpeculate = false;
    await userEvent.unhover(trigger);
    await userEvent.hover(trigger);
    expect(loader.preload).toHaveBeenCalledTimes(2);

    await userEvent.click(trigger);
    loader.snapshot = { error: null, module: null, status: "loading" };
    view.rerender(
      <WorkspaceSwitcher
        workspaces={[{ id: "ws_fallback", name: "Fallback", created_at: 1 }]}
      />,
    );
    expect(trigger.getAttribute("aria-busy")).toBe("true");

    state.pathname = "/w/ws_missing/fleets";
    view.rerender(
      <WorkspaceSwitcher
        workspaces={[{ id: "ws_fallback", name: "Fallback", created_at: 1 }]}
      />,
    );
    await waitFor(() =>
      expect(
        screen.getByTestId("workspace-switcher").getAttribute("aria-busy"),
      ).not.toBe("true"),
    );

    showError(loader);
    view.rerender(
      <WorkspaceSwitcher
        workspaces={[{ id: "ws_fallback", name: "Fallback", created_at: 1 }]}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Retry workspace menu" }),
    );
    expect(loader.retry).toHaveBeenCalledOnce();

    showLoaded(loader);
    view.rerender(
      <WorkspaceSwitcher
        workspaces={[{ id: "ws_fallback", name: "Fallback", created_at: 1 }]}
      />,
    );
    expect(state.renderedProps).toMatchObject({ open: true });

    state.pathname = "/w/ws_missing/events";
    view.rerender(
      <WorkspaceSwitcher
        workspaces={[{ id: "ws_fallback", name: "Fallback", created_at: 1 }]}
      />,
    );
    await waitFor(() =>
      expect(screen.getByTestId("workspace-switcher")).toBeTruthy(),
    );
  });
});
