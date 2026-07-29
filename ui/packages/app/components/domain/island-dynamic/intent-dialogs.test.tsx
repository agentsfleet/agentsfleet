import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
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
  renderedProps: null as Record<string, unknown> | null,
}));
const CREATE_FLEET_LIBRARY_LABEL = "Create fleet library";
const RETRY_CREATE_FLEET_LIBRARY_LABEL = `Retry ${CREATE_FLEET_LIBRARY_LABEL}`;
const CLOSE_LABEL = "Close";
const RETRY_LABEL = "Retry";

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
    tooltip: _tooltip,
    variant: _variant,
    ...props
  }: React.ButtonHTMLAttributes<HTMLButtonElement> & {
    size?: string;
    tooltip?: string;
    variant?: string;
  }) {
    return ReactModule.createElement("button", props, children);
  }
  function Dialog({
    children,
    open,
  }: React.PropsWithChildren<{ open?: boolean }>) {
    return open === false ? null : ReactModule.createElement(ReactModule.Fragment, null, children);
  }
  function Element({
    children,
    ...props
  }: React.PropsWithChildren<React.HTMLAttributes<HTMLDivElement>>) {
    return ReactModule.createElement("div", props, children);
  }
  function DialogContent({
    children,
    ...props
  }: React.PropsWithChildren<React.HTMLAttributes<HTMLDivElement>>) {
    return ReactModule.createElement("div", { role: "dialog", ...props }, children);
  }
  function DialogTitle({
    children,
  }: React.PropsWithChildren) {
    return ReactModule.createElement("h2", null, children);
  }
  function DialogDescription({
    children,
    ...props
  }: React.PropsWithChildren<React.HTMLAttributes<HTMLParagraphElement>>) {
    return ReactModule.createElement("p", props, children);
  }
  function Spinner({
    label,
    srLabel,
  }: {
    label?: React.ReactNode;
    srLabel?: string;
  }) {
    return ReactModule.createElement("span", { role: "status" }, label ?? srLabel);
  }
  return {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter: Element,
    DialogHeader: Element,
    DialogTitle,
    Spinner,
    TooltipButton: Button,
  };
});

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn() }),
}));

vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: vi.fn(),
}));

function loadedProbe(props: Record<string, unknown>) {
  state.renderedProps = props;
  return React.createElement(
    "button",
    {
      onClick: () =>
        (props.onOpenChange as ((open: boolean) => void) | undefined)?.(false),
      type: "button",
    },
    "loaded dialog",
  );
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
  state.renderedProps = null;
  vi.resetModules();
});

afterEach(cleanup);

describe("fleet-library intent dialogs", () => {
  it("opens loading status immediately when defaultOpen activates an unloaded dialog", async () => {
    const { default: AddLibraryDialog } = await import(
      "./AddLibraryDialogDynamic"
    );
    const loader = latestLoader();
    const view = render(
      <AddLibraryDialog workspaceId="ws_1" defaultOpen />,
    );

    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: CREATE_FLEET_LIBRARY_LABEL }),
    ).toBeNull();
    expect(loader.preload).toHaveBeenCalledOnce();

    showError(loader);
    view.rerender(<AddLibraryDialog workspaceId="ws_1" defaultOpen />);
    expect(screen.getByRole("alert")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: CLOSE_LABEL }));
    expect(
      screen.getByRole("button", {
        name: RETRY_CREATE_FLEET_LIBRARY_LABEL,
      }),
    ).toBeTruthy();
    await userEvent.click(
      screen.getByRole("button", {
        name: RETRY_CREATE_FLEET_LIBRARY_LABEL,
      }),
    );
    expect(loader.retry).toHaveBeenCalledOnce();
  });

  it("keeps the add form closed, preloads on open, retries, and forwards props", async () => {
    const { default: AddFleetDialog, preloadAddFleetDialog } = await import(
      "./AddFleetDialogDynamic"
    );
    const loader = latestLoader();
    expect(await loader.importModule()).toHaveProperty("default");
    const onOpenChange = vi.fn();
    const view = render(
      <AddFleetDialog
        open={false}
        onOpenChange={onOpenChange}
        prefillRepo="agentsfleet/example"
        prefillRef="main"
      />,
    );
    expect(view.container.textContent).toBe("");
    expect(loader.preload).not.toHaveBeenCalled();

    preloadAddFleetDialog();
    view.rerender(
      <AddFleetDialog
        open
        onOpenChange={onOpenChange}
        prefillRepo="agentsfleet/example"
        prefillRef="main"
      />,
    );
    expect(screen.getByText("Loading fleet library form…")).toBeTruthy();
    expect(loader.preload).toHaveBeenCalledTimes(2);

    showError(loader);
    view.rerender(<AddFleetDialog open onOpenChange={onOpenChange} />);
    expect(screen.getByRole("alert").textContent).toContain(
      "Could not load the fleet library form.",
    );
    await userEvent.click(screen.getByRole("button", { name: CLOSE_LABEL }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
    await userEvent.click(screen.getByRole("button", { name: RETRY_LABEL }));
    expect(loader.retry).toHaveBeenCalledOnce();

    showLoaded(loader);
    view.rerender(
      <AddFleetDialog
        open
        onOpenChange={onOpenChange}
        prefillRepo="agentsfleet/example"
        prefillRef="main"
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "loaded dialog" }));
    expect(state.renderedProps).toMatchObject({
      open: true,
      prefillRepo: "agentsfleet/example",
      prefillRef: "main",
    });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("loads the install-route library dialog only from eligible intent", async () => {
    const { default: AddLibraryDialog, preloadAddLibraryDialog } = await import(
      "./AddLibraryDialogDynamic"
    );
    const loader = latestLoader();
    expect(await loader.importModule()).toHaveProperty("default");
    const view = render(<AddLibraryDialog workspaceId="ws_1" />);
    const trigger = screen.getByRole("button", {
      name: CREATE_FLEET_LIBRARY_LABEL,
    });

    preloadAddLibraryDialog();
    trigger.focus();
    await userEvent.hover(trigger);
    expect(loader.preload).toHaveBeenCalledTimes(3);

    state.maySpeculate = false;
    await userEvent.unhover(trigger);
    await userEvent.hover(trigger);
    expect(loader.preload).toHaveBeenCalledTimes(3);

    await userEvent.click(trigger);
    loader.snapshot = { error: null, module: null, status: "loading" };
    view.rerender(<AddLibraryDialog workspaceId="ws_1" />);
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getByRole("status")).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: CREATE_FLEET_LIBRARY_LABEL }),
    ).toBeNull();

    showError(loader);
    view.rerender(
      <AddLibraryDialog workspaceId="ws_1" triggerLabel="Add another" />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: RETRY_LABEL }),
    );
    expect(loader.retry).toHaveBeenCalledOnce();

    showLoaded(loader);
    view.rerender(
      <AddLibraryDialog workspaceId="ws_1" triggerLabel="Add another" />,
    );
    expect(screen.getByRole("button", { name: "loaded dialog" })).toBeTruthy();
    expect(state.renderedProps).toMatchObject({
      defaultOpen: true,
      triggerLabel: "Add another",
      workspaceId: "ws_1",
    });
  });
});
