import { SOURCE_URL, VERIFIED_AT } from "@/tests/helpers/catalogue-fixtures";
import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { LibraryModel } from "@/lib/api/model-library-types";
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
  catalogueState: { models: [] as LibraryModel[], status: "ready" as CatalogueStatus, preload: vi.fn(), preloadAdvanced: vi.fn() },
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

const cap = (id: string, provider: string, displayName = id): LibraryModel => ({
  id,
  provider,
  display_name: displayName,
  advanced: false,
  context_cap_tokens: 1,
  input_nanos_per_mtok: "1",
  cached_input_nanos_per_mtok: "1",
  output_nanos_per_mtok: "1",
  context_source_url: SOURCE_URL, context_verified_at: VERIFIED_AT, prices: [],
});

beforeEach(() => {
  vi.clearAllMocks();
  catalogueState.models = [];
  catalogueState.status = CATALOGUE_STATUS.ready;
});
afterEach(() => cleanup());

describe("ProviderModelSelect", () => {
  it.each(["gpt-6.1-sol", "gpt-5.6-terra"])("requests advanced entries explicitly and hides them again for %s", (model) => {
    const terra = { ...cap("gpt-5.6-terra", "openai", "GPT-5.6 Terra"), advanced: true };
    catalogueState.models = [cap("gpt-6.1-sol", "openai"), terra];
    const onModelChange = vi.fn();
    render(<ProviderModelSelect id="advanced-model" provider="openai" model={model} onModelChange={onModelChange} />);
    expect(screen.queryByRole("option", { name: /Terra/ })).toBeNull();
    const toggle = screen.getByRole("switch", { name: "Show advanced models" });
    fireEvent.click(toggle);
    expect(catalogueState.preloadAdvanced).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("option", { name: /Terra/ })).toBeTruthy();
    fireEvent.click(toggle);
    expect(screen.queryByRole("option", { name: /Terra/ })).toBeNull();
    if (model === terra.id) expect(onModelChange).toHaveBeenCalledWith("");
    else expect(onModelChange).not.toHaveBeenCalled();
  });

  it("offers a retry when the requested advanced catalogue fails", () => {
    catalogueState.status = CATALOGUE_STATUS.error;
    render(<ProviderModelSelect id="advanced-error" model="" onModelChange={vi.fn()} />);
    fireEvent.click(screen.getByRole("switch", { name: "Show advanced models" }));
    expect(screen.getByRole("alert").textContent).toContain("Could not load advanced models.");
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(catalogueState.preloadAdvanced).toHaveBeenCalledTimes(2);
  });

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
    expect(screen.getByRole("option", { name: "a1" })).toBeTruthy();
    expect(screen.queryByRole("option", { name: "o1" })).toBeNull();
    // Custom label is applied.
    expect(screen.getByLabelText("Pick model")).toBeTruthy();
  });

  it("a picked model reads as one truncating line, and each option lists its id under the name", () => {
    catalogueState.models = [cap("claude-sonnet-5", "anthropic", "Sonnet 5"), cap("openai/gpt-5-mini", "openai", "GPT-5 Mini")];
    render(React.createElement(ProviderModelSelect, { id: "m", model: "claude-sonnet-5", onModelChange: vi.fn() }));

    const value = document.querySelector("[data-select-value]");
    expect(value?.textContent).toBe("Sonnet 5 claude-sonnet-5");
    expect(value?.className).toContain("min-w-0");
    expect(value?.className).toContain("truncate");
    const option = screen.getByRole("option", { name: "GPT-5 Mini openai/gpt-5-mini" });
    expect(option.querySelector(".flex-col")?.lastElementChild?.textContent).toBe("openai/gpt-5-mini");
  });

  it("renders a provider-agnostic option list when no provider is given", () => {
    catalogueState.models = [cap("a1", "anthropic"), cap("o1", "openai")];
    render(
      React.createElement(ProviderModelSelect, { id: "m", model: "", onModelChange: vi.fn() }),
    );
    expect(screen.getByText("a1")).toBeTruthy();
    expect(screen.getByText("o1")).toBeTruthy();
  });

  it("does not invent model suggestions when the catalogue has no rows for the provider", () => {
    catalogueState.models = [];
    render(
      React.createElement(ProviderModelSelect, {
        id: "m",
        provider: "anthropic",
        model: "",
        onModelChange: vi.fn(),
      }),
    );
    expect(screen.queryByText("claude-sonnet-5-5")).toBeNull();
    expect(screen.getByRole("textbox")).toBeTruthy();
  });

  it("allows an exact custom identifier for a provider absent from the catalogue", () => {
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
