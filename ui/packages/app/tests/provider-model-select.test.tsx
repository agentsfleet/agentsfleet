import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { LibraryModel } from "@/lib/api/model_library";
import {
  CATALOGUE_STATUS,
  type CatalogueStatus,
} from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/catalogue-status";

// Catalogue-backed model picker. With a catalogue it constrains to a <Select>
// (provider-scoped or provider-agnostic); when empty it degrades to a free-text
// <Input>; while the catalogue is IN FLIGHT it holds a disabled select rather
// than letting the empty array pick the input shape. The catalogue comes from
// useModelCatalogue, mocked here so every branch is deterministic.

const { catalogueState } = vi.hoisted(() => ({
  catalogueState: { models: [] as LibraryModel[], status: "ready" as CatalogueStatus, preload: vi.fn() },
}));

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider", () => ({
  useModelCatalogue: () => catalogueState,
  // These suites exercise form shape, not prefetch policy — that has its own
  // suite in model-catalogue-provider.test.tsx. Hover speculation is off so a
  // stray pointer event cannot perturb the call counts asserted below.
  maySpeculateOnHover: () => false,
}));
vi.mock("@agentsfleet/design-system", async () => (await import("./helpers/models-component-mocks")).designSystemStub());

import ProviderModelSelect from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderModelSelect";

const cap = (id: string, provider: string): LibraryModel => ({
  id,
  provider,
  context_cap_tokens: 1,
  input_nanos_per_mtok: 1,
  cached_input_nanos_per_mtok: 1,
  output_nanos_per_mtok: 1,
});

beforeEach(() => {
  vi.clearAllMocks();
  catalogueState.models = [];
  catalogueState.status = CATALOGUE_STATUS.ready;
});
afterEach(() => cleanup());

describe("ProviderModelSelect", () => {
  it("holds a stable, disabled select while the catalogue is in flight", () => {
    // The catalogue loads on dialog-open intent, so a cold open renders this
    // state for the round-trip. It must NOT be the free-text input: swapping
    // control identity when the catalogue lands drops focus and visually
    // orphans anything already typed.
    catalogueState.status = CATALOGUE_STATUS.loading;
    render(React.createElement(ProviderModelSelect, { id: "m", model: "", onModelChange: vi.fn() }));
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByText("Loading models…")).toBeTruthy();
  });

  it("degrades to a free-text input when the catalogue is empty, firing onModelChange", () => {
    const onModelChange = vi.fn();
    render(
      React.createElement(ProviderModelSelect, { id: "m", model: "", onModelChange }),
    );
    const input = screen.getByLabelText("Model") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "claude-x" } });
    expect(onModelChange).toHaveBeenCalledWith("claude-x");
  });

  it("renders a provider-scoped select when the catalogue has matching models", () => {
    catalogueState.models = [cap("a1", "anthropic"), cap("o1", "openai")];
    render(
      React.createElement(ProviderModelSelect, {
        id: "m",
        provider: "anthropic",
        model: "a1",
        onModelChange: vi.fn(),
        label: "Pick model",
      }),
    );
    // Provider-scoped → only the anthropic model is an option.
    expect(screen.getByText("a1")).toBeTruthy();
    expect(screen.queryByText("o1")).toBeNull();
    // Custom label is applied.
    expect(screen.getByLabelText("Pick model")).toBeTruthy();
  });

  it("renders a provider-agnostic option list when no provider is given", () => {
    catalogueState.models = [cap("a1", "anthropic"), cap("o1", "openai")];
    render(
      React.createElement(ProviderModelSelect, { id: "m", model: "", onModelChange: vi.fn() }),
    );
    expect(screen.getByText("a1")).toBeTruthy();
    expect(screen.getByText("o1")).toBeTruthy();
  });

  it("falls back to the static known-models list before free text when the catalogue has no rows for the provider", () => {
    catalogueState.models = []; // empty catalogue — provider isn't priced yet
    render(
      React.createElement(ProviderModelSelect, {
        id: "m",
        provider: "anthropic",
        model: "",
        onModelChange: vi.fn(),
      }),
    );
    expect(screen.getByText("claude-sonnet-5")).toBeTruthy();
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("still degrades to free text when the provider is in neither the catalogue nor the static list (regression)", () => {
    catalogueState.models = [];
    const onModelChange = vi.fn();
    render(
      React.createElement(ProviderModelSelect, {
        id: "m",
        provider: "some-uncatalogued-provider",
        model: "",
        onModelChange,
      }),
    );
    const input = screen.getByLabelText("Model") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "custom-model" } });
    expect(onModelChange).toHaveBeenCalledWith("custom-model");
  });
});
