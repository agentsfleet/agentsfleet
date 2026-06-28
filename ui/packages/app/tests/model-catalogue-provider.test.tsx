import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

const getModelCapsMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/api/model_caps", () => ({ getModelCaps: getModelCapsMock }));

import {
  ModelCatalogueProvider,
  useModelCatalogue,
} from "@/app/(dashboard)/settings/models/components/ModelCatalogueProvider";

const cap = (id: string, provider: string) => ({
  id,
  provider,
  context_cap_tokens: 1,
  input_nanos_per_mtok: 1,
  cached_input_nanos_per_mtok: 1,
  output_nanos_per_mtok: 1,
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
  it("fetches the catalogue once on mount and provides the models", async () => {
    getModelCapsMock.mockResolvedValue({
      version: "1",
      models: [cap("m1", "anthropic"), cap("m2", "openai")],
      rates: { run_nanos_per_sec: 0, event_nanos: 0 },
      billing: { starter_credit_nanos: 0, free_trial_end_ms: 0, free_trial_stage_nanos: 0 },
    });
    render(
      React.createElement(ModelCatalogueProvider, null, React.createElement(Probe)),
    );
    await waitFor(() => expect(screen.getByTestId("loading").textContent).toBe("false"));
    expect(screen.getByTestId("error").textContent).toBe("false");
    expect(screen.getByTestId("models").textContent).toBe("m1,m2");
    expect(getModelCapsMock).toHaveBeenCalledTimes(1);
  });

  it("degrades to error=true / empty models when the fetch rejects", async () => {
    getModelCapsMock.mockRejectedValue(new Error("503"));
    render(
      React.createElement(ModelCatalogueProvider, null, React.createElement(Probe)),
    );
    await waitFor(() => expect(screen.getByTestId("error").textContent).toBe("true"));
    expect(screen.getByTestId("loading").textContent).toBe("false");
    expect(screen.getByTestId("models").textContent).toBe("");
  });

  it("ignores a resolved fetch after unmount (no state update on a dead component)", async () => {
    // A deferred resolve that lands after the effect cleanup ran (active=false):
    // the `if (active)` guard in .then must skip the setState.
    let resolveCaps!: (v: unknown) => void;
    getModelCapsMock.mockReturnValue(new Promise((r) => (resolveCaps = r)));
    const { unmount } = render(
      React.createElement(ModelCatalogueProvider, null, React.createElement(Probe)),
    );
    unmount();
    await act(async () => {
      resolveCaps({
        version: "1",
        models: [cap("late", "anthropic")],
        rates: { run_nanos_per_sec: 0, event_nanos: 0 },
        billing: { starter_credit_nanos: 0, free_trial_end_ms: 0, free_trial_stage_nanos: 0 },
      });
    });
    // No throw / act warning means the guarded branch held.
    expect(getModelCapsMock).toHaveBeenCalledTimes(1);
  });

  it("ignores a rejected fetch after unmount (no state update on a dead component)", async () => {
    let rejectCaps!: (e: unknown) => void;
    getModelCapsMock.mockReturnValue(new Promise((_r, rej) => (rejectCaps = rej)));
    const { unmount } = render(
      React.createElement(ModelCatalogueProvider, null, React.createElement(Probe)),
    );
    unmount();
    await act(async () => {
      rejectCaps(new Error("late-503"));
    });
    expect(getModelCapsMock).toHaveBeenCalledTimes(1);
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
    expect(getModelCapsMock).not.toHaveBeenCalled();
  });
});
