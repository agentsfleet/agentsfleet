import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

const getModelLibraryActionMock = vi.hoisted(() => vi.fn());
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  getModelLibraryAction: getModelLibraryActionMock,
}));

import {
  ModelCatalogueProvider,
  useModelCatalogue,
} from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider";

const model = (id: string, provider: string) => ({
  id,
  provider,
  context_cap_tokens: 1,
  input_nanos_per_mtok: 1,
  cached_input_nanos_per_mtok: 1,
  output_nanos_per_mtok: 1,
});

const okLibrary = (models: ReturnType<typeof model>[]) => ({
  ok: true as const,
  data: { version: "1", models },
});

function Probe() {
  const { models, loading, error } = useModelCatalogue();
  return React.createElement(
    "div",
    null,
    React.createElement("span", { "data-testid": "loading" }, String(loading)),
    React.createElement("span", { "data-testid": "error" }, String(error)),
    React.createElement("span", { "data-testid": "models" }, models.map((m) => m.id).join(",")),
  );
}

beforeEach(() => vi.clearAllMocks());
afterEach(() => cleanup());

describe("ModelCatalogueProvider", () => {
  it("fetches the library once on mount through the Server Action and provides the models", async () => {
    getModelLibraryActionMock.mockResolvedValue(okLibrary([model("m1", "anthropic"), model("m2", "openai")]));
    render(
      React.createElement(ModelCatalogueProvider, null, React.createElement(Probe)),
    );
    await waitFor(() => expect(screen.getByTestId("loading").textContent).toBe("false"));
    expect(screen.getByTestId("error").textContent).toBe("false");
    expect(screen.getByTestId("models").textContent).toBe("m1,m2");
    expect(getModelLibraryActionMock).toHaveBeenCalledTimes(1);
  });

  it("degrades to error=true / empty models when the action reports failure (auth/network mapped to ok:false)", async () => {
    getModelLibraryActionMock.mockResolvedValue({ ok: false, error: "Service Unavailable", status: 503 });
    render(
      React.createElement(ModelCatalogueProvider, null, React.createElement(Probe)),
    );
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("true"));
    expect(screen.getByTestId("loading").textContent).toBe("false");
    expect(screen.getByTestId("models").textContent).toBe("");
  });

  it("degrades to error=true / empty models when the action call itself rejects", async () => {
    getModelLibraryActionMock.mockRejectedValue(new Error("network"));
    render(
      React.createElement(ModelCatalogueProvider, null, React.createElement(Probe)),
    );
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("true"));
    expect(screen.getByTestId("loading").textContent).toBe("false");
    expect(screen.getByTestId("models").textContent).toBe("");
  });

  it("ignores a resolved fetch after unmount (no state update on a dead component)", async () => {
    // A deferred resolve that lands after the effect cleanup ran (active=false):
    // the `if (!active)` guard in .then must skip the setState.
    let resolveLibrary!: (v: unknown) => void;
    getModelLibraryActionMock.mockReturnValue(new Promise((r) => (resolveLibrary = r)));
    const { unmount } = render(
      React.createElement(ModelCatalogueProvider, null, React.createElement(Probe)),
    );
    unmount();
    await act(async () => {
      resolveLibrary(okLibrary([model("late", "anthropic")]));
    });
    // No throw / act warning means the guarded branch held.
    expect(getModelLibraryActionMock).toHaveBeenCalledTimes(1);
  });

  it("ignores a rejected fetch after unmount (no state update on a dead component)", async () => {
    let rejectLibrary!: (e: unknown) => void;
    getModelLibraryActionMock.mockReturnValue(new Promise((_r, rej) => (rejectLibrary = rej)));
    const { unmount } = render(
      React.createElement(ModelCatalogueProvider, null, React.createElement(Probe)),
    );
    unmount();
    await act(async () => {
      rejectLibrary(new Error("late-503"));
    });
    expect(getModelLibraryActionMock).toHaveBeenCalledTimes(1);
  });
});

describe("useModelCatalogue outside a provider", () => {
  it("returns the safe degraded fallback state", () => {
    render(React.createElement(Probe));
    // No provider mounted → the context default fires: not loading, error true,
    // empty models — pickers fall back to free-text entry.
    expect(screen.getByTestId("loading").textContent).toBe("false");
    expect(screen.getByTestId("error").textContent).toBe("true");
    expect(screen.getByTestId("models").textContent).toBe("");
    expect(getModelLibraryActionMock).not.toHaveBeenCalled();
  });
});
