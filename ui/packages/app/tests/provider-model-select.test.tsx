import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ModelCap } from "@/lib/api/model_caps";

// Catalogue-backed model picker. With a catalogue it constrains to a <Select>
// (provider-scoped or provider-agnostic); when empty it degrades to a free-text
// <Input>. The catalogue comes from useModelCatalogue, mocked here so the two
// branches are deterministic.

const { catalogueState } = vi.hoisted(() => ({
  catalogueState: { models: [] as ModelCap[], loading: false, error: false },
}));

vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider", () => ({
  useModelCatalogue: () => catalogueState,
}));
vi.mock("@agentsfleet/design-system", async () => (await import("./helpers/models-component-mocks")).designSystemStub());

import ProviderModelSelect from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderModelSelect";

const cap = (id: string, provider: string): ModelCap => ({
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
  catalogueState.loading = false;
  catalogueState.error = false;
});
afterEach(() => cleanup());

describe("ProviderModelSelect", () => {
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
});
