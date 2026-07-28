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
  }: React.PropsWithChildren) {
    return ReactModule.createElement("p", null, children);
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
  useRouter: () => ({ refresh: vi.fn() }),
}));

vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: vi.fn(),
}));

function loadedProbe(props: Record<string, unknown>) {
  state.renderedProps = props;
  return React.createElement(
    "button",
    {
      onClick: () => {
        (props.onOpenChange as ((open: boolean) => void) | undefined)?.(false);
        (props.onCreated as ((value: unknown) => void) | undefined)?.({
          id: "created",
        });
        (props.onUpdated as ((value: unknown) => void) | undefined)?.({
          id: "updated",
        });
        (props.onSaved as ((value: unknown) => void) | undefined)?.({
          id: "saved",
        });
      },
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

describe("model-library intent dialogs", () => {
  it("preserves the create trigger through loading and retry", async () => {
    const { default: AddModelDialog, preloadAddModelDialog } = await import(
      "./AddModelDialogDynamic"
    );
    const loader = latestLoader();
    expect(await loader.importModule()).toHaveProperty("default");
    const onCreated = vi.fn();
    const view = render(<AddModelDialog onCreated={onCreated} />);
    const trigger = screen.getByRole("button", {
      name: "Create model library",
    });

    preloadAddModelDialog();
    trigger.focus();
    await userEvent.hover(trigger);
    expect(loader.preload).toHaveBeenCalledTimes(3);
    state.maySpeculate = false;
    await userEvent.unhover(trigger);
    await userEvent.hover(trigger);
    expect(loader.preload).toHaveBeenCalledTimes(3);

    await userEvent.click(trigger);
    loader.snapshot = { error: null, module: null, status: "loading" };
    view.rerender(<AddModelDialog onCreated={onCreated} />);
    expect(trigger.getAttribute("aria-busy")).toBe("true");

    showError(loader);
    view.rerender(<AddModelDialog onCreated={onCreated} />);
    await userEvent.click(
      screen.getByRole("button", { name: "Retry create model library" }),
    );
    expect(loader.retry).toHaveBeenCalledOnce();

    showLoaded(loader);
    view.rerender(<AddModelDialog onCreated={onCreated} />);
    await userEvent.click(screen.getByRole("button", { name: "loaded dialog" }));
    expect(state.renderedProps?.defaultOpen).toBe(true);
    expect(onCreated).toHaveBeenCalledWith({ id: "created" });
  });

  it("preloads the editor, retries failure, and forwards callbacks", async () => {
    const { default: EditModelDialog, preloadEditModelDialog } = await import(
      "./EditModelDialogDynamic"
    );
    const loader = latestLoader();
    expect(await loader.importModule()).toHaveProperty("default");
    const onOpenChange = vi.fn();
    const onUpdated = vi.fn();
    const model = { id: "model_1" } as never;
    const view = render(
      <EditModelDialog
        model={model}
        onOpenChange={onOpenChange}
        onUpdated={onUpdated}
      />,
    );
    expect(screen.getByText("Loading model editor…")).toBeTruthy();
    expect(loader.preload).toHaveBeenCalledOnce();
    preloadEditModelDialog();
    expect(loader.preload).toHaveBeenCalledTimes(2);

    showError(loader);
    view.rerender(
      <EditModelDialog
        model={model}
        onOpenChange={onOpenChange}
        onUpdated={onUpdated}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(loader.retry).toHaveBeenCalledOnce();

    showLoaded(loader);
    view.rerender(
      <EditModelDialog
        model={model}
        onOpenChange={onOpenChange}
        onUpdated={onUpdated}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "loaded dialog" }));
    expect(state.renderedProps?.model).toBe(model);
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onUpdated).toHaveBeenCalledWith({ id: "updated" });
  });
});

describe("fleet-library editor intent dialog", () => {
  it("covers closed/open loading, failure, retry, and save forwarding", async () => {
    const { default: EditFleetDialog, preloadEditFleetDialog } = await import(
      "./EditFleetDialogDynamic"
    );
    const loader = latestLoader();
    expect(await loader.importModule()).toHaveProperty("default");
    const onOpenChange = vi.fn();
    const onSaved = vi.fn();
    const entry = { id: "fleet_1" } as never;
    const view = render(
      <EditFleetDialog
        entry={entry}
        open={false}
        onOpenChange={onOpenChange}
        onSaved={onSaved}
      />,
    );
    expect(
      screen.queryByText("Loading fleet library editor…"),
    ).toBeNull();
    expect(loader.preload).not.toHaveBeenCalled();
    preloadEditFleetDialog();

    view.rerender(
      <EditFleetDialog
        entry={entry}
        open
        onOpenChange={onOpenChange}
        onSaved={onSaved}
      />,
    );
    expect(screen.getByText("Loading fleet library editor…")).toBeTruthy();
    expect(loader.preload).toHaveBeenCalledTimes(2);

    showError(loader);
    view.rerender(
      <EditFleetDialog
        entry={entry}
        open
        onOpenChange={onOpenChange}
        onSaved={onSaved}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(loader.retry).toHaveBeenCalledOnce();

    showLoaded(loader);
    view.rerender(
      <EditFleetDialog
        entry={entry}
        open
        onOpenChange={onOpenChange}
        onSaved={onSaved}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "loaded dialog" }));
    expect(state.renderedProps).toMatchObject({ entry, open: true });
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onSaved).toHaveBeenCalledWith({ id: "saved" });
  });
});
