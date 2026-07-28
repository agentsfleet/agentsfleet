import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

const getModelLibraryActionMock = vi.hoisted(() => vi.fn());
const routerPushMock = vi.hoisted(() => vi.fn());
// Stable router instance — Next's real useRouter returns a stable object, and
// `preload` is memoised against it; a per-render mock object would churn the
// callback identity for no reason.
const routerMock = vi.hoisted(() => ({ push: routerPushMock }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  getModelLibraryAction: getModelLibraryActionMock,
}));
vi.mock("next/navigation", () => ({ useRouter: () => routerMock }));

import { CATALOGUE_STATUS } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/catalogue-status";
import {
  maySpeculateOnHover,
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
  const { models, status, preload } = useModelCatalogue();
  return React.createElement(
    "div",
    null,
    React.createElement("span", { "data-testid": "status" }, status),
    React.createElement("span", { "data-testid": "models" }, models.map((m) => m.id).join(",")),
    React.createElement("button", { "data-testid": "preload", onClick: preload }, "preload"),
  );
}

function renderProvider() {
  return render(React.createElement(ModelCatalogueProvider, null, React.createElement(Probe)));
}

function fireIntent() {
  return act(async () => {
    screen.getByTestId("preload").click();
  });
}

/** Install a matchMedia whose `(pointer: coarse)` answer is ours to choose. */
function stubPointer(coarse: boolean) {
  vi.stubGlobal(
    "matchMedia",
    vi.fn((q: string) => ({ matches: coarse && q.includes("coarse"), media: q })),
  );
}

beforeEach(() => vi.clearAllMocks());
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("ModelCatalogueProvider — intent loading", () => {
  it("does not fetch the catalogue on mount", async () => {
    renderProvider();
    // The whole point of the change: an ordinary visit to the Models page
    // pays nothing for a catalogue it may never consult.
    expect(getModelLibraryActionMock).not.toHaveBeenCalled();
    expect(screen.getByTestId("status").textContent).toBe(CATALOGUE_STATUS.idle);
  });

  it("fetches once on intent and provides the models", async () => {
    getModelLibraryActionMock.mockResolvedValue(okLibrary([model("m1", "anthropic"), model("m2", "openai")]));
    renderProvider();
    await fireIntent();
    await waitFor(() => expect(screen.getByTestId("status").textContent).toBe(CATALOGUE_STATUS.ready));
    expect(screen.getByTestId("models").textContent).toBe("m1,m2");
    expect(getModelLibraryActionMock).toHaveBeenCalledTimes(1);
  });

  it("coalesces a burst of intents into one request", async () => {
    // Hover, focus, and click all fire within one gesture. Without the
    // single-flight guard a deliberate click costs three catalogue reads.
    getModelLibraryActionMock.mockResolvedValue(okLibrary([model("m1", "anthropic")]));
    renderProvider();
    await act(async () => {
      const button = screen.getByTestId("preload");
      button.click();
      button.click();
      button.click();
    });
    await waitFor(() => expect(screen.getByTestId("status").textContent).toBe(CATALOGUE_STATUS.ready));
    expect(getModelLibraryActionMock).toHaveBeenCalledTimes(1);
  });

  it("does not refetch once ready", async () => {
    getModelLibraryActionMock.mockResolvedValue(okLibrary([model("m1", "anthropic")]));
    renderProvider();
    await fireIntent();
    await waitFor(() => expect(screen.getByTestId("status").textContent).toBe(CATALOGUE_STATUS.ready));
    await fireIntent();
    expect(getModelLibraryActionMock).toHaveBeenCalledTimes(1);
  });

  it("test_model_picker_prefetch_policy_and_latest_result — latest request wins when an earlier resolves late", async () => {
    // Latest-wins. A slow first attempt that errors must not stamp `error`
    // over a later attempt that already succeeded.
    let failFirst!: (e: unknown) => void;
    getModelLibraryActionMock
      .mockReturnValueOnce(new Promise((_r, rej) => (failFirst = rej)))
      .mockResolvedValueOnce(okLibrary([model("fresh", "anthropic")]));

    renderProvider();
    await fireIntent();
    // First attempt settles (rejects) — that frees the single-flight guard.
    await act(async () => {
      failFirst(new Error("slow-503"));
    });
    await waitFor(() => expect(screen.getByTestId("status").textContent).toBe(CATALOGUE_STATUS.error));

    await fireIntent();
    await waitFor(() => expect(screen.getByTestId("status").textContent).toBe(CATALOGUE_STATUS.ready));
    expect(screen.getByTestId("models").textContent).toBe("fresh");
  });

  it("degrades to error / empty models on a non-auth failure", async () => {
    getModelLibraryActionMock.mockResolvedValue({ ok: false, error: "Service Unavailable", status: 503 });
    renderProvider();
    await fireIntent();
    await waitFor(() => expect(screen.getByTestId("status").textContent).toBe(CATALOGUE_STATUS.error));
    expect(screen.getByTestId("models").textContent).toBe("");
    expect(routerPushMock).not.toHaveBeenCalled();
  });

  it("routes to sign-in on a 401 — an expired session is not a catalogue outage", async () => {
    getModelLibraryActionMock.mockResolvedValue({ ok: false, error: "Not authenticated", status: 401 });
    renderProvider();
    await fireIntent();
    await waitFor(() => expect(routerPushMock).toHaveBeenCalledWith("/sign-in"));
    // No free-text degrade: the user leaves for sign-in instead of being
    // handed silent manual model-id inputs.
    expect(screen.getByTestId("status").textContent).not.toBe(CATALOGUE_STATUS.error);
  });

  it("degrades to error when the action call itself rejects", async () => {
    getModelLibraryActionMock.mockRejectedValue(new Error("network"));
    renderProvider();
    await fireIntent();
    await waitFor(() => expect(screen.getByTestId("status").textContent).toBe(CATALOGUE_STATUS.error));
    expect(screen.getByTestId("models").textContent).toBe("");
  });
});

describe("maySpeculateOnHover — prefetch policy", () => {
  it("allows speculation on a fine pointer with no Save-Data", () => {
    stubPointer(false);
    vi.stubGlobal("navigator", { connection: { saveData: false } });
    expect(maySpeculateOnHover()).toBe(true);
  });

  it("blocks speculation on a coarse pointer", () => {
    // A touch that lands on a control is already a press, so "hover" prefetch
    // there is an unconditional fetch wearing a different name.
    stubPointer(true);
    vi.stubGlobal("navigator", { connection: { saveData: false } });
    expect(maySpeculateOnHover()).toBe(false);
  });

  it("blocks speculation under Save-Data", () => {
    stubPointer(false);
    vi.stubGlobal("navigator", { connection: { saveData: true } });
    expect(maySpeculateOnHover()).toBe(false);
  });

  it("allows speculation when the connection API is absent", () => {
    // Absence of the hint is not a request to conserve — Safari and Firefox
    // do not implement it, and treating that as Save-Data would disable
    // prefetch for most desktop users.
    stubPointer(false);
    vi.stubGlobal("navigator", {});
    expect(maySpeculateOnHover()).toBe(true);
  });
});

describe("maySpeculateOnHover — environments without matchMedia", () => {
  it("still speculates when the environment has no matchMedia at all", () => {
    // The pointer probe is a `typeof` check rather than an optional chain
    // because the property is typed non-nullish; an environment that omits it
    // must fall through to the Save-Data question, not throw or refuse.
    vi.stubGlobal("matchMedia", undefined);
    expect(maySpeculateOnHover()).toBe(true);
  });
});

describe("useModelCatalogue outside a provider", () => {
  it("returns the safe degraded fallback state", () => {
    render(React.createElement(Probe));
    // No provider mounted → the context default fires: error status, empty
    // models — pickers fall back to free-text entry, and preload is a no-op.
    expect(screen.getByTestId("status").textContent).toBe(CATALOGUE_STATUS.error);
    expect(screen.getByTestId("models").textContent).toBe("");
    expect(getModelLibraryActionMock).not.toHaveBeenCalled();
  });
});
