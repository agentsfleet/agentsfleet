import { describe, expect, it } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { entry, registry, renderTable, renderTableWithLibrary } from "./helpers/models-registry-fixtures";
import { clickRowAction } from "./helpers/models-registry-edit-fixtures";

async function details(model = "claude-sonnet-5") {
  await clickRowAction(userEvent.setup(), model, "Model details");
  return within(await screen.findByRole("dialog"));
}

describe("model details retain truthful context and prices", () => {
  it("keeps the platform card compact and names its configured model", async () => {
    await renderTable(registry([], true, {provider: "anthropic", model: "claude-sonnet-5", display_name: "Sonnet 5", context_cap_tokens: 200000}));
    expect(within(screen.getByLabelText("Platform default")).getByText(/Sonnet 5/)).toBeTruthy();
  });

  it("does not invent a platform identity when none is configured", async () => {
    await renderTable(registry([], false));
    expect(screen.getByText("No platform default is configured.")).toBeTruthy();
  });

  it("reads catalogue pricing in details without adding columns to the list", async () => {
    await renderTableWithLibrary(registry([entry({context_cap_tokens: 200000})]));
    expect(screen.queryByText("3.00 / 0.30 / 15.00")).toBeNull();
    const dialog = await details();
    expect(dialog.getByText("200,000")).toBeTruthy();
    expect(dialog.getByText("3.00 / 0.30 / 15.00")).toBeTruthy();
  });

  it("shows the server's registered rates even when the catalogue is unavailable", async () => {
    await renderTable(registry([entry({context_cap_tokens: 32000, input_nanos_per_mtok: "3000000000", cached_input_nanos_per_mtok: "300000000", output_nanos_per_mtok: "15000000000"})]));
    const dialog = await details();
    expect(dialog.getByText("32,000")).toBeTruthy();
    expect(dialog.getByText("3.00 / 0.30 / 15.00")).toBeTruthy();
  });

  it.each([{cap: undefined, shown: "—"}, {cap: 0, shown: "0"}, {cap: 500, shown: "500"}])("distinguishes absent, zero and small context caps ($shown)", async ({cap, shown}) => {
    await renderTable(registry([entry({context_cap_tokens: cap})]));
    const dialog = await details();
    expect(dialog.getByText(shown)).toBeTruthy();
    expect(dialog.getByText("Unknown")).toBeTruthy();
    expect(dialog.queryByText("0.00 / 0.00 / 0.00")).toBeNull();
  });

  it("keeps endpoint and missing credential information in details", async () => {
    await renderTable(registry([entry({provider: "openai-compatible", base_url: "https://vllm.corp/v1", has_key: false})]));
    const dialog = await details();
    expect(dialog.getByText("Key unavailable")).toBeTruthy();
    expect(dialog.getByText("https://vllm.corp/v1")).toBeTruthy();
  });

  it("shows unknown provider without guessing one from the model name", async () => {
    await renderTable(registry([entry({provider: undefined})]));
    const dialog = await details();
    expect(dialog.getByText("Provider").parentElement?.textContent).toContain("Unknown");
  });
});
