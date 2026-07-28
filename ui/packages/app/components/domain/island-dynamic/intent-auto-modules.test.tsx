import React from "react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Snapshot = {
  error: unknown;
  module: { default: React.ComponentType<Record<string, unknown>> } | null;
  status: "idle" | "loading" | "ready" | "error";
};

type TestLoader = {
  preload: ReturnType<typeof vi.fn>;
  retry: ReturnType<typeof vi.fn>;
  snapshot: Snapshot;
};

const state = vi.hoisted(() => ({
  loaders: [] as TestLoader[],
  renderedProps: null as Record<string, unknown> | null,
}));

vi.mock("./intent-module-loader", () => ({
  INTENT_MODULE_STATUS: {
    idle: "idle",
    loading: "loading",
    ready: "ready",
    error: "error",
  },
  createIntentModuleLoader: () => {
    const loader: TestLoader = {
      preload: vi.fn(() => Promise.resolve(loader.snapshot.module)),
      retry: vi.fn(() => Promise.resolve(loader.snapshot.module)),
      snapshot: { error: null, module: null, status: "idle" },
    };
    state.loaders.push(loader);
    return loader;
  },
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
  function Spinner({ srLabel }: { srLabel?: string }) {
    return ReactModule.createElement("span", { role: "status" }, srLabel);
  }
  return { Button, Spinner };
});

function latestLoader(): TestLoader {
  const loader = state.loaders.at(-1);
  if (!loader) throw new Error("expected an automatic module loader");
  return loader;
}

function showError(loader: TestLoader) {
  loader.snapshot = {
    error: new Error("chunk unavailable"),
    module: null,
    status: "error",
  };
}

function showLoaded(loader: TestLoader) {
  loader.snapshot = {
    error: null,
    module: {
      default: (props) => {
        state.renderedProps = props;
        return React.createElement("div", null, "loaded island");
      },
    },
    status: "ready",
  };
}

beforeEach(() => {
  state.loaders = [];
  state.renderedProps = null;
  vi.resetModules();
});

afterEach(cleanup);

describe("automatic shell islands", () => {
  it("contains account-menu loading failure and preserves retry", async () => {
    const { default: ClientOnlyAuthUserButton } = await import(
      "@/components/layout/ClientOnlyAuthUserButton"
    );
    const loader = latestLoader();
    const view = render(<ClientOnlyAuthUserButton />);
    expect(
      (
        screen.getByRole("button", {
          name: "Loading account menu",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(loader.preload).toHaveBeenCalledOnce();

    showError(loader);
    view.rerender(<ClientOnlyAuthUserButton />);
    await userEvent.click(
      screen.getByRole("button", { name: "Retry account menu" }),
    );
    expect(loader.retry).toHaveBeenCalledOnce();

    showLoaded(loader);
    view.rerender(<ClientOnlyAuthUserButton />);
    expect(screen.getByText("loaded island")).toBeTruthy();
  });

  it("isolates checklist failure and forwards props after recovery", async () => {
    const { default: GettingStartedWidgetDynamic } = await import(
      "./GettingStartedWidgetDynamic"
    );
    const loader = latestLoader();
    const view = render(
      <GettingStartedWidgetDynamic workspaceId="ws_1" pollingMode="desktop" />,
    );
    expect(view.container.textContent).toBe("");
    expect(loader.preload).toHaveBeenCalledOnce();

    showError(loader);
    view.rerender(
      <GettingStartedWidgetDynamic workspaceId="ws_1" pollingMode="desktop" />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Retry getting started" }),
    );
    expect(loader.retry).toHaveBeenCalledOnce();

    showLoaded(loader);
    view.rerender(
      <GettingStartedWidgetDynamic workspaceId="ws_1" pollingMode="desktop" />,
    );
    expect(state.renderedProps).toMatchObject({
      pollingMode: "desktop",
      workspaceId: "ws_1",
    });
  });
});
